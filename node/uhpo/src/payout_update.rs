use std::fmt::Error;

use bitcoin::{
    absolute::LockTime,
    hashes::Hash,
    key::Secp256k1,
    secp256k1::{Message, Scalar, SecretKey},
    sighash::{Prevouts, SighashCache, TaprootError},
    transaction::Version,
    Address, Amount, OutPoint, ScriptBuf, Sequence, TapSighashType, Transaction, TxIn, TxOut,
    Witness,
};

struct PayoutUpdate<'a> {
    transaction: Transaction,
    coinbase_txout: &'a TxOut,
    prev_update_txout: Option<&'a TxOut>,
}

impl<'a> PayoutUpdate<'a> {
    fn new(
        prev_update_tx: Option<&'a Transaction>,
        coinbase_tx: &'a Transaction,
        next_out_address: Address,
        projected_fee: u64,
    ) -> Result<Self, Error> {
        let prev_update_txout = prev_update_tx.map(|tx| &tx.output[0]);
        let coinbase_txout = &coinbase_tx.output[0];

        let payout_update_tx = build_transaction(
            coinbase_tx,
            prev_update_tx,
            next_out_address,
            projected_fee,
            coinbase_txout,
            prev_update_txout,
        )?;

        Ok(PayoutUpdate {
            transaction: payout_update_tx,
            coinbase_txout,
            prev_update_txout,
        })
    }

    fn add_coinbase_sig(
        &mut self,
        private_key: &SecretKey,
        tweak: &Scalar,
    ) -> Result<(), TaprootError> {
        add_signature(
            &mut self.transaction,
            0,
            &[self.coinbase_txout],
            private_key,
            tweak,
        )
    }

    fn add_prevout_sig(
        &mut self,
        private_key: &SecretKey,
        tweak: &Scalar,
    ) -> Result<(), TaprootError> {
        let prev_update_txout = self
            .prev_update_txout
            .ok_or(TaprootError::InvalidSighashType(0))?;
        add_signature(
            &mut self.transaction,
            1,
            &[prev_update_txout],
            private_key,
            tweak,
        )
    }

    fn build(self) -> Transaction {
        self.transaction
    }
}

fn build_transaction(
    coinbase_tx: &Transaction,
    prev_update_tx: Option<&Transaction>,
    next_out_address: Address,
    projected_fee: u64,
    coinbase_txout: &TxOut,
    prev_update_txout: Option<&TxOut>,
) -> Result<Transaction, Error> {
    let mut total_amount = coinbase_txout.value;
    if let Some(tx_out) = prev_update_txout {
        total_amount += tx_out.value;
    }

    let mut payout_update_tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![],
        output: vec![],
    };

    payout_update_tx.input.push(TxIn {
        previous_output: OutPoint {
            txid: coinbase_tx.compute_txid(),
            vout: 0,
        },
        script_sig: ScriptBuf::new(),
        sequence: Sequence::MAX,
        witness: Witness::default(),
    });

    if let Some(tx) = prev_update_tx {
        payout_update_tx.input.push(TxIn {
            previous_output: OutPoint {
                txid: tx.compute_txid(),
                vout: 0,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::default(),
        });
    }

    payout_update_tx.output.push(TxOut {
        value: total_amount - Amount::from_sat(projected_fee),
        script_pubkey: next_out_address.script_pubkey(),
    });

    Ok(payout_update_tx)
}

fn add_signature(
    transaction: &mut Transaction,
    input_idx: usize,
    prevouts: &[&TxOut],
    private_key: &SecretKey,
    tweak: &Scalar,
) -> Result<(), TaprootError> {
    let secp = Secp256k1::new();
    let keypair = private_key.keypair(&secp);

    let mut sighash_cache = SighashCache::new(transaction.clone());
    let sighash = sighash_cache.taproot_key_spend_signature_hash(
        input_idx,
        &Prevouts::All(prevouts),
        TapSighashType::All,
    )?;

    let message = Message::from_digest(sighash.as_raw_hash().to_byte_array());
    let tweaked_keypair = keypair
        .add_xonly_tweak(&secp, tweak)
        .map_err(|_| TaprootError::InvalidSighashType(0))?;

    let signature = secp.sign_schnorr_no_aux_rand(&message, &tweaked_keypair);
    let mut vec_sig = signature.serialize().to_vec();
    vec_sig.push(0x01);

    transaction.input[input_idx].witness.push(vec_sig);

    Ok(())
}
