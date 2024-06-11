use std::collections::HashMap;

use bitcoin::{
    absolute::LockTime,
    hashes::Hash,
    key::Secp256k1,
    secp256k1::{Message, Scalar, SecretKey},
    sighash::{Prevouts, SighashCache, TaprootError},
    transaction::Version,
    Address, Amount, OutPoint, TapSighashType, Transaction, TxIn, TxOut, Txid,
};

pub struct PayoutSettlement<'a> {
    transaction: Transaction,
    prevouts: &'a TxOut,
}

impl<'a> PayoutSettlement<'a> {
    pub fn new(latest_eltoo_out: &'a Transaction, payout_map: &HashMap<Address, Amount>) -> Self {
        let mut settlement_tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: latest_eltoo_out.compute_txid(),
                    vout: 0,
                },
                ..Default::default()
            }],
            output: vec![],
        };

        payout_map.iter().for_each(|(address, amount)| {
            settlement_tx.output.push(TxOut {
                script_pubkey: address.script_pubkey(),
                value: *amount,
            })
        });

        PayoutSettlement {
            transaction: settlement_tx,
            prevouts: &latest_eltoo_out.output[0],
        }
    }

    pub fn add_sig(&mut self, private_key: &SecretKey, tweak: &Scalar) -> Result<(), TaprootError> {
        let secp = Secp256k1::new();
        let keypair = private_key.keypair(&secp);

        let mut sighash_cache = SighashCache::new(self.transaction.clone());

        let sighash = sighash_cache.taproot_key_spend_signature_hash(
            0,
            &Prevouts::All(&[self.prevouts]),
            TapSighashType::All,
        )?;

        let message = Message::from_digest(sighash.as_raw_hash().to_byte_array());
        let tweaked_keypair = keypair
            .add_xonly_tweak(&secp, tweak)
            .map_err(|_| TaprootError::InvalidSighashType(0))?;

        let signature =
            secp.sign_schnorr_with_rng(&message, &tweaked_keypair, &mut rand::thread_rng());
        let mut vec_sig = signature.serialize().to_vec();
        vec_sig.push(0x01);

        secp.verify_schnorr(&signature, &message, &tweaked_keypair.x_only_public_key().0)
            .unwrap();

        self.transaction.input[0].witness.push(vec_sig);

        Ok(())
    }

    pub fn build(self) -> Transaction {
        self.transaction
    }
}

// unit tests
#[cfg(test)]
mod tests {
    use bitcoin::{key::Keypair, secp256k1::All, Network, Script, ScriptBuf};
    use rand::Rng;

    use super::*;

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

        let settlement_tx = PayoutSettlement::new(&latest_payout_update, &cashout_map).build();

        assert!(settlement_tx.output.len() == 3);
        assert!(settlement_tx.input.len() == 1);

        settlement_tx.output.iter().for_each(|o| {
            let address = Address::from_script(&o.script_pubkey, Network::Regtest).unwrap();

            assert!(cashout_map.get(&address).unwrap() == &o.value);
        });
    }

    #[test]
    fn test_add_sig() {
        let (secp, keypair, cashout_map, _) = setup(3);
        let latest_payout_update = create_dummy_transaction(Amount::from_sat(1000));

        let mut payout_settlement = PayoutSettlement::new(&latest_payout_update, &cashout_map);
        let tweak = Scalar::from_be_bytes([0; 32]).unwrap();

        payout_settlement
            .add_sig(&keypair.secret_key(), &tweak)
            .unwrap();

        assert!(payout_settlement.transaction.input[0].witness.len() == 1);
    }

    fn create_dummy_transaction(amount_out: Amount) -> Transaction {
        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![],
            output: vec![TxOut {
                value: amount_out,
                script_pubkey: ScriptBuf::default(),
            }],
        };
        tx
    }
}


