
use std::{collections::HashMap, io::Error, process::Command};

use bitcoin::{
    key::{Keypair, Secp256k1},
    opcodes::{
        all::{OP_CHECKSIG, OP_CSV, OP_DROP},
        OP_0,
    },
    script,
    secp256k1::{All, SecretKey},
    taproot::{TaprootBuilder, TaprootBuilderError, TaprootSpendInfo},
    Address, Network, PublicKey, ScriptBuf, XOnlyPublicKey,
};
use bitcoincore_rpc::Client;
use rand::Rng;


// -- common setup -- //
pub fn setup(n: u64) -> (Secp256k1<All>, Vec<Address>, Vec<Keypair>, Client ) {
    let secp = Secp256k1::new();
    let mut rng = rand::thread_rng();

    let mut keypairs = Vec::new();

    for _ in 0..n {
        let data: [u8; 32] = rng.gen();
        let keypair = SecretKey::from_slice(&data).unwrap().keypair(&secp);
        keypairs.push(keypair);
    }

    let btc_client = Client::new(
        "http://localhost:18443",
        bitcoincore_rpc::Auth::UserPass(String::from("admin1"), String::from("123")),
    )
    .expect("Error creating rpc client");

    let miner_addresses: Vec<Address> = keypairs
        .iter()
        .map(|m| Address::p2pkh(&PublicKey::new(m.public_key()), Network::Regtest))
        .collect();

    (secp, miner_addresses, keypairs, btc_client )
}

// uses nigiri rpc to invoke bitcoin-cli to mine desired block
pub fn mine_blocks(nblocks: u64) -> Result<(), Error> {
    let output = Command::new("nigiri")
        .arg("rpc")
        .arg("--generate")
        .arg(nblocks.to_string())
        .output()?;

    if output.status.success() {
        return Ok(());
    }

    let err = std::str::from_utf8(&output.stderr).map_err(|_| {
        Error::new(
            std::io::ErrorKind::Other,
            "Failed to convert output to string",
        )
    })?;

    Err(Error::new(
        std::io::ErrorKind::Other,
        format!("Failed to mine {} blocks with error: {}", nblocks, err),
    ))
}



// Generate random P2TR address
pub fn generate_taproot_address_nums(secp: &Secp256k1<All>) -> (Address, Keypair) {
    let mut rng = rand::thread_rng();
    let data: [u8; 32] = rng.gen();
    let keypair = SecretKey::from_slice(&data).unwrap().keypair(secp);
    let address = Address::p2tr(&secp, keypair.x_only_public_key().0, None, Network::Regtest);
    (address, keypair)
}

/**
 * Generate random P2TR address
 * one of taproot leaves is a timelock script with a 150 block timelock.
 * this makes sure for the miner to spend the coinbase 50 blocks after coinbase Maturity(100 blocks).
 *
 * Keypath Spend              -> poolkey
 * timelock script path spend -> miner who mined the block
 */
pub fn generate_coinbase_address(
    secp: &Secp256k1<All>,
    poolkey: XOnlyPublicKey,
    timelock_pubkey: &XOnlyPublicKey,
) -> Result<(Address, TaprootSpendInfo), TaprootBuilderError> {
    let taproot_spend_info = TaprootBuilder::new()
        .add_leaf(1, build_timelock_script(150, timelock_pubkey))?
        .add_leaf(1, script::Builder::new().push_opcode(OP_0).into_script())?
        .finalize(&secp, poolkey)
        .expect("Failed to finalize script");
    Ok((
        Address::p2tr_tweaked(taproot_spend_info.output_key(), Network::Regtest),
        taproot_spend_info,
    ))
}

/*
    TimeLock script

    Stack execution
    opcodes                  | stack after execution
                             |
                             | <sig>
    OP_PUSHNUM_N             | <sig> <n>
    OP_CSV                   | <sig> 1|0
    OP_DROP                  | <sig>
    pub_timelock             | <sig> <pubkey>
    OP_CHECKSIG              |  true|false
*/
pub fn build_timelock_script(nblocks: i64, pubkey: &XOnlyPublicKey) -> ScriptBuf {
    script::Builder::new()
        .push_int(nblocks)
        .push_opcode(OP_CSV)
        .push_opcode(OP_DROP)
        .push_x_only_key(pubkey)
        .push_opcode(OP_CHECKSIG)
        .into_script()
}


// Get Balance from Esplora Client
#[derive(Debug, serde::Deserialize)]
struct ChainStats {
    funded_txo_sum: u64,
    spent_txo_sum: u64,
}

pub fn get_balance(address: &str) -> Result<u64, BalanceError> {
    let url = format!("http://localhost:3000/address/{}", address);
    let response = reqwest::blocking::get(&url)?.text()?;

    let json_data: HashMap<String, serde_json::Value> = serde_json::from_str(&response)?;
    let chain_stats = json_data.get("chain_stats")
        .ok_or_else(|| "chain_stats field not found".to_owned())?;

    let chain_stats: ChainStats = serde_json::from_value(chain_stats.clone())?;

    let balance = chain_stats.funded_txo_sum - chain_stats.spent_txo_sum;
    Ok(balance)
}

use core::fmt;

#[derive(Debug)]
pub enum BalanceError {
    RequestError(reqwest::Error),
    JsonParseError(serde_json::Error),
    MissingField(String),
}

impl fmt::Display for BalanceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BalanceError::RequestError(err) => write!(f, "Request error: {}", err),
            BalanceError::JsonParseError(err) => write!(f, "JSON parse error: {}", err),
            BalanceError::MissingField(field) => write!(f, "Missing field: {}", field),
        }
    }
}

impl std::error::Error for BalanceError {}

impl From<reqwest::Error> for BalanceError {
    fn from(err: reqwest::Error) -> Self {
        BalanceError::RequestError(err)
    }
}

impl From<serde_json::Error> for BalanceError {
    fn from(err: serde_json::Error) -> Self {
        BalanceError::JsonParseError(err)
    }
}

impl From<std::string::String> for BalanceError {
    fn from(err: std::string::String) -> Self {
        BalanceError::MissingField(err)
    }
}