use super::message::Message;
use super::peer;
use super::server::Handle as ServerHandle;
use crate::types::block::Block;
use crate::types::hash::{H256, Hashable};
use crate::blockchain::Blockchain;
use std::collections::HashMap;
use std::thread;
use std::sync::{Arc, Mutex};

use log::{debug, warn, error};

#[cfg(any(test,test_utilities))]
use super::peer::TestReceiver as PeerTestReceiver;
#[cfg(any(test,test_utilities))]
use super::server::TestReceiver as ServerTestReceiver;
#[derive(Clone)]
pub struct Worker {
    msg_chan: smol::channel::Receiver<(Vec<u8>, peer::Handle)>,
    num_worker: usize,
    server: ServerHandle,
    arc_mutex: Arc<Mutex<Blockchain>>, 
}

#[derive(Clone)]
pub struct OrphanBuffer {
    // orphan_buffer: Vec<Block>,
    pub hash_map: HashMap<H256, Block>,
}

impl Worker {
    pub fn new(
        num_worker: usize,
        msg_src: smol::channel::Receiver<(Vec<u8>, peer::Handle)>,
        server: &ServerHandle,
        arc_mutex: &Arc<Mutex<Blockchain>>, 
    ) -> Self {
        Self {
            msg_chan: msg_src,
            num_worker,
            server: server.clone(),
            arc_mutex: arc_mutex.clone()
        }
    }

    pub fn start(self) {
        let num_worker = self.num_worker;
        for i in 0..num_worker {
            let cloned = self.clone();
            thread::spawn(move || {
                cloned.worker_loop();
                warn!("Worker thread {} exited", i);
            });
        }
    }

    fn worker_loop(&self) {
        let mut orphanbuffer = OrphanBuffer {
            hash_map: HashMap::new(),
        };

        loop {
            let result = smol::block_on(self.msg_chan.recv());
            if let Err(e) = result {
                error!("network worker terminated {}", e);
                break;
            }

            let msg = result.unwrap();
            let (msg, mut peer) = msg;
            let msg: Message = bincode::deserialize(&msg).unwrap();
            match msg {
                Message::Ping(nonce) => {
                    debug!("Ping: {}", nonce);
                    peer.write(Message::Pong(nonce.to_string()));
                }
                Message::Pong(nonce) => {
                    debug!("Pong: {}", nonce);
                }
                Message::NewBlockHashes(hashvec) => {
                    let mut new_hashes = Vec::<H256>::new();
                    for hash in hashvec {
                        if !&self.arc_mutex.lock().unwrap().hash_map.contains_key(&hash) {
                            new_hashes.push(hash);
                            print!("{}", hash);
                        }
                    }
                    peer.write(Message::GetBlocks(new_hashes));
                }
                Message::GetBlocks(hashvec) => {
                    let mut blocks = Vec::new();
                    
                    for hash in hashvec {
                        if self.arc_mutex.lock().unwrap().hash_map.contains_key(&hash){ 
                            let blockchain_local = self.arc_mutex.lock().unwrap();
                            let block_response = blockchain_local.hash_map.get(&hash);
                            let block_option = Option::expect(block_response, "block not found");
                            blocks.push(block_option.clone());
                        } 
                    }

                    peer.write(Message::Blocks(blocks));
                }
                Message::Blocks(blockvec) => {
                    let mut new_hashes = Vec::<H256>::new();
                    let mut parent_vec = Vec::new();
                    // Project 6 // 
                    for block in blockvec {
                    // PoW Check
                    assert!(block.hash() <= block.get_difficulty(), "PoW validity check failed");

                    if !self.arc_mutex.lock().unwrap().hash_map.contains_key(&block.hash()) {

                        let blockchain_local = self.arc_mutex.lock().unwrap();
                        // Parent Check
                        if blockchain_local.hash_map.contains_key(&block.get_parent()) {
                            self.arc_mutex.lock().unwrap().insert(&block); // if the parent exists, add the block to your blockchain
                            new_hashes.push(block.hash());
                        }

                        assert!(block.get_parent().hash() == block.get_difficulty(), "Difficulty not consistent");

                        // if the new block is the parent of any block in the buffer
                        let mut parent_hash = block.hash();
                        while orphanbuffer.hash_map.contains_key(&parent_hash) {
                                let removed_hash = parent_hash; // the hash to be removed from the buffer
                                let selected_block = orphanbuffer.hash_map.get(&parent_hash);
                                let selected_block_option = Option::expect(selected_block, "block not found");
                                self.arc_mutex.lock().unwrap().insert(&selected_block_option.clone()); // add the block to your blockchain
                                new_hashes.push(selected_block_option.clone().hash());

                                parent_hash = selected_block_option.clone().hash(); // update the hash for next round
                                orphanbuffer.hash_map.remove(&removed_hash); // remove the block from the buffer
                            }
                        }
                    else {
                        let parent_hash = block.get_parent();
                        orphanbuffer.hash_map.insert(block.get_parent(), block); // if the parent does not exist, add the block to the buffer
                        parent_vec.push(parent_hash);
                    }
                    }
                    peer.write(Message::GetBlocks(parent_vec));
                    self.server.broadcast(Message::NewBlockHashes(new_hashes));  // broadcast the new hashes
                }
                Message::NewTransactionHashes(_) => todo!(),
                Message::GetTransactions(_) => todo!(),
                Message::Transactions(_) => todo!(),
            }
        }
    }
}

