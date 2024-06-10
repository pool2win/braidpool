use std::collections::HashMap;

use bitcoin::{io::Error, secp256k1::SecretKey, Address, Transaction};

pub struct PayoutSettlement {
    transaction: Transaction,
}

impl PayoutSettlement {}
