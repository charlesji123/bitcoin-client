use super::message::Message;
use super::peer;
use super::server::Handle as ServerHandle;
use crate::types::address::Address;
use crate::types::block::Block;
use crate::types::hash::{H256, Hashable};
use crate::blockchain::{Blockchain, Mempool, State};
use crate::types::transaction::{Transaction, SignedTransaction, sign};
use std::collections::HashMap;
use std::convert::{TryInto, TryFrom};
use std::io::{self, Write};
use std::thread::{self, current};
use std::sync::{Arc, Mutex};
use ring::signature::{Ed25519KeyPair, Signature, self};

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
    wrapped_blockchain: Arc<Mutex<Blockchain>>, 
    wrapped_mempool: Arc<Mutex<Mempool>>,
}

#[derive(Clone)]
pub struct OrphanBuffer {
    pub hash_map: HashMap<H256, Block>,
}

impl Worker {
    pub fn new(
        num_worker: usize,
        msg_src: smol::channel::Receiver<(Vec<u8>, peer::Handle)>,
        server: &ServerHandle,
        wrapped_blockchain: &Arc<Mutex<Blockchain>>, 
        wrapped_mempool: &Arc<Mutex<Mempool>>, 
    ) -> Self {
        Self {
            msg_chan: msg_src,
            num_worker,
            server: server.clone(),
            wrapped_blockchain: wrapped_blockchain.clone(),
            wrapped_mempool: wrapped_mempool.clone()
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
                    println!("newblockhashes msg with length hashvec: {}", hashvec.len());
                    let mut new_hashes = Vec::<H256>::new();
                    for hash in hashvec {
                        print!(" this hash is {} ", hash);
                        print!("block hashmap length {}", {self.wrapped_blockchain.lock().unwrap().hash_map.len()});
                        println!(" blockchain contains hash: {}", {self.wrapped_blockchain.lock().unwrap().hash_map.contains_key(&hash)});
                        if !self.wrapped_blockchain.lock().unwrap().hash_map.contains_key(&hash) {
                            new_hashes.push(hash);
                        }
                        println!("for loop ends");
                    }
                    println!("new_hashes length: {:?}", new_hashes.len());
                    if new_hashes.len() > 0 {
                        peer.write(Message::GetBlocks(new_hashes));
                        println!("sent getblocks msg");
                    }
                }
                Message::GetBlocks(hashvec) => {
                    println!("Getblocks msg");
                    let mut blocks = Vec::new();
                    
                    for hash in hashvec {
                        if self.wrapped_blockchain.lock().unwrap().hash_map.contains_key(&hash){ 
                            let blockchain_local = {self.wrapped_blockchain.lock().unwrap()};
                            let block_response = blockchain_local.hash_map.get(&hash);
                            let block_option = Option::expect(block_response, "block not found");
                            blocks.push(block_option.clone());
                            println!("block hash in GetBlocks: {}", block_option.hash());
                        } 
                    }
                    if blocks.len() > 0 {
                        peer.write(Message::Blocks(blocks));
                    }
                }

                Message::Blocks(blockvec) => {
                    println!("Blocks in message blocks {:?}", blockvec);
                    let mut new_hashes = Vec::<H256>::new();
                    let mut parent_vec = Vec::new();
                    // Check the block before inserting the block into blockchain
                    for block in blockvec {
                        println!("does blockchain contain the parent in network {}", {self.wrapped_blockchain.lock().unwrap().hash_map.contains_key(&block.get_parent())});
                        // Check if the block passed POW difficulty check
                        let pow_passed = block.hash() <= block.get_difficulty();
                        
                        // Check if transactions in a block are valid
                        let block_clone = block.clone(); 
                        let signed_transactions = block_clone.content.transactions;

                        // After updating the mempool, proceed to insert the block
                        // If the blockchain does not already contain the block
                        println!("does blockchain not contain the block: {}", !{self.wrapped_blockchain.lock().unwrap().hash_map.contains_key(&block.hash())});
                        println!("does the block pass the pow test: {}", pow_passed);
                        if !{self.wrapped_blockchain.lock().unwrap().hash_map.contains_key(&block.hash())} && pow_passed {
                            // let current_blockchain = {self.wrapped_blockchain.lock().unwrap()};
                            
                            println!("stage 0");
                            // But contains the block's parent, add the block to the blockchain and remove the block's transactions from the mempool
                            if self.wrapped_blockchain.lock().unwrap().hash_map.contains_key(&block.get_parent()) {
                                println!("stage 1");
                                // get the state of the blockchain tip based on the block's parent
                                let parent = block.get_parent();
                                let state_copy = {self.wrapped_blockchain.lock().unwrap().state_map.get(&parent).unwrap().clone()};
                                
                                let mut all_transactions_valid = true;
                                // Check the block's transactions - if any transaction if invalid, skip the entire block
                                for signed_transaction in signed_transactions {
                                    // by first checking if transaction signature is valid
                                    if !verify(&signed_transaction.t, &signed_transaction.signature_vector, &signed_transaction.signer_public_key) {
                                        all_transactions_valid = false;
                                        break;
                                    }
                                    if Address::from_public_key_bytes(signed_transaction.signer_public_key.as_slice()) != signed_transaction.t.receiver {
                                        all_transactions_valid = false;
                                        break;
                                    }

                                    let sender = Address::from_public_key_bytes(signed_transaction.signer_public_key.as_slice());
                                    let amount = signed_transaction.t.value;
                                    let nonce = signed_transaction.t.account_nonce;
                                    
                                    println!("does the state agree with the tx {}", state_copy.state.contains_key(&sender));
                                    // check if the state agrees with the validity of the transaction
                                    if state_copy.state.contains_key(&sender) {
                                        // spending check
                                        if amount > state_copy.state.get(&sender).unwrap().1 || nonce != state_copy.state.get(&sender).unwrap().0 + 1{
                                            all_transactions_valid = false;
                                            break;
                                        }
                                    }
                                    else {
                                        all_transactions_valid = false;
                                        break
                                    }
                                }
                                println!("all transactions valid: {}", all_transactions_valid);

                                if all_transactions_valid {
                                    {self.wrapped_blockchain.lock().unwrap().insert(&block.clone())};
                                    new_hashes.push(block.hash()); 

                                    // remove the block's transactions from the mempool after inserting the block to the blockchain
                                    let transactions = block.clone().content.transactions;
                                    for signed_transaction in transactions {
                                        if self.wrapped_mempool.lock().unwrap().hash_map.contains_key(&signed_transaction.hash()) {
                                            self.wrapped_mempool.lock().unwrap().hash_map.remove(&signed_transaction.hash());
                                        }
                                    }

                                    // After inserting the block, update the mempool based on the new tip (Transaction Mempool Update)
                                    let new_tip = {self.wrapped_blockchain.lock().unwrap().tip()};
                                    let new_state_copy = {self.wrapped_blockchain.lock().unwrap().state_map.get(&new_tip).unwrap().clone()};
                                    for (hash, signed_transaction) in self.wrapped_mempool.lock().unwrap().hash_map.clone() {
                                        let sender = Address::from_public_key_bytes(signed_transaction.signer_public_key.as_slice());
                                        let tx_nonce = signed_transaction.t.account_nonce;
                                        if new_state_copy.state.contains_key(&sender) {
                                            let nonce = new_state_copy.state.get(&sender).unwrap().0;
                                            if tx_nonce < nonce {
                                                self.wrapped_mempool.lock().unwrap().hash_map.remove(&hash);
                                            }
                                        }
                                    }
                                    println!("stage 3");
                                }
                                // if a block contains at least one invalid transaction, skip the entire block
                                else {
                                    continue;
                                }
                            }

                            // if the new block is the parent of any block in the buffer
                            let mut parent_hash = block.hash();
                            while orphanbuffer.hash_map.contains_key(&parent_hash) {
                                
                                let removed_hash = parent_hash; // the hash to be removed from the buffer
                                let selected_block = orphanbuffer.hash_map.get(&parent_hash);
                                let selected_block_option = Option::expect(selected_block, "block not found");
                                {self.wrapped_blockchain.lock().unwrap().insert(&selected_block_option.clone())}; // add the block to your blockchain
                                new_hashes.push(selected_block_option.clone().hash());

                                parent_hash = selected_block_option.clone().hash(); // update the hash for next round
                                orphanbuffer.hash_map.remove(&removed_hash); // remove the block from the buffer
                            }
                            println!("stage 4");
                        }
                        // if the blockchain already contains the block, add the repeated block to the orphan buffer
                        else if pow_passed {
                            let parent_hash = block.get_parent();
                            orphanbuffer.hash_map.insert(parent_hash, block); // if the parent does not exist, add the block to the buffer
                            parent_vec.push(parent_hash);
                        }
                    }

                    if parent_vec.len() > 0 {
                        peer.write(Message::GetBlocks(parent_vec));
                        print!(" new parent vector is sent ");
                    }
                    else {
                        print!(" there is no parent vector to get blocks ");
                    }
                    if new_hashes.len() > 0 {
                        peer.write(Message::NewBlockHashes(new_hashes));
                        print!(" new block hashes are sent with length ");
                    }
                    else {
                        print!(" there is no new block hashes to send ");
                    }
                }
                
                Message::NewTransactionHashes(trans_hashes) => {
                    println!("get transaction hashes in message newtransactionhases");
                    let mut get_hashes = Vec::<H256>::new();
                    // for all the transaction hashes in the message
                    for hash in trans_hashes {
                        // if the transaction is not in the mempool, add it to the mempool
                        if !self.wrapped_mempool.lock().unwrap().hash_map.contains_key(&hash) {
                            get_hashes.push(hash);
                        }
                    }
                    // broadcast the new transaction hashes that are newly added to the mempool
                    if get_hashes.len() > 0 {
                        peer.write(Message::GetTransactions(get_hashes));
                    }
                }
                Message::GetTransactions(trans_vec) => {
                    let mut transactions = Vec::new();
                    println!("get transaction in message gettransactions");
                    
                    for hash in trans_vec {
                        if self.wrapped_mempool.lock().unwrap().hash_map.contains_key(&hash){ 
                            let mempool = {self.wrapped_mempool.lock().unwrap()};
                            let transaction = mempool.hash_map.get(&hash);
                            let transaction_option = Option::expect(transaction, "block not found");
                            transactions.push(transaction_option.clone());
                        } 
                    }
                    if transactions.len() > 0 {
                        peer.write(Message::Transactions(transactions));
                        
                    }
                }
                Message::Transactions(signed_transactions) => {
                    println!("received transactions in message transactions");
                    let mut new_hashes = Vec::<H256>::new();

                    // retrive the trasnactions of the hashes from the mempool, and check their validity
                    for signed_transaction in signed_transactions {
                        let mut signature_is_valid = true;
                        println!("transaction is received");
                        // first, check transaction signature validity
                        if !verify(&signed_transaction.t, &signed_transaction.signer_public_key, &signed_transaction.signature_vector) {
                            signature_is_valid = false;
                        }
                        println!("verify {}", signature_is_valid);

                        // spending check
                        // let amount = signed_transaction.t.value;
                        // let nonce = signed_transaction.t.account_nonce;
                        // // if the amount is larger than the balance or the nonce is not 1 + account nonce , the transaction is invalid
                        // if amount > state_option.state.get(&sender).unwrap().1 || nonce != state_option.state.get(&sender).unwrap().0 + 1{
                        //     signature_is_valid = false;
                        // }
                        // println!("spending check {}", signature_is_valid);

                        // if the transaction is not in the mempool, add it to the mempool
                        if !{self.wrapped_mempool.lock().unwrap().hash_map.contains_key(&signed_transaction.hash())} && signature_is_valid {
                            new_hashes.push(signed_transaction.hash());
                            {self.wrapped_mempool.lock().unwrap().hash_map.insert(signed_transaction.hash(), signed_transaction)};
                            println!("transaction inserted into the mempool!");
                        }
                        else {
                            println!("transaction already exists in the mempool!");
                        }
                    }
                    println!("new_hashes length is {}", new_hashes.len());
                    if new_hashes.len() > 0 {
                        self.server.broadcast(Message::NewTransactionHashes(new_hashes));  
                        println!("new transaction hashes are broadcasted ");
                    }
                }
            }
            println!("end of the loop");
        }
    }
}

