use std::collections::HashMap;

use bitcoin::{Address, Amount, Transaction, TxOut};
use secp256k1::Scalar;

use crate::{
    crypto::{
        add_signature,
        signature::{KeypairBehavior, Secp256k1Behavior, SecretKeyBehavior},
    },
    transaction::TransactionBuilder,
    UhpoError,
};

// `PayoutSettlement` represents an cashout of an Eltoo style payout.
pub struct PayoutSettlement {
    transaction: Transaction,

    // `prev_update_txout` is the output of the latest Eltoo style `PayoutUpdate` it is stored and later used to sign inputs.
    prev_update_txout: TxOut,
}

impl PayoutSettlement {
    /// Creates a new `PayoutSettlement`
    ///
    /// # Arguments
    /// * `latest_eltoo_out` - The output of the latest `PayoutUpdate`
    /// * `payout_map` - A map of payout addresses to payout amounts
    ///
    /// # Returns
    /// Result containing a new `PayoutSettlement` or an `UhpoError`
    pub fn new(latest_eltoo_out: Transaction, payout_map: HashMap<Address, Amount>) -> Self {
        // payout_update_tx would always have vout set to 0
        let builder = TransactionBuilder::new().add_input(latest_eltoo_out.compute_txid(), 0);

        let builder = payout_map
            .into_iter()
            .fold(builder, |builder, (address, amount)| {
                builder.add_output(address, amount)
            });

        PayoutSettlement {
            transaction: builder.build(),
            prev_update_txout: latest_eltoo_out.output[0].clone(),
        }
    }

    /// Adds a signature to the latest `PayoutUpdate`
    ///
    /// # Arguments
    /// * `private_key` - The private key to sign with
    /// * `tweak` - An optional tweak
    /// * `secp` - The secp256k1 context
    ///
    /// # Returns
    /// Result indicating success or an `UhpoError`
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
        let prevouts = vec![&self.prev_update_txout];

        add_signature(
            &mut self.transaction,
            0,
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
    use bitcoin::{absolute::LockTime, transaction::Version, Network, ScriptBuf};
    use rand::Rng;
    use secp256k1::{All, Keypair, Secp256k1, SecretKey};

    use crate::crypto::signature::{
        MockKeypairBehavior, MockSecp256k1Behavior, MockSecretKeyBehavior,
    };

    use super::*;

    fn create_dummy_transaction(amount_out: Amount) -> Transaction {
        Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![],
            output: vec![TxOut {
                value: amount_out,
                script_pubkey: ScriptBuf::default(),
            }],
        }
    }

    fn setup(miner_count: u64) -> (Secp256k1<All>, Keypair, HashMap<Address, Amount>, u64) {
        let secp = Secp256k1::new();
        let mut rng = rand::thread_rng();

        let data: [u8; 32] = rng.gen();
        let keypair = SecretKey::from_slice(&data).unwrap().keypair(&secp);

        let mut total_amount = 0;
        let cashout_map: HashMap<Address, Amount> = (0..miner_count)
            .map(|_| {
                let data: [u8; 32] = rng.gen();
                let keypair = SecretKey::from_slice(&data).unwrap().keypair(&secp);
                let new_user_address: Address =
                    Address::p2tr(&secp, keypair.x_only_public_key().0, None, Network::Regtest);

                let amount = rng.gen::<u32>();
                total_amount += amount as u64;
                (new_user_address, Amount::from_sat(amount as u64))
            })
            .collect();

        (secp, keypair, cashout_map, total_amount)
    }

    #[test]
    fn test_payout_settlement() {
        let (_, _, cashout_map, total_amount) = setup(3);
        let latest_payout_update = create_dummy_transaction(Amount::from_sat(total_amount + 1000));

        let settlement_tx =
            PayoutSettlement::new(latest_payout_update, cashout_map.clone()).build();

        assert!(settlement_tx.output.len() == 3);
        assert!(settlement_tx.input.len() == 1);

        settlement_tx.output.iter().for_each(|o| {
            let address = Address::from_script(&o.script_pubkey, Network::Regtest).unwrap();
            assert!(
                cashout_map
                    .get(&address)
                    .expect("Address not found in cashout_map")
                    == &o.value
            );
        });
    }

    #[test]
    fn test_add_signature_with_no_tweak() {
        let (secp, keypair, cashout_map, _) = setup(3);
        let latest_payout_update = create_dummy_transaction(Amount::from_sat(1000));

        let mut payout_settlement = PayoutSettlement::new(latest_payout_update, cashout_map);

        payout_settlement
            .add_prev_update_sig(keypair.secret_key(), None, &secp)
            .expect("Failed to add signature");

        assert_eq!(payout_settlement.transaction.input[0].witness.len(), 1);
        assert_eq!(payout_settlement.transaction.input[0].witness[0].len(), 65);
    }

    #[test]
    fn test_add_signature_with_tweak() {
        let (secp, keypair, cashout_map, _) = setup(3);
        let latest_payout_update = create_dummy_transaction(Amount::from_sat(1000));

        let mut payout_settlement = PayoutSettlement::new(latest_payout_update, cashout_map);

        payout_settlement
            .add_prev_update_sig(
                keypair.secret_key(),
                Some(&Scalar::random_custom(&mut rand::thread_rng())),
                &secp,
            )
            .expect("Failed to add signature");

        assert_eq!(payout_settlement.transaction.input[0].witness.len(), 1);
        assert_eq!(payout_settlement.transaction.input[0].witness[0].len(), 65);
    }

    #[test]
    fn test_add_signature_mock_error_should_fail() {
        let (_, _, cashout_map, _) = setup(3);
        let latest_payout_update = create_dummy_transaction(Amount::from_sat(1000));

        let mut payout_settlement = PayoutSettlement::new(latest_payout_update, cashout_map);

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

        let result = payout_settlement.add_prev_update_sig(
            create_mock_secret_key(),
            Some(&Scalar::random_custom(&mut rand::thread_rng())),
            &mock_secp,
        );

        assert!(matches!(result, Err(UhpoError::KeypairCreationError(_))));
    }
}
