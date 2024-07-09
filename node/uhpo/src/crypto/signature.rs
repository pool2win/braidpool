use bitcoin::{
    hashes::Hash,
    sighash::{Prevouts, SighashCache},
    TapSighashType, Transaction, TxOut, XOnlyPublicKey,
};
use mockall::automock;
use rand::{CryptoRng, Rng};
use secp256k1::{schnorr::Signature, All, Keypair, Message, Scalar, Secp256k1, SecretKey};

use crate::UhpoError;

// traits for mock testing

/// Trait for secret key behavior, allowing creation of keypairs.
#[automock]
pub trait SecretKeyBehavior<K: KeypairBehavior<E>, E: Secp256k1Behavior + 'static> {
    fn keypair(&self, secp: &E) -> K;
}

/// Trait for keypair behavior, allowing tweaking of keypairs, Exclusively used by mock tests to mock Keypair methods
#[automock]
pub trait KeypairBehavior<E: Secp256k1Behavior + 'static> {
    fn add_xonly_tweak(
        self,
        secp: &E,
        tweak: &Scalar,
    ) -> Result<Keypair, bitcoin::secp256k1::Error>;

    /// Converts the keypair to a standard Keypair type.
    fn to_keypair(&self) -> Keypair;
}

/// Trait for secp256k1 behavior, including signing and verification operations.
#[automock]
pub trait Secp256k1Behavior {
    /// Signs a message using Schnorr signature scheme.
    fn sign_schnorr_with_rng<R: Rng + CryptoRng + 'static>(
        &self,
        msg: &Message,
        keypair: &Keypair,
        rng: &mut R,
    ) -> Signature;

    /// Verifies a Schnorr signature.
    fn verify_schnorr(
        &self,
        signature: &Signature,
        message: &Message,
        pubkey: &XOnlyPublicKey,
    ) -> Result<(), bitcoin::secp256k1::Error>;
}

// Implementations for concrete types

impl SecretKeyBehavior<Keypair, Secp256k1<All>> for SecretKey {
    fn keypair(&self, secp: &Secp256k1<All>) -> Keypair {
        self.keypair(secp)
    }
}

impl KeypairBehavior<Secp256k1<All>> for Keypair {
    fn add_xonly_tweak(
        self,
        secp: &Secp256k1<All>,
        tweak: &Scalar,
    ) -> Result<Keypair, bitcoin::secp256k1::Error> {
        self.add_xonly_tweak(secp, tweak)
    }

    fn to_keypair(&self) -> Keypair {
        self.clone()
    }
}

impl Secp256k1Behavior for Secp256k1<All> {
    fn sign_schnorr_with_rng<R: Rng + CryptoRng>(
        &self,
        msg: &Message,
        keypair: &Keypair,
        rng: &mut R,
    ) -> Signature {
        self.sign_schnorr_with_rng(msg, keypair, rng)
    }

    fn verify_schnorr(
        &self,
        signature: &Signature,
        message: &Message,
        pubkey: &XOnlyPublicKey,
    ) -> Result<(), bitcoin::secp256k1::Error> {
        self.verify_schnorr(signature, message, pubkey)
    }
}

