use crate::types::address::Address;
use crate::types::block::{Block,generate_random_block, generate_genesis_block};
use crate::types::hash::{H256, Hashable};
use crate::types::key_pair;
use crate::types::transaction::SignedTransaction;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread::current;
use hex_literal::hex;
use ring::signature::{Ed25519KeyPair, KeyPair};
use url::quirks::port;

pub struct Blockchain {
    pub hash_map: HashMap<H256, Block>,
    tip: H256,
    pub state_map:HashMap<H256, State> // state per block
}

#[derive(Clone)]
pub struct State {
    pub state: HashMap<Address, (usize, usize)> // mapping from account address to (account nonce, balance)
}


#[derive(Clone)]
pub struct Mempool {
    pub hash_map: HashMap<H256, SignedTransaction>,
}

impl Blockchain {
    /// Create a new blockchain, only containing the genesis block
    pub fn new(seed: u8) -> Self {
        let parent: [u8; 32] = [0; 32];
        let parent_hash = H256::from(parent);
        let genesis_block: Block = generate_genesis_block(&parent_hash);
        let genesis_hash = genesis_block.hash();
        let mut hash_map: HashMap <H256, Block> = HashMap::new();
        hash_map.insert(genesis_hash, genesis_block);

        // create 3 determinstic key pairs based on the seed fed by port IP
        let first_address = Address::from_public_key_bytes(Ed25519KeyPair::from_seed_unchecked(&[0; 32]).unwrap().public_key().as_ref());
        let second_address = Address::from_public_key_bytes(Ed25519KeyPair::from_seed_unchecked(&[1; 32]).unwrap().public_key().as_ref());
        let third_address = Address::from_public_key_bytes(Ed25519KeyPair::from_seed_unchecked(&[2; 32]).unwrap().public_key().as_ref());
        
        // initialize a new state that contains 3 address, nonce, and balance
        let mut state = HashMap::new();
        state.insert(first_address, (0, 100));
        state.insert(second_address, (0, 0));
        state.insert(third_address, (0, 0));

        // insert the new state into the state map per genesis block
        let mut state_map = HashMap::new();
        state_map.insert(genesis_hash, State {state});

        Blockchain { hash_map, tip: genesis_hash, state_map }
    }

    /// Insert a block into blockchain
    pub fn insert(&mut self, block: &Block) {
        let new_block = block.clone(); 
        println!(" pass the length test?: {}", new_block.header.length > self.hash_map.get(&self.tip()).unwrap().header.length);
        if new_block.header.length > self.hash_map.get(&self.tip()).unwrap().header.length {
            println!("is tip same as parent?: {}", new_block.get_parent() == self.tip());
            
            self.hash_map.insert(block.hash(), new_block);
            self.tip = block.hash();
            let mut state_copy = self.state_map.get(&block.get_parent()).unwrap().clone();

            for transaction in &block.content.transactions {
                // update the state of the sender
                let receiver = transaction.t.receiver;
                let sender = Address::from_public_key_bytes(transaction.signer_public_key.as_slice());
                let tx_amount = transaction.t.value;

                let new_nonce = state_copy.state.get(&sender).unwrap().0 + 1;
                let new_balance = state_copy.state.get(&sender).unwrap().1 - tx_amount;

                state_copy.state.insert(sender, (new_nonce, new_balance));

                if state_copy.state.contains_key(&receiver) {
                    let rec_nonce = state_copy.state.get(&receiver).unwrap().0;
                    let rec_balance = state_copy.state.get(&receiver).unwrap().1 + tx_amount;
                    state_copy.state.insert(receiver, (rec_nonce, rec_balance));
                    println!("receiver state updated");
                }
                // create a new entry for the receiver if it does not exist
                else {
                    state_copy.state.insert(receiver, (0, tx_amount));
                    println!("new receiver state created");
                }
            }
            self.state_map.insert(block.hash(), state_copy);
        }
        println!("block is inserted in the blockchain insert() function");
        println!("does blockchain contain the parent in the blockchain mod {}", self.hash_map.contains_key(&block.get_parent()));
    }

    /// Get the last block's hash of the longest chain
    pub fn tip(&self) -> H256 {
        self.tip
    }

    /// Get all blocks' hashes of the longest chain, ordered from genesis to the tip
    pub fn all_blocks_in_longest_chain(&self) -> Vec<H256> {
        let mut blocks = Vec::new();
        let mut current_hash = self.tip;
        let mut current_length = self.hash_map.get(&self.tip).unwrap().header.length;
        print!("{}", current_length);
        while current_length > 0 {
            blocks.push(current_hash);
            current_hash = self.hash_map.get(&current_hash).unwrap().get_parent(); // update the hash
            current_length = self.hash_map.get(&current_hash).unwrap().header.length; // update the length
        }
        blocks.push(current_hash);

        let mut reversed_blocks: Vec<H256> = Vec::new();

        for n in 0..blocks.len() {
            reversed_blocks.push(blocks[blocks.len() - n - 1]);
        }
        reversed_blocks
    }

    /// Get all blocks' hashes of the longest chain, ordered from genesis to the tip
    pub fn all_tx_in_longest_chain(&self) -> Vec<Vec<H256>> {
        let longest_chain = self.all_blocks_in_longest_chain();

        let mut all_txs = Vec::new();
        for n in 0..longest_chain.len() {
            let mut this_block_tx = Vec::new();
            let block = self.hash_map.get(&longest_chain[n]).unwrap();
            for m in 0..block.content.transactions.len() {
                this_block_tx.push(block.content.transactions[m].hash());
            }
            all_txs.push(this_block_tx);
        }
        return all_txs;
    }

}

impl Mempool {
    /// Create a new mempool
    pub fn new() -> Self {
        let hash_map: HashMap <H256, SignedTransaction> = HashMap::new();
        Mempool { hash_map }
    }

    /// Get all transactions in the mempool
    pub fn all_transactions(&self) -> Vec<SignedTransaction> {
        let mut transactions = Vec::new();
        for (_, transaction) in self.hash_map.iter() {
            transactions.push(transaction.clone());
        }
        transactions
    }
}


// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::block::generate_random_block;
    use crate::types::hash::Hashable;

    #[test]
    fn insert_one() {
        let mut blockchain = Blockchain::new(0);
        let genesis_hash = blockchain.tip();
        let block = generate_random_block(&genesis_hash);
        blockchain.insert(&block);
        assert_eq!(blockchain.tip(), block.hash());

    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST