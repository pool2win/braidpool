use bitcoin::{
    absolute::LockTime, transaction::Version, Address, Amount, OutPoint, ScriptBuf, Sequence,
    Transaction, TxIn, TxOut, Txid, Witness,
};
pub struct TransactionBuilder {
    transaction: Transaction,
}

impl TransactionBuilder {
    pub fn new() -> Self {
        Self {
            transaction: Transaction {
                version: Version::TWO,
                lock_time: LockTime::ZERO,
                input: vec![],
                output: vec![],
            },
        }
    }

    pub fn add_input(mut self, txid: Txid, vout: u32) -> Self {
        self.transaction.input.push(TxIn {
            previous_output: OutPoint { txid, vout },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::default(),
        });
        self
    }

    pub fn add_output(mut self, address: Address, amount: Amount) -> Self {
        self.transaction.output.push(TxOut {
            value: amount,
            script_pubkey: address.script_pubkey(),
        });
        self
    }

    pub fn build(self) -> Transaction {
        self.transaction
    }
}