#[cfg(any(test,test_utilities))]
struct TestMsgSender {
    s: smol::channel::Sender<(Vec<u8>, peer::Handle)>
}
#[cfg(any(test,test_utilities))]
impl TestMsgSender {
    fn new() -> (TestMsgSender, smol::channel::Receiver<(Vec<u8>, peer::Handle)>) {
        let (s,r) = smol::channel::unbounded();
        (TestMsgSender {s}, r)
    }

    fn send(&self, msg: Message) -> PeerTestReceiver {
        let bytes = bincode::serialize(&msg).unwrap();
        let (handle, r) = peer::Handle::test_handle();
        smol::block_on(self.s.send((bytes, handle))).unwrap();
        r
    }
}
#[cfg(any(test,test_utilities))]
/// returns two structs used by tests, and an ordered vector of hashes of all blocks in the blockchain
fn generate_test_worker_and_start() -> (TestMsgSender, ServerTestReceiver, Vec<H256>) {

    let (server, server_receiver) = ServerHandle::new_for_test();
    let (test_msg_sender, msg_chan) = TestMsgSender::new();
    let new_blockchain= &Arc::new(Mutex::new(Blockchain::new()));
    let worker = Worker::new(1, msg_chan, &server, new_blockchain);
    worker.start(); 
    // generate and append the hash of the genesis block
    let blockchain_vector = new_blockchain.lock().unwrap().all_blocks_in_longest_chain();
    (test_msg_sender, server_receiver, blockchain_vector)
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod test {
    use ntest::timeout;
    use crate::types::block::generate_random_block;
    use crate::types::hash::Hashable;

    use super::super::message::Message;
    use super::generate_test_worker_and_start;

    #[test]
    #[timeout(60000)]
    fn reply_new_block_hashes() {
        let (test_msg_sender, _server_receiver, v) = generate_test_worker_and_start();
        let random_block = generate_random_block(v.last().unwrap());
        let mut peer_receiver = test_msg_sender.send(Message::NewBlockHashes(vec![random_block.hash()]));
        let reply = peer_receiver.recv();
        if let Message::GetBlocks(v) = reply {
            assert_eq!(v, vec![random_block.hash()]);
        } else {
            panic!();
        }
    }
    #[test]
    #[timeout(60000)]
    fn reply_get_blocks() {
        let (test_msg_sender, _server_receiver, v) = generate_test_worker_and_start();
        let h = v.last().unwrap().clone();
        let mut peer_receiver = test_msg_sender.send(Message::GetBlocks(vec![h.clone()]));
        let reply = peer_receiver.recv();
        if let Message::Blocks(v) = reply {
            assert_eq!(1, v.len());
            assert_eq!(h, v[0].hash())
        } else {
            panic!();
        }
    }
    #[test]
    #[timeout(60000)]
    fn reply_blocks() {
        let (test_msg_sender, server_receiver, v) = generate_test_worker_and_start();
        let random_block = generate_random_block(v.last().unwrap());
        let mut _peer_receiver = test_msg_sender.send(Message::Blocks(vec![random_block.clone()]));
        let reply = server_receiver.recv().unwrap();
        if let Message::NewBlockHashes(v) = reply {
            assert_eq!(v, vec![random_block.hash()]);
        } else {
            panic!();
        }
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST