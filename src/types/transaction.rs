use serde::{Serialize,Deserialize}; // declare serde as (serialize, deserialize)
use ring::signature::{Ed25519KeyPair, Signature, self};
use rand::{Rng, RngCore}; // bind rand to Rng
use crate::types::address::Address;

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct Transaction {
    pub receiver: Address,
    pub value: usize,
    pub account_nonce: usize,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct SignedTransaction {
    pub t: Transaction,
    pub signature_vector: Vec<u8>,
    pub signer_public_key: Vec<u8>,
}

/// Create digital signature of a transaction
pub fn sign(t: &Transaction, key: &Ed25519KeyPair) -> Signature {
    // searlized transaction 256 bytes
    let transac = bincode::serialize(t).unwrap(); // unwrap gives a vector
    let trans: &[u8]= transac.as_slice(); // convert to u8
    // Sign the transaction
    key.sign(&trans) // sign the message
}

/// Verify digital signature of a transaction, using public key instead of secret key
pub fn verify(t: &Transaction, public_key: &[u8], signature: &[u8]) -> bool {
    let transac = bincode::serialize(t).unwrap();
    let trans = transac.as_slice();
    let peer_public_key =
        ring::signature::UnparsedPublicKey::new(&signature::ED25519, public_key);
    peer_public_key.verify(trans, signature).is_ok() // verify the mesage
}

// #[cfg(any(test, test_utilities))]
pub fn generate_random_transaction() -> Transaction {

    let mut rng = rand::thread_rng();

    let mut receiver:Vec<u8>= Vec::with_capacity(20);
    rng.fill_bytes(&mut receiver); // generates a 20 bit address
    let receiver_add = Address::from_public_key_bytes(receiver.as_slice());

    Transaction {
        receiver: receiver_add,
        value: rng.gen(),
        account_nonce: rng.gen(),
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. BEFORE TEST

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::key_pair;
    use ring::signature::KeyPair;


    #[test]
    fn sign_verify() {
        let t = generate_random_transaction();
        let key = key_pair::random();
        let signature = sign(&t, &key);
        assert!(verify(&t, key.public_key().as_ref(), signature.as_ref()));
    }
    #[test]
    fn sign_verify_two() {
        let t = generate_random_transaction();
        let key = key_pair::random();
        let signature = sign(&t, &key);
        let key_2 = key_pair::random();
        let t_2 = generate_random_transaction();
        assert!(!verify(&t_2, key.public_key().as_ref(), signature.as_ref()));
        assert!(!verify(&t, key_2.public_key().as_ref(), signature.as_ref()));
    }
}

// DO NOT CHANGE THIS COMMENT, IT IS FOR AUTOGRADER. AFTER TEST

