# Bitcoin Client Project

This project is developed as part of Princeton's Principles of Blockchains course taught by Prof. Pramod Viswanath. In this semester-long project, I implemented a simplified Bitcoin client in Rust with full node functionality --- a proof-of-work-based distributed system that manages consensus about the state of accounts and authorizes transactions among them. You can find the main course website here (https://blockchains.princeton.edu/principles-of-blockchains/).

## Project Description
The project has the following 4 major components:

1) Mining(src/miner): a miner module that builds blocks continuously in the main mining loop and stuff blocks with transactions from the mempool

2) Network(src/network): a network module that forms a p2p network and uses gossip protocol to exchange data, allowing communications among nodes/clients

3) Transaction(src/txgen): includes the transaction mempool and the transaction generator. The transaction mempool stores all valid received transactions that have not been included in the blockchain, and the transaction generator produces transactions periodically based on the state.

4) State(state struct in src/blockchain/mod): maintain the Ledger State that contains all accounts' information (address, balance, and nonce) to check the validity of transactions, used to conduct the Initial Coin Offering

Note that this project does not include transaction fees, mining rewards, and the associated coinbase transactions. You can find the detailed project breakdown in the "project_breakdown" folder.

## Testing
To test the bitcoin client,

First run cargo build, which generates netid/ece598pv-sp2022-main/target/debug/bitcoin. It is the runnable binary of your code.

Then run three processes of this binary with different ip/ports to them: 
./bitcoin --p2p 127.0.0.1:6000 --api 127.0.0.1:7000
./bitcoin --p2p 127.0.0.1:6001 --api 127.0.0.1:7001 -c 127.0.0.1:6000
./bitcoin --p2p 127.0.0.1:6002 --api 127.0.0.1:7002 -c 127.0.0.1:6001

Then start generating transactions and mining using tx-generator API (theta=100) and mining API(lambda=0) for all 3 nodes. Let them run for 5 minutes:
http://127.0.0.1:7000/txgen/start?theta=100
http://127.0.0.1:7001/txgen/start?theta=100
http://127.0.0.1:7002/txgen/start?theta=100

http://127.0.0.1:7000/miner/start?lambda=0
http://127.0.0.1:7001/miner/start?lambda=0
http://127.0.0.1:7002/miner/start?lambda=0

Lastly, use /blockchain/state API to get the states in 3 nodes and check if they agree:
http://127.0.0.1:7000/blockchain/state?block=100 (which checks the state of node 7000 at the 100th block)

## Contact
If you have any questions or are interested in learning more about the project, feel free to dm me on twitter: https://twitter.com/JiBofan
