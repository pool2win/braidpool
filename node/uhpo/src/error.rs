use core::fmt;

use bitcoin::sighash::TaprootError;

#[derive(Debug)]
pub enum UhpoError {
    KeypairCreationError(bitcoin::secp256k1::Error),
    SignatureVerificationError(bitcoin::secp256k1::Error),
    TaprootError(TaprootError),
    NoPrevUpdateTxOut,
    Other(String),
}

impl From<TaprootError> for UhpoError {
    fn from(err: TaprootError) -> Self {
        UhpoError::TaprootError(err)
    }
}

impl From<bitcoin::secp256k1::Error> for UhpoError {
    fn from(err: bitcoin::secp256k1::Error) -> Self {
        UhpoError::KeypairCreationError(err)
    }
}

impl fmt::Display for UhpoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UhpoError::KeypairCreationError(err) => write!(f, "Keypair creation error: {}", err),
            UhpoError::TaprootError(err) => write!(f, "Taproot error: {}", err),
            UhpoError::SignatureVerificationError(err) => {
                write!(f, "Signature verification error: {}", err)
            }
            UhpoError::NoPrevUpdateTxOut => {
                write!(f, "No previous update transaction output provided")
            }
            UhpoError::Other(msg) => write!(f, "Other error: {}", msg),
        }
    }
}

impl std::error::Error for UhpoError {}

impl UhpoError {
    pub fn new(message: &str) -> Self {
        UhpoError::Other(message.to_string())
    }
}
