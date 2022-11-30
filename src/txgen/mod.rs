pub mod worker;

use log::info;

use crossbeam::channel::{unbounded, Receiver, Sender, TryRecvError};
use rand::Rng;
use ring::signature::{KeyPair, Ed25519KeyPair};
use core::time;
use std::thread;

use crate::network::{self, server};
use crate::types::address::Address;
use crate::types::block::{Block, Header, Content};
use crate::blockchain::{Blockchain, Mempool};
use crate::types::hash::Hashable;
use crate::types::key_pair;
use crate::types::transaction::{SignedTransaction, sign, Transaction};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

enum ControlSignal {
    Start(u64), // the number controls the theta of interval between block generation
    Update, // update the block in mining, it may due to new blockchain tip or new transaction
    Exit,
}

enum OperatingState {
    Paused,
    Run(u64),
    ShutDown,
}

// context for transaction
pub struct Context {
    /// Channel for receiving control signal
    arc_mutex: Arc<Mutex<Blockchain>>, 
    control_chan: Receiver<ControlSignal>,
    operating_state: OperatingState,
    finished_tx_chan: Sender<SignedTransaction>,
    mempool: Arc<Mutex<Mempool>>,
    key_pairs: Vec<Ed25519KeyPair>,
}

#[derive(Clone)]
pub struct Handle {
    /// Channel for sending signal to the miner thread
    control_chan: Sender<ControlSignal>,
}

pub fn new(blockchain: &Arc<Mutex<Blockchain>>, mempool: &Arc<Mutex<Mempool>>, seed: u8) -> (Context, Handle, Receiver<SignedTransaction>) {
    println!("creating a new txgen");
    let (signal_chan_sender, signal_chan_receiver) = unbounded();
    let (finished_tx_sender, finished_tx_receiver) = unbounded();
    let mut key_pairs = Vec::new();
    let genesis_keypair = Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap();
    key_pairs.push(genesis_keypair); // initialize the key pairs to genesis hash
    println!("genesis address generated");

    let ctx = Context {
        arc_mutex: Arc::clone(blockchain),
        control_chan: signal_chan_receiver,
        operating_state: OperatingState::Paused,
        finished_tx_chan: finished_tx_sender,
        mempool: Arc::clone(mempool),
        key_pairs,
    };

    let txgen_handle = Handle {
        control_chan: signal_chan_sender,
    };

    (ctx, txgen_handle, finished_tx_receiver) 
}

#[cfg(any(test,test_utilities))]
fn test_new() -> (Context, Handle, Receiver<SignedTransaction>) {
    let new_blockchain= &Arc::new(Mutex::new(Blockchain::new(0)));
    let new_mempool = &Arc::new(Mutex::new(Mempool::new()));
    new(new_blockchain, new_mempool, 0)
}

impl Handle {
    pub fn exit(&self) {
        self.control_chan.send(ControlSignal::Exit).unwrap();
    }

    pub fn start(&self, theta: u64) {
        self.control_chan
            .send(ControlSignal::Start(theta))
            .unwrap();
    }

    pub fn update(&self) {
        self.control_chan.send(ControlSignal::Update).unwrap();
    }
}

impl Context {
    pub fn start(mut self) {
        thread::Builder::new()
            .name("txgen".to_string())
            .spawn(move || {
                self.tx_loop();
            })
            .unwrap();
        info!("Txgen initialized into paused mode");
    }