/// Adds a signature to a transaction input.
///
/// This function creates a signature for the specified input of a transaction,
/// optionally applying a tweak to the private key before signing.
pub fn add_signature<S, K, E>(
    transaction: &mut Transaction,
    input_idx: usize,
    prevouts: &[&TxOut],
    private_key: S,
    tweak: Option<&Scalar>,
    secp: &E,
) -> Result<(), UhpoError>
where
    S: SecretKeyBehavior<K, E>,
    K: KeypairBehavior<E>,
    E: Secp256k1Behavior + 'static,
{
    // apply tweak if provided
    let keypair: Keypair = match tweak {
        Some(tweak) => private_key
            .keypair(secp)
            .add_xonly_tweak(secp, tweak)
            .map_err(UhpoError::KeypairCreationError)?,
        None => private_key.keypair(secp).to_keypair(),
    };

    // Compute the sighash
    let mut sighash_cache = SighashCache::new(transaction.clone());
    let sighash = sighash_cache.taproot_key_spend_signature_hash(
        input_idx,
        &Prevouts::All(prevouts),
        TapSighashType::All,
    )?;

    // Create and sign the message
    let message = Message::from_digest(sighash.as_raw_hash().to_byte_array());
    let signature = secp.sign_schnorr_with_rng(&message, &keypair, &mut rand::thread_rng());

    // Verify the signature
    secp.verify_schnorr(&signature, &message, &keypair.x_only_public_key().0)
        .map_err(UhpoError::SignatureVerificationError)?;

    // Add the signature to the transaction input
    let mut vec_sig = signature.serialize().to_vec();
    vec_sig.push(0x01);
    transaction.input[input_idx].witness.push(vec_sig);

    Ok(())
}

#[cfg(test)]
mod mock_tests {
    use crate::crypto::signature::*;

    use bitcoin::{absolute::LockTime, transaction::Version, Amount, ScriptBuf, TxIn};
    use rand::rngs::ThreadRng;

    // dummy transaction for testing with no inputs
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
    fn test_add_signatures_key_creation_fails() {
        // setup mocks
        let mock_secp = MockSecp256k1Behavior::new();
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

        let mut transaction = create_dummy_transaction();
        transaction.input.push(TxIn {
            ..Default::default()
        });
        let prevout = TxOut {
            value: Amount::from_sat(0),
            script_pubkey: ScriptBuf::new(),
        };
        let tweak = Some(Scalar::random_custom(&mut rand::thread_rng()));

        let result = add_signature(
            &mut transaction,
            0,
            &[&prevout],
            mock_secret_key,
            tweak.as_ref(),
            &mock_secp,
        );

        assert!(matches!(result, Err(UhpoError::KeypairCreationError(_))));
    }

    #[test]
    fn test_add_signatures_signature_verification_fails() {
        // setup
        // valid secp is used in order to generate valid keypair in add_signatures
        let real_secp = Secp256k1::new();

        // setup mocks
        let mut mock_secp = MockSecp256k1Behavior::new();
        let mut mock_keypair = MockKeypairBehavior::<MockSecp256k1Behavior>::new();
        let mut mock_secret_key = MockSecretKeyBehavior::<
            MockKeypairBehavior<MockSecp256k1Behavior>,
            MockSecp256k1Behavior,
        >::new();

        // add_xonly_tweak on keypair returns a valid keypair
        mock_keypair
            .expect_add_xonly_tweak()
            .return_once(move |_: &MockSecp256k1Behavior, _| {
                Ok(Keypair::new(&real_secp, &mut rand::thread_rng()))
            });

        // secretKey.keypair returns an MockKeypair
        mock_secret_key
            .expect_keypair()
            .return_once(move |_| mock_keypair);

        // sign_schnorr_with_rng returns a  signature
        mock_secp.expect_sign_schnorr_with_rng().return_once(
            |_: &Message, _: &Keypair, _: &mut ThreadRng| {
                Signature::from_slice(&[0u8; 64]).unwrap()
            },
        );

        // verify_schnorr returns an error
        mock_secp
            .expect_verify_schnorr()
            .return_once(|_, _, _| Err(secp256k1::Error::InvalidSignature));

        let mut transaction = create_dummy_transaction();
        transaction.input.push(TxIn {
            ..Default::default()
        });
        let prevout = TxOut {
            value: Amount::from_sat(0),
            script_pubkey: ScriptBuf::new(),
        };
        let tweak = Some(Scalar::random_custom(&mut rand::thread_rng()));

        let result = add_signature(
            &mut transaction,
            0,
            &[&prevout],
            mock_secret_key,
            tweak.as_ref(),
            &mock_secp,
        );

        assert!(matches!(
            result,
            Err(UhpoError::SignatureVerificationError(_))
        ));
    }
}
