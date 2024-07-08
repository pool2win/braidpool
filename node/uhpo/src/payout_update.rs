use std::fmt::Error;

use bitcoin::{
    hashes::Hash,
    key::{Keypair, Secp256k1},
    secp256k1::{Message, Scalar, SecretKey , All},
    sighash::{Prevouts, SighashCache},
    Address, Amount, TapSighashType, Transaction, TxOut,
};

use crate::{transaction::TransactionBuilder, UhpoError};

/// `PayoutUpdate` represents an update to a Eltoo style payout.
pub struct PayoutUpdate {
    transaction: Transaction,

    // coinbase and prev_update txout's are to be store and used while signing respective inputs
    coinbase_txout: TxOut,
    prev_update_txout: Option<TxOut>,
}


impl PayoutUpdate {
    pub fn new(
        prev_update_tx: Option<Transaction>,
        coinbase_tx: Transaction,
        next_out_address: Address,
        projected_fee: Amount,
    ) -> Result<Self, Error> {
        let coinbase_txout = coinbase_tx.output[0].clone();

        // coinbase created by implementation crate would always  have spending vout set to 0
        let mut builder = TransactionBuilder::new().add_input(coinbase_tx.compute_txid(), 0);

        let prev_update_txout = if let Some(tx) = prev_update_tx {
            builder = builder.add_input(tx.compute_txid(), 0);
            Some(tx.output[0].clone())
        } else {
            None
        };

        let total_amount = prev_update_txout
            .as_ref()
            .map_or(coinbase_txout.value, |txout| {
                coinbase_txout.value + txout.value
            });

        let transaction = builder
            .add_output(next_out_address, total_amount - projected_fee)
            .build();

        Ok(PayoutUpdate {
            transaction,
            coinbase_txout,
            prev_update_txout,
        })
    }

    pub fn add_coinbase_sig(
        &mut self,
        private_key: SecretKey,
        tweak: Option<&Scalar>,
        secp: &Secp256k1<All>,
    ) -> Result<(), UhpoError> {
        let prevouts = match &self.prev_update_txout {
            Some(prev_update_txout) => vec![&self.coinbase_txout, prev_update_txout],
            None => vec![&self.coinbase_txout],
        };

        add_signature(&mut self.transaction, 0, &prevouts, private_key, tweak , secp)
    }

    pub fn add_prev_update_sig(
        &mut self,
        private_key: SecretKey,
        tweak: Option<&Scalar>,
        secp: &Secp256k1<All>,
    ) -> Result<(), UhpoError> {
        let prev_update_txout = self
            .prev_update_txout
            .as_ref()
            .ok_or(UhpoError::NoPrevUpdateTxOut)?;
        let prevouts = vec![&self.coinbase_txout, prev_update_txout];

        add_signature(&mut self.transaction, 1, &prevouts, private_key, tweak , secp)
    }

    pub fn build(self) -> Transaction {
        self.transaction
    }
}

fn add_signature(
    transaction: &mut Transaction,
    input_idx: usize,
    prevouts: &[&TxOut],
    private_key: SecretKey,
    tweak: Option<&Scalar>,
    secp: &Secp256k1<All>,
) -> Result<(), UhpoError> {

    let keypair: Keypair = match tweak {
        Some(tweak) => private_key
            .keypair(secp)
            .add_xonly_tweak(secp, tweak)
            .map_err(UhpoError::KeypairCreationError)?,
        None => private_key.keypair(secp),
    };

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
        .map_err(UhpoError::SignatureVerificationError)?;

    transaction.input[input_idx].witness.push(vec_sig);

    Ok(())
}

// unit tests
#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::{
        absolute::LockTime, key::Keypair, secp256k1::All, transaction::Version, Network, ScriptBuf,
    };
    use rand::Rng;

    pub fn setup() -> (Secp256k1<All>, Keypair, Address) {
        let secp = Secp256k1::new();
        let mut rng = rand::thread_rng();

        let data: [u8; 32] = rng.gen();
        let keypair = SecretKey::from_slice(&data).unwrap().keypair(&secp);

        let new_payout_address: Address =
            Address::p2tr(&secp, keypair.x_only_public_key().0, None, Network::Regtest);

        (secp, keypair, new_payout_address)
    }

    pub fn create_dummy_transaction() -> Transaction {
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

    #[test]
    fn test_new_payout_update_only_cb() {
        let (_, _, new_payout_address) = setup();

        let coinbase_tx = create_dummy_transaction();
        let prev_update_tx = None;
        let projected_fee = Amount::from_sat(1000);

        let payout_update = PayoutUpdate::new(
            prev_update_tx,
            coinbase_tx,
            new_payout_address,
            projected_fee,
        )
        .expect("Failed to create payout update");

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
            Some(prev_update_tx),
            coinbase_tx,
            new_payout_address,
            projected_fee,
        )
        .expect("Failed to create payout update");

        assert_eq!(payout_update.transaction.input.len(), 2);
        assert_eq!(payout_update.transaction.output.len(), 1);
        assert_eq!(
            payout_update.transaction.output[0].value,
            Amount::from_sat(50000000 * 2) - projected_fee
        );
        assert!(payout_update.prev_update_txout.is_some());
    }

    #[test]
    fn test_add_signature_with_no_tweak() {
        let (secp, keypair, new_payout_address) = setup();

        let coinbase_tx = create_dummy_transaction();
        let prev_update_tx = create_dummy_transaction();
        let projected_fee = Amount::from_sat(1000);
        let mut payout_update = PayoutUpdate::new(
            Some(prev_update_tx),
            coinbase_tx,
            new_payout_address,
            projected_fee,
        )
        .expect("Failed to create payout update");

        payout_update
            .add_coinbase_sig(keypair.secret_key(), None , &secp)
            .expect("Failed to add coinbase signature");

        payout_update
            .add_prev_update_sig(keypair.secret_key(), None , &secp)
            .expect("Failed to add prev update signature");

        assert_eq!(payout_update.transaction.input[0].witness.len(), 1);
        assert_eq!(payout_update.transaction.input[1].witness.len(), 1);
    }

    #[test]
    fn test_add_signature_with_tweak() {
        let (secp, keypair, new_payout_address) = setup();

        let coinbase_tx = create_dummy_transaction();
        let prev_update_tx = create_dummy_transaction();
        let projected_fee = Amount::from_sat(1000);
        let mut payout_update = PayoutUpdate::new(
            Some(prev_update_tx),
            coinbase_tx,
            new_payout_address,
            projected_fee,
        )
        .expect("Failed to create payout update");

        payout_update
            .add_coinbase_sig(
                keypair.secret_key(),
                Some(&Scalar::random_custom(&mut rand::thread_rng())),
                &secp
            )
            .expect("Failed to add coinbase signature");

        payout_update
            .add_prev_update_sig(
                keypair.secret_key(),
                Some(&Scalar::random_custom(&mut rand::thread_rng())),
                &secp
            )
            .expect("Failed to add prev update signature");

        assert_eq!(payout_update.transaction.input[0].witness.len(), 1);
        assert_eq!(payout_update.transaction.input[1].witness.len(), 1);
    }
}

#[cfg(test)]
mod mock_tests {
    // unimplemented
}