    fn tx_loop(&mut self) {
        // main tx gening loop
        loop {
            // check and react to control signals
            match self.operating_state {
                OperatingState::Paused => {
                    let signal = self.control_chan.recv().unwrap();
                    match signal {
                        ControlSignal::Exit => {
                            info!("Miner shutting down");
                            self.operating_state = OperatingState::ShutDown;
                        }
                        ControlSignal::Start(i) => {
                            info!("Miner starting in continuous mode with theta {}", i);
                            self.operating_state = OperatingState::Run(i);
                        }
                        ControlSignal::Update => {
                            // in paused state, don't need to update
                        }
                    };
                    continue;
                }
                OperatingState::ShutDown => {
                    return;
                }
                _ => match self.control_chan.try_recv() {
                    Ok(signal) => {
                        match signal {
                            ControlSignal::Exit => {
                                info!("Tx Generator shutting down");
                                self.operating_state = OperatingState::ShutDown;
                            }
                            ControlSignal::Start(i) => {
                                info!("Tx Generatir starting in continuous mode with theta {}", i);
                                self.operating_state = OperatingState::Run(i);
                            }
                            ControlSignal::Update => {
                                unimplemented!()
                            }
                        };
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => panic!("Tx Generator control channel detached"),
                },
            }
            if let OperatingState::ShutDown = self.operating_state {
                return;
            }
            println!("txgen is running");

            // dynamically generate sender addresses by setting the probability of creating a new keypair as 0.5
            let probability = rand::random::<bool>();
            if probability {
                // insert the new key pair into the key pairs controlled by this node
                self.key_pairs.push(key_pair::random());
                // update the state to create a new account for this new key pair
                let new_address = Address::from_public_key_bytes(self.key_pairs[self.key_pairs.len() - 1].public_key().as_ref().to_vec().as_slice());
                // instead of inserting it to the state here, set it as the recipient of the transaction for it to be inserted in network worker
                println!("new key pair generated {}", new_address);
            }
            
            // pick a random recipent address from the state
            let tip = {self.arc_mutex.lock().unwrap().tip()};
            let all_accounts = {self.arc_mutex.lock().unwrap().state_map.get(&tip).unwrap().clone()};
            let mut receiver = all_accounts.state.keys().nth(rand::random::<usize>() % all_accounts.state.len()).unwrap().clone();

            // if a new key pair was generated, set it as the recipient address for this transaction
            if probability {
                receiver = Address::from_public_key_bytes(self.key_pairs[self.key_pairs.len() - 1].public_key().as_ref().to_vec().as_slice()).clone();
            }
            // pick a random key pair as the sender
            let mut sender_index = rand::thread_rng().gen_range(0..self.key_pairs.len());
            // if a new key pair was generated, avoid assigning it as the sender
            if probability {
                // sender_index = rand::random::<usize>() % self.key_pairs.len()-1;
                sender_index = rand::thread_rng().gen_range(0..self.key_pairs.len()-1);
            }

            // only pick a sender that has a balance greater than 0 and is contained in the state
            let sender = Address::from_public_key_bytes(self.key_pairs[sender_index].public_key().as_ref().to_vec().as_slice()).clone();
            if all_accounts.state.contains_key(&sender) && all_accounts.state.get(&sender).unwrap().1 > 0{
                // println!("sender address {}", sender);
                // println!("state length {}", all_accounts.state.len());
                // println!("state contains sender {}", all_accounts.state.contains_key(&sender));
    
                let account_nonce = {all_accounts.state.get(&sender).unwrap().0 + 1};
                let value = {rand::thread_rng().gen_range(0..all_accounts.state.get(&sender).unwrap().1)};
                let new_transaction = Transaction {
                    receiver,
                    value,
                    account_nonce,
                };
    
                let signature_vector = crate::types::transaction::sign(&new_transaction, &self.key_pairs[sender_index]);
                let signed_transaction = SignedTransaction {
                    t: new_transaction,
                    signature_vector: signature_vector.as_ref().to_vec(),
                    signer_public_key: self.key_pairs[sender_index].public_key().as_ref().to_vec(),
                };
                let signed_hash = signed_transaction.hash();
    
                let signed_transaction_clone = signed_transaction.clone();
    
                // add the transaction to mempool and pass it on to the worker for broadcasting
                {self.mempool.lock().unwrap().hash_map.insert(signed_hash, signed_transaction)};
                self.finished_tx_chan.send(signed_transaction_clone).unwrap();
                print!("new transaction generated: {}", signed_hash);
            }
   

            if let OperatingState::Run(i) = self.operating_state {
                if i != 0 {
                    let interval = time::Duration::from_micros(i*10000 as u64);
                    thread::sleep(interval);
                }
            }
        }
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST