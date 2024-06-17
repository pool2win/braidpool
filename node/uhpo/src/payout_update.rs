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

pub struct PayoutUpdate<'a> {
    transaction: Transaction,
    coinbase_txout: &'a TxOut,
    prev_update_txout: Option<&'a TxOut>,
}

impl<'a> PayoutUpdate<'a> {
    pub fn new(
        prev_update_tx: Option<&'a Transaction>,
        coinbase_tx: &'a Transaction,
        next_out_address: Address,
        projected_fee: Amount,
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

    pub fn add_coinbase_sig(
        &mut self,
        private_key: &SecretKey,
        tweak: &Option<Scalar>,
    ) -> Result<(), TaprootError> {
        let mut prevouts = vec![self.coinbase_txout];
        if self.prev_update_txout.is_some() {
            prevouts.push(self.prev_update_txout.unwrap());
        }

        add_signature(&mut self.transaction, 0, &prevouts, private_key, tweak)
    }

    pub fn add_prev_update_sig(
        &mut self,
        private_key: &SecretKey,
        tweak: &Option<Scalar>,
    ) -> Result<(), TaprootError> {
        let prev_update_txout = self
            .prev_update_txout
            .ok_or(TaprootError::InvalidSighashType(0))?;

        let prevouts = vec![self.coinbase_txout, prev_update_txout];
        add_signature(&mut self.transaction, 1, &prevouts, private_key, tweak)
    }

    pub fn build(self) -> Transaction {
        self.transaction
    }
}

fn build_transaction(
    coinbase_tx: &Transaction,
    prev_update_tx: Option<&Transaction>,
    next_out_address: Address,
    projected_fee: Amount,
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
        value: total_amount - projected_fee,
        script_pubkey: next_out_address.script_pubkey(),
    });

    Ok(payout_update_tx)
}

fn add_signature(
    transaction: &mut Transaction,
    input_idx: usize,
    prevouts: &[&TxOut],
    private_key: &SecretKey,
    tweak: &Option<Scalar>,
) -> Result<(), TaprootError> {
    let secp = Secp256k1::new();
    let keypair: bitcoin::key::Keypair;

    if let Some(tweak) = tweak {
        keypair = private_key
            .keypair(&secp)
            .add_xonly_tweak(&secp, tweak)
            .unwrap();
    } else {
        keypair = private_key.keypair(&secp);
    }

    let mut sighash_cache = SighashCache::new(transaction.clone());

    let sighash = sighash_cache.taproot_key_spend_signature_hash(
        input_idx,
        &Prevouts::All(prevouts),
        TapSighashType::All,
    )?;

    let message = Message::from_digest(sighash.as_raw_hash().to_byte_array());

    let signature = secp.sign_schnorr_with_rng(&message, &keypair, &mut rand::thread_rng());
    let mut vec_sig = signature.serialize().to_vec();
    vec_sig.push(0x01);

    secp.verify_schnorr(&signature, &message, &keypair.x_only_public_key().0)
        .unwrap();

    transaction.input[input_idx].witness.push(vec_sig);

    Ok(())
}

// unit tests
#[cfg(test)]
mod tests {
    use bitcoin::{key::Keypair, secp256k1::All, Network};
    use rand::Rng;

    use super::*;

    fn setup() -> (Secp256k1<All>, Keypair, Address) {
        let secp = Secp256k1::new();
        let mut rng = rand::thread_rng();

        let data: [u8; 32] = rng.gen();
        let keypair = SecretKey::from_slice(&data).unwrap().keypair(&secp);

        let new_payout_address: Address =
            Address::p2tr(&secp, keypair.x_only_public_key().0, None, Network::Regtest);

        (secp, keypair, new_payout_address)
    }

    #[test]
    fn test_new_payout_update_only_cb() {
        let (_, _, new_payout_address) = setup();

        let coinbase_tx = create_dummy_transaction();
        let prev_update_tx = None;
        let projected_fee = Amount::from_sat(1000);

        let payout_update = PayoutUpdate::new(
            prev_update_tx,
            &coinbase_tx,
            new_payout_address,
            projected_fee,
        )
        .unwrap();

        assert_eq!(payout_update.transaction.input.len(), 1);
        assert_eq!(payout_update.transaction.output.len(), 1);
        assert_eq!(
            payout_update.transaction.output[0].value,
            Amount::from_sat(50000000) - projected_fee
        );
        assert!(payout_update.prev_update_txout.is_none());
    }

    #[test]
    fn test_new_payout_update() {
        let (_, _, new_payout_address) = setup();

        let coinbase_tx = create_dummy_transaction();
        let prev_update_tx = create_dummy_transaction();

        let projected_fee = Amount::from_sat(1000);

        let payout_update = PayoutUpdate::new(
            Some(&prev_update_tx),
            &coinbase_tx,
            new_payout_address,
            projected_fee,
        )
        .unwrap();

        assert_eq!(payout_update.transaction.input.len(), 2);
        assert_eq!(payout_update.transaction.output.len(), 1);
        assert_eq!(
            payout_update.transaction.output[0].value,
            Amount::from_sat(50000000 * 2) - projected_fee
        );
        assert!(payout_update.prev_update_txout.is_some());
    }

    #[test]
    fn test_add_signature() {
        let (_, keypair, new_payout_address) = setup();

        let coinbase_tx = create_dummy_transaction();
        let prev_update_tx = create_dummy_transaction();
        let projected_fee = Amount::from_sat(1000);
        let mut payout_update = PayoutUpdate::new(
            Some(&prev_update_tx),
            &coinbase_tx,
            new_payout_address,
            projected_fee,
        )
        .unwrap();

        payout_update
            .add_coinbase_sig(&keypair.secret_key(), &None)
            .unwrap();

        payout_update
            .add_prev_update_sig(&keypair.secret_key(), &None)
            .unwrap();

        assert_eq!(payout_update.transaction.input[0].witness.len(), 1);
        assert_eq!(payout_update.transaction.input[1].witness.len(), 1);
    }

    fn create_dummy_transaction() -> Transaction {
        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![],
            output: vec![TxOut {
                value: Amount::from_sat(50000000),
                script_pubkey: ScriptBuf::new(),
            }],
        };
        tx
    }
}
