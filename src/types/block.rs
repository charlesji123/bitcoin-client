use serde::{Serialize, Deserialize};
use crate::types::hash::{H256, Hashable};
use crate::types::transaction::SignedTransaction;
use rand::{thread_rng, Rng};
use crate::types::merkle::MerkleTree;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Header {
    pub parent: H256,
    pub nonce: u32,
    pub difficulty: H256,
    pub timestamp: u128,
    pub merkle_root: H256,
    pub length: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Content {
    pub transactions: Vec<SignedTransaction>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Block {
    pub header: Header,
    pub content: Content,
}

impl Hashable for SignedTransaction {
    fn hash(&self) -> H256 {
        let transac = bincode::serialize(self).unwrap();
        let transac_bytes = transac.as_slice();
        ring::digest::digest(&ring::digest::SHA256, &transac_bytes).into()
    }
}

impl Hashable for Header {
    fn hash(&self) -> H256 {
        let header = bincode::serialize(self).unwrap();
        let header_bytes = header.as_slice();
        ring::digest::digest(&ring::digest::SHA256, header_bytes).into()
    }
}

impl Hashable for Block {
    fn hash(&self) -> H256 {
        self.header.hash()
    }
}

impl Block {
    pub fn get_parent(&self) -> H256 {
        self.header.parent
    }

    pub fn get_difficulty(&self) -> H256 {
        self.header.difficulty
    }
}

// #[cfg(any(test, test_utilities))]
pub fn generate_random_block(parent: &H256) -> Block {
    // generate a random integer for nounce
    let mut rng = rand::thread_rng();

    // generate a merkle tree
    let data: [H256; 0] = [];
    let merkle_tree = MerkleTree::new(&data);
    let merkle_root = merkle_tree.root();
    let difficulty = [255u8; 32].into(); // set maximum difficulty
  
    let header = Header {
        parent: *parent,
        nonce: rng.gen(),
        difficulty,
        timestamp: rng.gen(),
        merkle_root,
        length: 0,
    };
    
    let content = Content {transactions: Vec::new()};
    Block {header, content}
}