pub mod worker;

use log::info;

use crossbeam::channel::{unbounded, Receiver, Sender, TryRecvError};
use smol::stream::TryNextFuture;
use std::convert::TryInto;
use std::time;

use std::thread;

use crate::network;
use crate::network::message::Message;
use crate::types::address::Address;
use crate::types::block::{Block, Header, Content};
use crate::blockchain::{Blockchain, Mempool};
use crate::types::hash::Hashable;
use crate::types::transaction::SignedTransaction;
use crate::types::transaction::Transaction;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::types::merkle::MerkleTree;
use rand::Rng;
use crate::network::server::Handle as ServerHandle;

enum ControlSignal {
    Start(u64), // the number controls the lambda of interval between block generation
    Update, // update the block in mining, it may due to new blockchain tip or new transaction
    Exit,
}

enum OperatingState {
    Paused,
    Run(u64),
    ShutDown,
}

pub struct Context {
    /// Channel for receiving control signal
    arc_mutex: Arc<Mutex<Blockchain>>, 
    control_chan: Receiver<ControlSignal>,
    operating_state: OperatingState,
    finished_block_chan: Sender<Block>,
    mempool: Arc<Mutex<Mempool>>,
}

#[derive(Clone)]
pub struct Handle {
    /// Channel for sending signal to the miner thread
    control_chan: Sender<ControlSignal>,
}

pub fn new(blockchain: &Arc<Mutex<Blockchain>>, mempool: &Arc<Mutex<Mempool>>) -> (Context, Handle, Receiver<Block>) {
    let (signal_chan_sender, signal_chan_receiver) = unbounded();
    let (finished_block_sender, finished_block_receiver) = unbounded();

    let ctx = Context {
        arc_mutex: Arc::clone(blockchain),
        control_chan: signal_chan_receiver,
        operating_state: OperatingState::Paused,
        finished_block_chan: finished_block_sender,
        mempool: Arc::clone(mempool),
    };

    let handle = Handle {
        control_chan: signal_chan_sender,
    };

    (ctx, handle, finished_block_receiver) // this handle can be used either for miner or transaction
}

#[cfg(any(test,test_utilities))]
fn test_new() -> (Context, Handle, Receiver<Block>) {
    let new_blockchain= &Arc::new(Mutex::new(Blockchain::new(0)));
    let new_mempool = &Arc::new(Mutex::new(Mempool::new()));
    new(new_blockchain, new_mempool)
}

impl Handle {
    pub fn exit(&self) {
        self.control_chan.send(ControlSignal::Exit).unwrap();
    }

    pub fn start(&self, lambda: u64) {
        self.control_chan
            .send(ControlSignal::Start(lambda))
            .unwrap();
    }

    pub fn update(&self) {
        self.control_chan.send(ControlSignal::Update).unwrap();
    }
}

impl Context {
    pub fn start(mut self) {
        thread::Builder::new()
            .name("miner".to_string())
            .spawn(move || {
                self.miner_loop();
            })
            .unwrap();
        info!("Miner initialized into paused mode");
    }

    fn miner_loop(&mut self) {
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
                            info!("Miner starting in continuous mode with lambda {}", i);
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
                                info!("Miner starting in continuous mode with lambda {}", i);
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
            
            let tip = {self.arc_mutex.lock().unwrap().tip().clone()};
            let state_copy = {self.arc_mutex.lock().unwrap().state_map.get(&tip).unwrap().clone()};

            let mut this_block_transactions= Vec::new();
            // if the block is consistent with the difficulty of the blockchain, insert the block into the blockchain
            // update the block's transactions based on the mempool before inserting into the blockchain
            let mut count = 0;
            let mut included_senders = Vec::new();

            // println!("mempool length: {}", {self.mempool.lock().unwrap().hash_map.len()});
            for (hash, transaction) in self.mempool.lock().unwrap().hash_map.clone() {
            
                let mut transaction_is_valid = true;

                let sender = Address::from_public_key_bytes(transaction.signer_public_key.as_slice());
                // println!("do we already include the same sender: {}", included_senders.contains(&sender));
                if included_senders.contains(&sender) {
                    continue;
                }
                let tx_amount = transaction.t.value;
                let tx_nonce = transaction.t.account_nonce;
                // update the state and append the transaction if the transaction is valid
                if state_copy.state.contains_key(&sender) {
                    if tx_amount > state_copy.state.get(&sender).unwrap().1 || tx_nonce != state_copy.state.get(&sender).unwrap().0 + 1{
                        transaction_is_valid = false;
                    }
                }
                else {
                    transaction_is_valid = false;
                }
            
                // only append transaction if it is valid
                if transaction_is_valid {
                    included_senders.push(sender);
                    this_block_transactions.push(transaction);
                    count += 1;
                    if count >= 16 {
                        break;
                    }
                } 
            }                    
            // println!("# transaction for this block is: {}",count);

            // After initializing the transactions, initialize timestap, difficulty, content, merkle root, and nonce
            let parent = tip;
            let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
            let difficulty = {self.arc_mutex.lock().unwrap().hash_map.get(&parent).unwrap().get_difficulty()};

            let merkle_tree = MerkleTree::new(&this_block_transactions);
            let merkle_root = merkle_tree.root(); // hash of the block is the hash of the merkle root

            let mut rng = rand::thread_rng();
            let nonce: usize = rng.gen();

            let length = {self.arc_mutex.lock().unwrap().hash_map.get(&parent).unwrap().header.length + 1};

            let header = Header {
                parent,
                nonce,
                difficulty,
                timestamp,
                merkle_root,
                length,
            };

            let content = Content {
                transactions: this_block_transactions.to_vec(),
            };
            
            let block = Block {header, content};
            

            if block.hash() <= difficulty && count > 0 {            
                println!(" is block's parent same as tip? {}", block.get_parent() == {self.arc_mutex.lock().unwrap().tip().clone()});

                // only remove the transactions from the mempool after the block is passed through
                for transactions in block.content.transactions.clone() {
                    {self.mempool.lock().unwrap().hash_map.remove(&transactions.hash())};
                }

                {self.arc_mutex.lock().unwrap().insert(&block)}; 
                println!(" new block inserted");

                self.finished_block_chan.send(block.clone()).expect("Send finished block error");

                // After successful block insertion, state changes, so implement Transaction Mempool Update to purify the mempool
                // let tip = {self.arc_mutex.lock().unwrap().tip().clone()};
                // println!("length of the blockchain's state hashmap is {}", {self.arc_mutex.lock().unwrap().state_map.len()});
                // print!(" number of accounts in the tip's state is {} ", {self.arc_mutex.lock().unwrap().state_map.get(&tip).unwrap().state.len()});
                // if {self.arc_mutex.lock().unwrap().state_map.get(&tip).unwrap().state.len()} > 0 {
                //     let state_copy = {self.arc_mutex.lock().unwrap().state_map.get(&tip).unwrap().clone()};
                //     for (hash, signed_transaction) in {self.mempool.lock().unwrap().hash_map.clone()} {
                //         let sender = Address::from_public_key_bytes(signed_transaction.signer_public_key.as_slice());
                //         let amount = signed_transaction.t.value;
                //         // remove the tx if the sender's balance is less than the amount
                //         if state_copy.state.contains_key(&sender) {
                //             if amount > state_copy.state.get(&sender).unwrap().1 {
                //                 self.mempool.lock().unwrap().hash_map.remove(&hash);
                //             }
                //         }
                //         // or if the state map does not contain the sender
                //         else {
                //             self.mempool.lock().unwrap().hash_map.remove(&hash);
                //         }
                //     }
                // }
            }

            if let OperatingState::Run(i) = self.operating_state {
                if i != 0 {
                    let interval = time::Duration::from_micros(i as u64);
                    thread::sleep(interval);
                }
            }
        }
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod test {
    use ntest::timeout;
    use crate::types::hash::Hashable;

    #[test]
    #[timeout(60000)]
    fn miner_three_block() {
        let (miner_ctx, miner_handle, finished_block_chan) = super::test_new();
        miner_ctx.start();
        miner_handle.start(0);
        let mut block_prev = finished_block_chan.recv().unwrap();
        for _ in 0..2 {
            let block_next = finished_block_chan.recv().unwrap();
            assert_eq!(block_prev.hash(), block_next.get_parent());
            block_prev = block_next;
        }
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST