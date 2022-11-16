pub mod worker;

use log::info;

use crossbeam::channel::{unbounded, Receiver, Sender, TryRecvError};
use ring::signature::KeyPair;
use core::time;
use std::thread;

use crate::network::{self, server};
use crate::types::block::{Block, Header, Content};
use crate::blockchain::{Blockchain, Mempool};
use crate::types::hash::Hashable;
use crate::types::transaction::SignedTransaction;
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
}

#[derive(Clone)]
pub struct Handle {
    /// Channel for sending signal to the miner thread
    control_chan: Sender<ControlSignal>,
}

pub fn new(blockchain: &Arc<Mutex<Blockchain>>, mempool: &Arc<Mutex<Mempool>>) -> (Context, Handle, Receiver<SignedTransaction>) {
    let (signal_chan_sender, signal_chan_receiver) = unbounded();
    let (finished_tx_sender, finished_tx_receiver) = unbounded();

    let ctx = Context {
        arc_mutex: Arc::clone(blockchain),
        control_chan: signal_chan_receiver,
        operating_state: OperatingState::Paused,
        finished_tx_chan: finished_tx_sender,
        mempool: Arc::clone(mempool),
    };

    let txgen_handle = Handle {
        control_chan: signal_chan_sender,
    };

    (ctx, txgen_handle, finished_tx_receiver) 
}

#[cfg(any(test,test_utilities))]
fn test_new() -> (Context, Handle, Receiver<SignedTransaction>) {
    let new_blockchain= &Arc::new(Mutex::new(Blockchain::new()));
    let new_mempool = &Arc::new(Mutex::new(Mempool::new()));
    new(new_blockchain, new_mempool)
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
        info!("Miner initialized into paused mode");
    }

    fn tx_loop(&mut self) {
        // main mining loop
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
                                info!("Miner shutting down");
                                self.operating_state = OperatingState::ShutDown;
                            }
                            ControlSignal::Start(i) => {
                                info!("Miner starting in continuous mode with theta {}", i);
                                self.operating_state = OperatingState::Run(i);
                            }
                            ControlSignal::Update => {
                                unimplemented!()
                            }
                        };
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => panic!("Miner control channel detached"),
                },
            }
            if let OperatingState::ShutDown = self.operating_state {
                return;
            }
            
            // generate a random signed transaction
            let random_transaction = crate::types::transaction::generate_random_transaction();
            let signer_public_key = crate::types::key_pair::random();
            let signature_vector = crate::types::transaction::sign(&random_transaction, &signer_public_key);

            let signed_transaction = SignedTransaction {
                t: random_transaction,
                signature_vector: signature_vector.as_ref().to_vec(),
                signer_public_key: signer_public_key.public_key().as_ref().to_vec(),
            };
            let signed_hash = signed_transaction.hash();
            print!("{}", signed_hash);

            let signed_transaction_clone = signed_transaction.clone();
            // add the transaction to mempool and pass it on to the worker for broadcasting
            self.mempool.lock().unwrap().hash_map.insert(signed_hash, signed_transaction);
            self.finished_tx_chan.send(signed_transaction_clone).unwrap();
            print!("new transaction generated: {}", signed_hash);

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