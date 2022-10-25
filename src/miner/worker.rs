use crossbeam::channel::{unbounded, Receiver, Sender, TryRecvError};
use log::{debug, info};
use crate::network::message::Message::NewBlockHashes;
use crate::types::block::Block;
use crate::network::server::Handle as ServerHandle;
use crate::miner::Context;
use crate::blockchain::Blockchain;
use crate::types::hash::Hashable;
use std::thread;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Worker {
    server: ServerHandle,
    finished_block_chan: Receiver<Block>,
    arc_mutex: Arc<Mutex<Blockchain>>, 
}

impl Worker {
    pub fn new(
        server: &ServerHandle,
        finished_block_chan: Receiver<Block>,
        arc_mutex: &Arc<Mutex<Blockchain>>,
    ) -> Self {
        Self {
            server: server.clone(),
            finished_block_chan,
            arc_mutex: arc_mutex.clone(),
        }
    }

    pub fn start(self) {
        thread::Builder::new()
            .name("miner-worker".to_string())
            .spawn(move || {
                self.worker_loop();
            })
            .unwrap();
        info!("Miner initialized into paused mode");
    }

    fn worker_loop(&self) {
        loop {
            let _block = self.finished_block_chan.recv().expect("Receive finished block error");
            // insert the block into the blockchain
            let mut blockchain = self.arc_mutex.lock().unwrap();
            blockchain.insert(&_block);

            // broadcast the block hash after insert the block
            let _block_hash = _block.hash();
            let block_vector = vec![_block_hash];
            self.server.broadcast(NewBlockHashes(block_vector));
        }
    }
}
