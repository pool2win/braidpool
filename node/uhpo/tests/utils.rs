use std::{io::Error, process::Command};

use bitcoin::{
    key::{Keypair, Secp256k1},
    opcodes::all::{OP_CHECKSIG, OP_CSV, OP_DROP},
    script,
    secp256k1::{All, SecretKey},
    taproot::{TaprootBuilder, TaprootSpendInfo},
    Address, Network, ScriptBuf, XOnlyPublicKey,
};
use rand::Rng;

pub fn setup(n: u64) -> (Secp256k1<All>, Vec<Keypair>) {
    let secp = Secp256k1::new();
    let mut rng = rand::thread_rng();

    // let alice = SecretKey::from_slice(
    //     &hex::decode("b6a3de6f252db639b5601152af470cd290d317b45c5fa9cf0c84e4ccb4d7166b").unwrap(),
    // )

    let mut keys = Vec::new();
    for _ in 0..n {
        let data: [u8; 32] = rng.gen();
        let keypair = SecretKey::from_slice(&data).unwrap().keypair(&secp);
        keys.push(keypair);
    }
    (secp, keys)
}

pub fn mine_blocks(nblocks: u64) -> Result<(), Error> {
    let output = Command::new("nigiri")
        .arg("rpc")
        .arg("--generate")
        .arg(nblocks.to_string())
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        return Ok(());
    }

    let err = std::str::from_utf8(&output.stderr)
        .expect("Failed to convert output to string")
        .trim()
        .to_string();

    Err(Error::new(
        std::io::ErrorKind::Other,
        format!("Failed to mine {} blocks with error: {}", nblocks, err),
    ))
}

pub fn generate_block_to_address(address: &Address) -> Result<(), Error> {
    let output = Command::new("nigiri")
        .arg("rpc")
        .arg("generatetoaddress")
        .arg("1")
        .arg(address.to_string())
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        return Ok(());
    }

    let err = std::str::from_utf8(&output.stderr)
        .expect("Failed to convert output to string")
        .trim()
        .to_string();

    Err(Error::new(
        std::io::ErrorKind::Other,
        format!("Failed to generate block to address with error: {}", err),
    ))
}

pub fn generate_taproot_address_nums() -> (Address, Keypair) {
    let secp = Secp256k1::new();

    let mut rng = rand::thread_rng();
    let data: [u8; 32] = rng.gen();
    let keypair = SecretKey::from_slice(&data).unwrap().keypair(&secp);

    let address = Address::p2tr(&secp, keypair.x_only_public_key().0, None, Network::Regtest);

    (address, keypair)
}

pub fn generate_coinbase_address(
    poolkey: XOnlyPublicKey,
    timelock_pubkey: &XOnlyPublicKey,
) -> Result<(Address, TaprootSpendInfo), Error> {
    let secp = Secp256k1::new();
    let taproot_spend_info = TaprootBuilder::new()
        .add_leaf(1, build_timelock_script(150, timelock_pubkey))
        .expect("Failed to build timelock script")
        .add_leaf(1, build_timelock_script(150, timelock_pubkey))
        .expect("Failed to build timelock script")
        .finalize(&secp, poolkey)
        .expect("Failed to finalize script");
    Ok((
        Address::p2tr_tweaked(taproot_spend_info.output_key(), Network::Regtest),
        taproot_spend_info,
    ))
}

pub fn build_timelock_script(nblocks: i64, pubkey: &XOnlyPublicKey) -> ScriptBuf {
    script::Builder::new()
        .push_int(nblocks)
        .push_opcode(OP_CSV)
        .push_opcode(OP_DROP)
        .push_x_only_key(pubkey)
        .push_opcode(OP_CHECKSIG)
        .into_script()
}
