use std::fmt::Error;

use bitcoin::{secp256k1::Scalar, Address, Amount, Transaction, TxOut};

use crate::crypto::signature::{
    add_signature, KeypairBehavior, Secp256k1Behavior, SecretKeyBehavior,
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
    /// Creates a new PayoutUpdate instance.
    ///
    /// # Arguments
    /// * `prev_update_tx` - Optional previous update transaction
    /// * `coinbase_tx` - The coinbase transaction
    /// * `next_out_address` - The address for the next output
    /// * `projected_fee` - The projected transaction fee
    ///
    /// # Returns
    /// Result containing a new PayoutUpdate instance or an Error
    pub fn new(
        prev_update_tx: Option<Transaction>,
        coinbase_tx: Transaction,
        next_out_address: Address,
        projected_fee: Amount,
    ) -> Result<Self, Error> {
        let coinbase_txout = coinbase_tx.output[0].clone();

        // coinbase created by implementation crate would always  have spending vout set to 0
        let mut builder = TransactionBuilder::new().add_input(coinbase_tx.compute_txid(), 0);

        // Add previous update transaction input if it exists
        // previous update tx would be None if and only if this is the first update
        let prev_update_txout = if let Some(tx) = prev_update_tx {
            builder = builder.add_input(tx.compute_txid(), 0);
            Some(tx.output[0].clone())
        } else {
            None
        };

        // Calculate total amount from coinbase and previous update
        let total_amount = prev_update_txout.as_ref().map_or_else(
            || coinbase_txout.value,
            |txout| coinbase_txout.value + txout.value,
        );

        let transaction = builder
            .add_output(next_out_address, total_amount - projected_fee)
            .build();

        Ok(PayoutUpdate {
            transaction,
            coinbase_txout,
            prev_update_txout,
        })
    }

    /// Adds a signature for the coinbase input.
    ///
    /// # Arguments
    /// * `private_key` - The private key for signing
    /// * `tweak` - Optional tweak for the private key
    /// * `secp` - The Secp256k1 context
    ///
    /// # Returns
    /// Result indicating success or an UhpoError
    pub fn add_coinbase_sig<S, K, E>(
        &mut self,
        private_key: S,
        tweak: Option<&Scalar>,
        secp: &E,
    ) -> Result<(), UhpoError>
    where
        S: SecretKeyBehavior<K, E>,
        K: KeypairBehavior<E>,
        E: Secp256k1Behavior + 'static,
    {
        let prevouts = match &self.prev_update_txout {
            Some(prev_update_txout) => vec![&self.coinbase_txout, prev_update_txout],
            None => vec![&self.coinbase_txout],
        };

        add_signature(
            &mut self.transaction,
            0, // coinbase input idx
            &prevouts,
            private_key,
            tweak,
            secp,
        )
    }

    /// Adds a signature for the previous update input.
    ///
    /// # Arguments
    /// * `private_key` - The private key for signing
    /// * `tweak` - Optional tweak for the private key
    /// * `secp` - The Secp256k1 context
    ///
    /// # Returns
    /// Result indicating success or an UhpoError
    pub fn add_prev_update_sig<S, K, E>(
        &mut self,
        private_key: S,
        tweak: Option<&Scalar>,
        secp: &E,
    ) -> Result<(), UhpoError>
    where
        S: SecretKeyBehavior<K, E>,
        K: KeypairBehavior<E>,
        E: Secp256k1Behavior + 'static,
    {
        let prev_update_txout = self
            .prev_update_txout
            .as_ref()
            .ok_or(UhpoError::NoPrevUpdateTxOut)?;
        let prevouts = vec![&self.coinbase_txout, prev_update_txout];

        add_signature(
            &mut self.transaction,
            1,
            &prevouts,
            private_key,
            tweak,
            secp,
        )
    }

    pub fn build(self) -> Transaction {
        self.transaction
    }
}

// unit tests
#[cfg(test)]
mod tests {
    use crate::crypto::signature::{
        MockKeypairBehavior, MockSecp256k1Behavior, MockSecretKeyBehavior,
    };

    use super::*;
    use bitcoin::{
        absolute::LockTime, key::Keypair, secp256k1::All, transaction::Version, Network, ScriptBuf,
    };
    use rand::Rng;
    use secp256k1::{Secp256k1, SecretKey};

    // Helper function for setting up tests

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
        Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![],
            output: vec![TxOut {
                value: Amount::from_sat(50000000),
                script_pubkey: ScriptBuf::new(),
            }],
        }
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
            .add_coinbase_sig(keypair.secret_key(), None, &secp)
            .expect("Failed to add coinbase signature");

        payout_update
            .add_prev_update_sig(keypair.secret_key(), None, &secp)
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
                &secp,
            )
            .expect("Failed to add coinbase signature");

        payout_update
            .add_prev_update_sig(
                keypair.secret_key(),
                Some(&Scalar::random_custom(&mut rand::thread_rng())),
                &secp,
            )
            .expect("Failed to add prev update signature");

        assert_eq!(payout_update.transaction.input[0].witness.len(), 1);
        assert_eq!(payout_update.transaction.input[1].witness.len(), 1);
    }

    #[test]
    fn test_add_signature_mock_error_should_fail() {
        let (_, _, new_payout_address) = setup();

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

        // setup mocks
        let mock_secp = MockSecp256k1Behavior::new();

        let create_mock_secret_key = || {
            let mut mock_keypair = MockKeypairBehavior::new();
            let mut mock_secret_key = MockSecretKeyBehavior::<
                MockKeypairBehavior<MockSecp256k1Behavior>,
                MockSecp256k1Behavior,
            >::new();

            // add_xonly_tweak on keypair, returns an error
            mock_keypair
                .expect_add_xonly_tweak()
                .return_once(|_: &MockSecp256k1Behavior, _| Err(secp256k1::Error::InvalidTweak));

            // secretKey.keypair returns an MockKeypair
            mock_secret_key
                .expect_keypair()
                .return_once(move |_| mock_keypair);

            mock_secret_key
        };

        let result = payout_update.add_coinbase_sig(
            create_mock_secret_key(),
            Some(&Scalar::random_custom(&mut rand::thread_rng())),
            &mock_secp,
        );

        assert!(matches!(result, Err(UhpoError::KeypairCreationError(_))));

        let result = payout_update.add_prev_update_sig(
            create_mock_secret_key(),
            Some(&Scalar::random_custom(&mut rand::thread_rng())),
            &mock_secp,
        );

        assert!(matches!(result, Err(UhpoError::KeypairCreationError(_))));
    }
}
