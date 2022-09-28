use crate::miner::Context;
use super::hash::{Hashable, H256};
use ring::digest;

/// A Merkle tree.
#[derive(Debug, Default)]
pub struct MerkleTree {
    merkle: Vec<Vec<H256>>,
}

impl MerkleTree {
    pub fn new<T>(data: &[T]) -> Self where T: Hashable, {
        // &[T] suggests a vector of hashable data type
        let mut merkletree: Vec<Vec<H256>> = Vec::new();

        // initialize the base layer
        let mut base_layer: Vec<H256> = Vec::new();
        for n in 0..data.len(){
            base_layer.push(data[n].hash()); }
        let last_item = base_layer[base_layer.len() - 1];
        if base_layer.len() %2 == 1 {base_layer.push(last_item)}
        merkletree.push(base_layer); // debugging: forgot to push base layer

        let mut layer = 0;
        while merkletree[layer].len() > 1 {
            // collect the accurate hash functions to this layer
            let mut parent_layer: Vec<H256> = Vec::new();
            let mut prev_layer = merkletree[layer].clone();
            
            // duplicate the last item if the layer has an odd number of hashes
            if prev_layer.len() % 2 == 1 {prev_layer.push(prev_layer[prev_layer.len() - 1])}

            for i in 0..prev_layer.len(){
                // hash data pair-wise
                if i % 2 == 0 {
                    let mut ctx = digest::Context::new(&digest::SHA256);
                    ctx.update(prev_layer[i].as_ref());
                    ctx.update(prev_layer[i+1].as_ref());
                    let new_hash = ctx.finish();
                    parent_layer.push(new_hash.into());
                }
            }

            // append this layer to the merkle tree vector
            merkletree.push(parent_layer); 
            layer = layer + 1;
        }
        MerkleTree{merkle: merkletree}
    }
    
    pub fn root(&self) -> H256 {
        self.merkle[self.merkle.len()-1][0]
    }

    /// Returns the Merkle Proof of data at index i
    pub fn proof(&self, index: usize) -> Vec<H256> {
        // leaf sibiling 
        let mut new_index = index;
        let mut prooftree: Vec<H256> = Vec::new();
        for n in 0..self.merkle.len()-1{
            if new_index % 2 == 0 {prooftree.push(self.merkle[n][new_index+1])
            }
            else {prooftree.push(self.merkle[n][index-1])
            }
            new_index = new_index / 2;
        }
        prooftree
    }
}

/// Verify that the datum hash with a vector of proofs will produce the Merkle root. Also need the
/// index of datum and `leaf_size`, the total number of leaves.
pub fn verify(root: &H256, datum: &H256, proof: &[H256], index: usize, leaf_size: usize) -> bool {
    if index >= leaf_size {
        return false;
    }

    let mut proof_sibiling = *datum;
    let mut _num = index;

    for n in 0..proof.len() {
        let mut ctx = digest::Context::new(&digest::SHA256);

        if _num %2 == 1 {
            ctx.update(proof[n].as_ref());
            ctx.update(proof_sibiling.as_ref());
        }
        else {
            ctx.update(proof_sibiling.as_ref());
            ctx.update(proof[n].as_ref());
        }
        let new_hash = ctx.finish();

        proof_sibiling = new_hash.into();

        _num = _num / 2;
    }
    return *root == proof_sibiling;
    
}
// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod tests {
    use crate::types::hash::H256;
    use super::*;

    macro_rules! gen_merkle_tree_data {
        () => {{
            vec![
                (hex!("0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d")).into(),
                (hex!("0101010101010101010101010101010101010101010101010101010101010202")).into(),
            ]
        }};
    }

    #[test]
    fn merkle_root() {
        let input_data: Vec<H256> = gen_merkle_tree_data!();
        let merkle_tree = MerkleTree::new(&input_data);
        let root = merkle_tree.root();
        assert_eq!(
            root,
            (hex!("6b787718210e0b3b608814e04e61fde06d0df794319a12162f287412df3ec920")).into()
        );
        // "b69566be6e1720872f73651d1851a0eae0060a132cf0f64a0ffaea248de6cba0" is the hash of
        // "0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d0a0b0c0d0e0f0e0d"
        // "965b093a75a75895a351786dd7a188515173f6928a8af8c9baa4dcff268a4f0f" is the hash of
        // "0101010101010101010101010101010101010101010101010101010101010202"
        // "6b787718210e0b3b608814e04e61fde06d0df794319a12162f287412df3ec920" is the hash of
        // the concatenation of these two hashes "b69..." and "965..."
        // notice that the order of these two matters
    }

    #[test]
    fn merkle_proof() {
        let input_data: Vec<H256> = gen_merkle_tree_data!();
        let merkle_tree = MerkleTree::new(&input_data);
        let proof = merkle_tree.proof(0);
        assert_eq!(proof,
                   vec![hex!("965b093a75a75895a351786dd7a188515173f6928a8af8c9baa4dcff268a4f0f").into()]
        );
        // "965b093a75a75895a351786dd7a188515173f6928a8af8c9baa4dcff268a4f0f" is the hash of
        // "0101010101010101010101010101010101010101010101010101010101010202"
    }

    #[test]
    fn merkle_verifying() {
        let input_data: Vec<H256> = gen_merkle_tree_data!();
        let merkle_tree = MerkleTree::new(&input_data);
        let proof = merkle_tree.proof(0);
        print!("{}", merkle_tree.merkle.len());
        assert!(verify(&merkle_tree.root(), &input_data[0].hash(), &proof, 0, input_data.len()));
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST