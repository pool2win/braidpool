use core::fmt;

use bitcoin::sighash::TaprootError;

#[derive(Debug)]
pub enum UhpoError {
    TaprootError(TaprootError),
    Secp256k1Error(bitcoin::secp256k1::Error),
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
        UhpoError::Secp256k1Error(err)
    }
}

impl fmt::Display for UhpoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UhpoError::TaprootError(err) => write!(f, "Taproot error: {}", err),
            UhpoError::Secp256k1Error(err) => {
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
    pub fn new_other_error(message: &str) -> Self {
        UhpoError::Other(message.to_string())
    }
}


