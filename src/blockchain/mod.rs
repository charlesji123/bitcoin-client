use crate::types::block::{Block,generate_random_block};
use crate::types::hash::{H256, Hashable};
use std::collections::HashMap;

pub struct Blockchain {
    hash_map: HashMap<H256, Block>,
    tip: H256,
}

impl Blockchain {
    /// Create a new blockchain, only containing the genesis block
    pub fn new() -> Self {
        let parent: [u8; 32] = [0; 32];
        let parent_hash = H256::from(parent);
        let genesis_block: Block = generate_random_block(&parent_hash);
        let genesis_hash = genesis_block.hash();
        let mut hash_map: HashMap <H256, Block> = HashMap::new();
        hash_map.insert(genesis_hash, genesis_block);
        Blockchain { hash_map, tip: genesis_hash }
    }

    /// Insert a block into blockchain
    pub fn insert(&mut self, block: &Block) {
        let hash = block.hash();
        let mut new_block = block.clone(); 
        new_block.header.length = self.hash_map.get(&new_block.get_parent()).unwrap().header.length + 1;
        if new_block.header.length > self.hash_map.get(&self.tip).unwrap().header.length {
            self.tip = hash;
        }
        self.hash_map.insert(hash, new_block);
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
        while current_length != 0 {
            blocks.push(current_hash);
            current_hash = self.hash_map.get(&current_hash).unwrap().get_parent(); // update the hash
            current_length = self.hash_map.get(&current_hash).unwrap().header.length; // update the length
        }
        blocks.push(current_hash);

        let mut reversed_blocks: Vec<H256> = Vec::new();

        for n in (blocks.len() - 1)..=0 {
            reversed_blocks.push(blocks[n]);
        }
        reversed_blocks
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
        let mut blockchain = Blockchain::new();
        let genesis_hash = blockchain.tip();
        let block = generate_random_block(&genesis_hash);
        blockchain.insert(&block);
        assert_eq!(blockchain.tip(), block.hash());

    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST