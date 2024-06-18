mod utils;

use std::{collections::HashMap, thread::sleep, time::Duration};

use bitcoin::{
    absolute::LockTime,
    hashes::Hash,
    key::{Keypair, Secp256k1},
    opcodes::OP_0,
    script,
    secp256k1::Scalar,
    taproot::TaprootBuilder,
    transaction::Version,
    Address, Amount, BlockHash, Network, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut,
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
 * - Tested in Regtest Environment.
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

// Uhpo Transactions and Signing Keypairs cached
struct UhpoState {
    payout_update_kp: Keypair,
    payout_update_tx: Transaction,
    payout_settlement_tx: Transaction,
}

#[test]
fn standard_flow() {
    // -- Setup -- //
    let (secp, miner_addresses, miners, btcd_client) = setup(3);

    // using short hand clossure to mine blocks
    let mine_blocks = |nblocks| {
        for _ in 0..nblocks {
            btcd_client.generate_to_address(1, &Address::p2shwsh(&script::Builder::new().push_opcode(OP_0).into_script(), Network::Regtest)).unwrap();
        }
    };

    // compute Current block reward
    // Regtest Has halving for every 150 blocks making current block reward unpredictable. Each testcase mines about 110 blocks
    let inital_block_num = btcd_client.get_block_count().unwrap();
    let block_reward: Amount =
        Amount::from_btc(50_f64 / 2_f64.powf((inital_block_num as u32 / 150_u32) as f64)).unwrap();

    // Cache UhpoState entries
    let mut uhpo_entries: Vec<UhpoState> = Vec::new();

    // Mine 3 blocks and cache payout update/settlement Transactions
    (0..3).for_each(|i| {
        let (_, coinbase_kp) = generate_taproot_address_nums(&secp);

        // compute coinbase address for each miner cb_addr = p2tr(poolkey + MAST(timelock)*G)
        let (coinbase_addr, coinbase_tr_info) = generate_coinbase_address(
            &secp,
            coinbase_kp.x_only_public_key().0,
            &miners[i].x_only_public_key().0,
        )
        .unwrap();

        // build coinbaseTx exact same way as built by Regtest
        let coinbase_tx: &Transaction = &compute_coinbase_tx(
            coinbase_addr.script_pubkey(),
            (btcd_client.get_block_count().unwrap() + 1_u64)
                .try_into()
                .unwrap(),
            block_reward,
        );
        let (_, payout_update_kp) = generate_taproot_address_nums(&secp);

        // Initial Payout Update is None!!
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

        let block_hashs: Vec<BlockHash> =
            btcd_client.generate_to_address(1, &coinbase_addr).unwrap();
        let coinbase_txid: Txid =
            btcd_client.get_block(&block_hashs[0]).unwrap().txdata[0].compute_txid();

        assert_eq!(coinbase_txid, coinbase_tx.compute_txid());
    });

    let current_block_count = btcd_client.get_block_count().unwrap();
    let payout_update_101: Result<Txid, bitcoincore_rpc::Error> =
        btcd_client.send_raw_transaction(&uhpo_entries[0].payout_update_tx);

    assert!(payout_update_101.is_err());
    assert_eq!(current_block_count - inital_block_num, 3);

    mine_blocks(97);

    // -- payout-update : 101 -- //
    let payout_update_101_txid: Txid = btcd_client
        .send_raw_transaction(&uhpo_entries[0].payout_update_tx)
        .unwrap();
    mine_blocks(1);

    // -- payout-update : 102 -- //
    let payout_update_102_txid = btcd_client
        .send_raw_transaction(&uhpo_entries[1].payout_update_tx)
        .unwrap();
    mine_blocks(1);

    // -- payout-update : 103 -- //
    let payout_update_103_txid = btcd_client
        .send_raw_transaction(&uhpo_entries[2].payout_update_tx)
        .unwrap();
    mine_blocks(1);

    // -- payout-settlement : 103 -- //
    let payout_settlement_103_txid = btcd_client
        .send_raw_transaction(&uhpo_entries[2].payout_settlement_tx)
        .unwrap();
    mine_blocks(1);

    assert_eq!(
        payout_update_101_txid,
        uhpo_entries[0].payout_update_tx.compute_txid()
    );
    assert_eq!(
        payout_update_102_txid,
        uhpo_entries[1].payout_update_tx.compute_txid()
    );
    assert_eq!(
        payout_update_103_txid,
        uhpo_entries[2].payout_update_tx.compute_txid()
    );
    assert_eq!(
        payout_settlement_103_txid,
        uhpo_entries[2].payout_settlement_tx.compute_txid()
    );

    // Electrs Indexer takes about 5 seconds to sync.
    sleep(Duration::from_millis(5000));

    // Ensure All Payouts are settled with correct amounts
    uhpo_entries[2]
        .payout_settlement_tx
        .output
        .iter()
        .for_each(|o| {
            let miner_address = Address::from_script(&o.script_pubkey, Network::Regtest).unwrap();
            let balance = get_balance(miner_address.to_string().as_str()).unwrap();
            assert_eq!(balance, o.value.to_sat());
        });

    // Ensure all payout updates are spent
    uhpo_entries.iter().for_each(|u| {
        let payout_update_address = Address::from_script(
            &u.payout_update_tx.output[0].script_pubkey,
            Network::Regtest,
        )
        .unwrap();
        let balance = get_balance(payout_update_address.to_string().as_str()).unwrap();
        assert_eq!(balance, 0);
    });

    // Try spending payout-settlement Transaction (Expected to fail)
    uhpo_entries.iter().for_each(|u| {
        let payout_settlement = btcd_client.send_raw_transaction(&u.payout_settlement_tx);
        assert!(payout_settlement.is_err());
    })
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
