mod utils;

use std::collections::HashMap;

use bitcoin::{
    absolute::LockTime,
    hashes::Hash,
    key::{Keypair, Secp256k1},
    opcodes::OP_0,
    script,
    secp256k1::Scalar,
    taproot::TaprootBuilder,
    transaction::Version,
    Address, Amount, Network, OutPoint, PublicKey, ScriptBuf, Sequence, Transaction, TxIn, TxOut,
    Txid, Witness,
};
use bitcoincore_rpc::RpcApi;
use rand::Rng;

use utils::*;

/**
 * Test that the standard flow works
 * (Reference Image)[https://gist.githubusercontent.com/pool2win/77bb9b98f9f3b8c0f90963343c3c840f/raw/8fa2481728e7c12d553608af23ca6e551c7c90c1/uhpo-success.png]
 *
 * Test Case follows above Execution Flow
 * - uses Nigiri for Regtest.
 *
 * Flow
 * - first block - mined by alice
 * - second block - mined by bob
 * - third block - mined by carol
 *
 * block 101 - payout-update
 * block 102 - payout-update
 * block 103 - payout-update and payout-settlement
 */

struct UhpoState {
    payout_update_kp: Keypair,
    payout_update_tx: Transaction,
    payout_settlement_tx: Transaction,
}

#[test]
fn standard_flow() {
    // -- Setup -- //
    let (secp, miners) = setup(3);

    let miner_addresses: Vec<Address> = miners
        .iter()
        .map(|m| Address::p2pkh(&PublicKey::new(m.public_key()), Network::Regtest))
        .collect();

    let client = bitcoincore_rpc::Client::new(
        "http://localhost:18443",
        bitcoincore_rpc::Auth::UserPass(String::from("admin1"), String::from("123")),
    )
    .unwrap();

    let inital_block_num = client.get_block_count().unwrap();
    let block_reward =
        Amount::from_btc(50_f64 / 2_f64.powf((inital_block_num as u32 / 150_u32) as f64)).unwrap();

    // Mine 0..3 blocks
    let mut uhpo_entries: Vec<UhpoState> = Vec::new();

    (0..3).for_each(|i| {
        // -- Mine First BLock -- //
        let (_, coinbase_kp) = generate_taproot_address_nums();

        let (coinbase_addr, coinbase_tr_info) = generate_coinbase_address(
            coinbase_kp.x_only_public_key().0,
            &miners[i].x_only_public_key().0,
        )
        .unwrap();

        //   -- payout-build/settlement and cache
        let coinbase_tx = &compute_coinbase_tx(
            coinbase_addr.script_pubkey(),
            (client.get_block_count().unwrap() + 1_u64)
                .try_into()
                .unwrap(),
            block_reward,
        );
        let (_, payout_update_kp) = generate_taproot_address_nums();

        let (prev_update_tx, prev_update_poolkp, prev_update_tweak) = if i == 0 {
            (None, None, None)
        } else {
            (
                Some(&uhpo_entries[i - 1].payout_update_tx),
                Some(uhpo_entries[i - 1].payout_update_kp),
                {
                    Some(
                        TaprootBuilder::new()
                            .finalize(
                                &secp,
                                uhpo_entries[i - 1].payout_update_kp.x_only_public_key().0,
                            )
                            .unwrap()
                            .tap_tweak()
                            .to_scalar(),
                    )
                },
            )
        };

        let payout_update_taptweak = Some(
            TaprootBuilder::new()
                .finalize(&secp, payout_update_kp.x_only_public_key().0)
                .unwrap()
                .tap_tweak()
                .to_scalar(),
        );

        let (payout_update_tx, payout_settlement_tx) = uhpo_txs(
            coinbase_tx,
            prev_update_tx,
            coinbase_kp,
            prev_update_poolkp,
            payout_update_kp,
            &miner_addresses,
            i as u64 + 1,
            block_reward,
            coinbase_tr_info.tap_tweak().to_scalar(),
            prev_update_tweak,
            payout_update_taptweak,
        );

        let uhpo_state = UhpoState {
            payout_update_kp,
            payout_update_tx,
            payout_settlement_tx,
        };

        uhpo_entries.push(uhpo_state);

        generate_block_to_address(&coinbase_addr).unwrap();
    });

    let current_block_count = client.get_block_count().unwrap();

    assert!(current_block_count - inital_block_num == 3);

    let payout_update_101_txid = client.send_raw_transaction(&uhpo_entries[0].payout_update_tx);
    assert!(payout_update_101_txid.is_err());

    mine_blocks(97).unwrap();

    // -- payout-update : 101 -- //
    let payout_update_101_txid = client
        .send_raw_transaction(&uhpo_entries[0].payout_update_tx)
        .unwrap();
    assert!(payout_update_101_txid == uhpo_entries[0].payout_update_tx.compute_txid());

    mine_blocks(1).unwrap();

    // -- payout-update : 102 -- //
    let payout_update_102_txid = client
        .send_raw_transaction(&uhpo_entries[1].payout_update_tx)
        .unwrap();
    assert!(payout_update_102_txid == uhpo_entries[1].payout_update_tx.compute_txid());
    mine_blocks(1).unwrap();

    // -- payout-update : 103 -- //
    let payout_update_103_txid = client
        .send_raw_transaction(&uhpo_entries[2].payout_update_tx)
        .unwrap();
    assert!(payout_update_103_txid == uhpo_entries[2].payout_update_tx.compute_txid());
    mine_blocks(1).unwrap();

    // -- payout-settlement : 103 -- //
    let payout_settlement_103_txid = client
        .send_raw_transaction(&uhpo_entries[2].payout_settlement_tx)
        .unwrap();
    assert!(payout_settlement_103_txid == uhpo_entries[2].payout_settlement_tx.compute_txid());

    // check states and balances
    // states - assertions, balances
}

fn compute_coinbase_tx(
    script_pubkey: ScriptBuf,
    current_block: i64,
    current_block_reward: Amount,
) -> Transaction {
    let witness_commitment =
        "6a24aa21a9ede2f61c3f71d1defd3fa999dfa36953755c690689799962b48bebd836974e8cf9";

    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![],
        output: vec![],
    };

    let mut vin = TxIn {
        previous_output: OutPoint {
            txid: Txid::all_zeros(),
            vout: 0xffffffff,
        },
        script_sig: script::Builder::new()
            .push_int(current_block)
            .push_opcode(OP_0)
            .into_script(),
        sequence: Sequence::MAX,
        witness: Witness::new(),
    };

    vin.witness.push(
        0x0000000000000000000000000000000000000000000000000000000000000000_usize.to_be_bytes(),
    );

    let vouts = vec![
        TxOut {
            script_pubkey: script_pubkey,
            value: current_block_reward,
        },
        TxOut {
            script_pubkey: hex::decode(witness_commitment)
                .expect("Failed to decode hex string")
                .into(),
            value: Amount::from_sat(0),
        },
    ];

    tx.input.push(vin);
    tx.output.extend(vouts);

    tx
}

fn distribute_payouts_with_rand(
    miners: &Vec<Address>,
    total_amount: Amount,
) -> HashMap<Address, Amount> {
    let mut payout_map = HashMap::new();
    let mut rng = rand::thread_rng();
    let mut remaining_amount = total_amount;

    for (index, miner) in miners.iter().enumerate() {
        let amount = if miners.len() - 1 == index {
            remaining_amount
        } else {
            let random_amount = Amount::from_sat(rng.gen_range(0..remaining_amount.to_sat()));
            remaining_amount -= random_amount;
            random_amount
        };
        payout_map.insert(miner.clone(), amount);
    }

    payout_map
}

fn uhpo_txs(
    coinbase_tx: &Transaction,
    prev_update_tx: Option<&Transaction>,
    coinbase_poolkp: Keypair,
    prev_update_kp: Option<Keypair>,
    update_poolkp: Keypair,
    miner_addresses: &Vec<Address>,
    block_count: u64,
    current_block_reward: Amount,
    coinbase_tweak: Scalar,
    prev_update_tweak: Option<Scalar>,
    cur_update_tweak: Option<Scalar>,
) -> (Transaction, Transaction) {
    let secp = Secp256k1::new();

    let update_pk = Address::p2tr(
        &secp,
        update_poolkp.x_only_public_key().0,
        None,
        Network::Regtest,
    );

    let mut payout_update_builder = uhpo::payout_update::PayoutUpdate::new(
        prev_update_tx,
        coinbase_tx,
        update_pk,
        Amount::from_sat(500),
    )
    .unwrap();

    payout_update_builder
        .add_coinbase_sig(&coinbase_poolkp.secret_key(), &Some(coinbase_tweak))
        .unwrap();

    if let Some(prev_update_kp) = prev_update_kp {
        payout_update_builder
            .add_prev_update_sig(&prev_update_kp.secret_key(), &prev_update_tweak)
            .unwrap();
    }

    let payout_update_tx = payout_update_builder.build();

    let miner_share_record = distribute_payouts_with_rand(
        miner_addresses,
        (current_block_reward - Amount::from_sat(600)) * block_count,
    );
    let mut payout_settlement_tx =
        uhpo::payout_settlement::PayoutSettlement::new(&payout_update_tx, &miner_share_record);

    payout_settlement_tx
        .add_sig(&update_poolkp.secret_key(), &cur_update_tweak)
        .unwrap();

    (payout_update_tx.clone(), payout_settlement_tx.build())
}
