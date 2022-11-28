use crossbeam::channel::{unbounded, Receiver, Sender, TryRecvError};
use log::{debug, info};
use crate::network::message::Message::{NewBlockHashes, self};
use crate::types::block::Block;
use crate::network::server::Handle as ServerHandle;
use crate::blockchain::{Blockchain, Mempool};
use crate::types::hash::Hashable;
use crate::types::transaction::SignedTransaction;
use std::thread;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Worker {
    server: ServerHandle,
    finished_tx_chan: Receiver<SignedTransaction>,
    blockchain: Arc<Mutex<Blockchain>>,
    mempool: Arc<Mutex<Mempool>>, 
}

impl Worker {
    pub fn new(
        server: &ServerHandle,
        finished_tx_chan: Receiver<SignedTransaction>,
        blockchain: &Arc<Mutex<Blockchain>>,
        mempool: &Arc<Mutex<Mempool>>,
    ) -> Self {
        Self {
            server: server.clone(),
            finished_tx_chan,
            blockchain: blockchain.clone(),
            mempool: mempool.clone(),
        }
    }

    pub fn start(self) {
        thread::Builder::new()
            .name("txgen-worker".to_string())
            .spawn(move || {
                self.tx_loop();
            })
            .unwrap();
        println!("txgen worker started");
        info!("tx generator initialized into paused mode");
    }

    fn tx_loop(&self) {
        loop {
            let _transaction = self.finished_tx_chan.recv().expect("Receive finished transaction error");
            let _transaction_hash = _transaction.hash();
            self.mempool.lock().unwrap().hash_map.insert(_transaction.hash(), _transaction);

            // broadcast the hashes of the transactions after insert the transaction
            let tx_vector = vec![_transaction_hash];
            if tx_vector.len() > 0 {
                self.server.broadcast(Message::NewTransactionHashes(tx_vector));
                print!("broadcast the hashes of the transactions after insert the transaction");
            }
        }
    }

}
