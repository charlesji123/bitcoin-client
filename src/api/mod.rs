use serde::Serialize;
use crate::blockchain::Blockchain;
use crate::miner::Handle as MinerHandle;
use crate::txgen::Handle as TransactionHandle;
use crate::network::server::Handle as NetworkServerHandle;
use crate::network::message::Message;
use crate::types::hash::Hashable;

use log::info;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use tiny_http::Header;
use tiny_http::Response;
use tiny_http::Server as HTTPServer;
use url::Url;

pub struct Server {
    handle: HTTPServer,
    miner: MinerHandle,
    network: NetworkServerHandle,
    blockchain: Arc<Mutex<Blockchain>>,
    tx_generator: TransactionHandle,
}

#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

macro_rules! respond_result {
    ( $req:expr, $success:expr, $message:expr ) => {{
        let content_type = "Content-Type: application/json".parse::<Header>().unwrap();
        let payload = ApiResponse {
            success: $success,
            message: $message.to_string(),
        };
        let resp = Response::from_string(serde_json::to_string_pretty(&payload).unwrap())
            .with_header(content_type);
        $req.respond(resp).unwrap();
    }};
}
macro_rules! respond_json {
    ( $req:expr, $message:expr ) => {{
        let content_type = "Content-Type: application/json".parse::<Header>().unwrap();
        let resp = Response::from_string(serde_json::to_string(&$message).unwrap())
            .with_header(content_type);
        $req.respond(resp).unwrap();
    }};
}

impl Server {
    pub fn start(
        addr: std::net::SocketAddr,
        miner: &MinerHandle,
        network: &NetworkServerHandle,
        blockchain: &Arc<Mutex<Blockchain>>,
        tx_generator: &TransactionHandle, 
    ) {
        let handle = HTTPServer::http(&addr).unwrap();
        let server = Self {
            handle,
            miner: miner.clone(),
            network: network.clone(),
            blockchain: Arc::clone(blockchain),
            tx_generator: tx_generator.clone(),
        };
        thread::spawn(move || {
            for req in server.handle.incoming_requests() {
                let miner = server.miner.clone();
                let network = server.network.clone();
                let tx_generator = server.tx_generator.clone();
                let blockchain = Arc::clone(&server.blockchain);
                thread::spawn(move || {
                    // a valid url requires a base
                    let base_url = Url::parse(&format!("http://{}/", &addr)).unwrap();
                    let url = match base_url.join(req.url()) {
                        Ok(u) => u,
                        Err(e) => {
                            respond_result!(req, false, format!("error parsing url: {}", e));
                            return;
                        }
                    };
                    match url.path() {
                        "/miner/start" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let lambda = match params.get("lambda") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing lambda");
                                    return;
                                }
                            };
                            let lambda = match lambda.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing lambda: {}", e)
                                    );
                                    return;
                                }
                            };
                            miner.start(lambda);
                            respond_result!(req, true, "ok");
                        }
                        "/txgen/start" => {
                            print!("txgen start");
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let theta = match params.get("theta") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing theta");
                                    return;
                                }
                            };
                            let theta = match theta.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing theta: {}", e)
                                    );
                                    return;
                                }
                            };
                            tx_generator.start(theta); //
                            respond_result!(req, true, "ok");
                        }
                        "/network/ping" => {
                            network.broadcast(Message::Ping(String::from("Test ping")));
                            respond_result!(req, true, "ok");
                        }
                        "/blockchain/longest-chain" => {
                            print!("longest chain length {}", {blockchain.lock().unwrap().all_blocks_in_longest_chain().len()});
                            let v = {blockchain.lock().unwrap().all_blocks_in_longest_chain()};
                            let v_string: Vec<String> = v.into_iter().map(|h|h.to_string()).collect();
                            respond_json!(req, v_string);
                        }
                        "/blockchain/longest-chain-tx" => {
                            // get the transaction hashes of all the blocks in the longest chain
                            let longest_chain_tx = {blockchain.lock().unwrap().all_tx_in_longest_chain()};
                            print!("longest chain tx length {}", longest_chain_tx.len());
                            let mut all_txs = Vec::new();
                            for n in 0..longest_chain_tx.len(){
                                let block_tx = longest_chain_tx[n].clone();
                                let v_string: Vec<String> = block_tx.into_iter().map(|h|h.to_string()).collect();
                                all_txs.push(v_string);
                            }
                            respond_json!(req, all_txs);
                        }
                        "/blockchain/state" => {
                            let params = url.query_pairs();
                            let params: HashMap<_, _> = params.into_owned().collect();
                            let lambda = match params.get("block") {
                                Some(v) => v,
                                None => {
                                    respond_result!(req, false, "missing block number");
                                    return;
                                }
                            };
                            let lambda = match lambda.parse::<u64>() {
                                Ok(v) => v,
                                Err(e) => {
                                    respond_result!(
                                        req,
                                        false,
                                        format!("error parsing lambda: {}", e)
                                    );
                                    return;
                                }
                            };
                            let mut index = 0;
                            let mut block_hash = {blockchain.lock().unwrap().tip()};
                            for hash in blockchain.lock().unwrap().all_blocks_in_longest_chain() {
                                if index == lambda {
                                    block_hash = hash;
                                    break;
                                }
                                index += 1;
                                println!("block hash in block api: {}", hash);
                            }
                            let state = {blockchain.lock().unwrap().state_map.get(&block_hash).unwrap().clone()};
                            let mut all_addresses = Vec::new();
                            for (address, (nonce, balance)) in state.state {
                                all_addresses.push((address, nonce, balance));
                            }
                            respond_json!(req, all_addresses);
                        }

                        "/blockchain/longest-chain-tx-count" => {
                            respond_result!(req, false, "unimplemented!");
                        }
                        _ => {
                            let content_type =
                                "Content-Type: application/json".parse::<Header>().unwrap();
                            let payload = ApiResponse {
                                success: false,
                                message: "endpoint not found".to_string(),
                            };
                            let resp = Response::from_string(
                                serde_json::to_string_pretty(&payload).unwrap(),
                            )
                            .with_header(content_type)
                            .with_status_code(404);
                            req.respond(resp).unwrap();
                        }
                    }
                });
            }
        });
        info!("API server listening at {}", &addr);
    }
}