// reimplement the verify function here
pub fn verify(t: &Transaction, public_key: &[u8], signature: &[u8]) -> bool {
    let transac = bincode::serialize(t).unwrap();
    let trans = transac.as_slice();
    let peer_public_key =
        ring::signature::UnparsedPublicKey::new(&signature::ED25519, public_key);
    peer_public_key.verify(trans, signature).is_ok() // verify the mesage
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
    let new_blockchain= &Arc::new(Mutex::new(Blockchain::new(0)));
    let new_mempool = &Arc::new(Mutex::new(Mempool::new()));
    let worker = Worker::new(1, msg_chan, &server, new_blockchain, new_mempool);
    worker.start(); 
    // generate and append the hash of the genesis block
    let blockchain_vector = {new_blockchain.lock().unwrap().all_blocks_in_longest_chain()};
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
        print!("this is v: {} ", v.last().unwrap());
        let random_block = generate_random_block(v.last().unwrap());
        let mut _peer_receiver = test_msg_sender.send(Message::Blocks(vec![random_block.clone()]));
        let reply = server_receiver.recv().unwrap();
        print!(" this is hash random block generted by v: {} ", random_block.hash());
        if let Message::NewBlockHashes(v) = reply {
            assert_eq!(v, vec![random_block.hash()]);
        } else {
            panic!();
        }
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST