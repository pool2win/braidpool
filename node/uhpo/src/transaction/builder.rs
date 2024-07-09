use bitcoin::{
    absolute::LockTime, transaction::Version, Address, Amount, OutPoint, ScriptBuf, Sequence,
    Transaction, TxIn, TxOut, Txid, Witness,
};
/// A builder for constructing Bitcoin transactions.
pub struct TransactionBuilder {
    /// The transaction being built.
    transaction: Transaction,
}

impl TransactionBuilder {
    /// Creates a new TransactionBuilder with default values.
    ///
    /// # Returns
    /// A new TransactionBuilder instance with an empty transaction.
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

    /// Adds an input to the transaction.
    ///
    /// # Arguments
    /// * `txid` - The transaction ID of the input
    /// * `vout` - The output index of the input
    ///
    /// # Returns
    /// Self, allowing for method chaining
    pub fn add_input(mut self, txid: Txid, vout: u32) -> Self {
        self.transaction.input.push(TxIn {
            previous_output: OutPoint { txid, vout },
            script_sig: ScriptBuf::new(), // Empty script signature (would remain empty since most of txs are p2tr)
            sequence: Sequence::MAX,      // Set sequence to maximum value
            witness: Witness::default(),  // Default (empty) witness
        });
        self
    }

    /// Adds an output to the transaction.
    ///
    /// # Arguments
    /// * `address` - The recipient's Bitcoin address
    /// * `amount` - The amount of Bitcoin to send
    ///
    /// # Returns
    /// Self, allowing for method chaining
    pub fn add_output(mut self, address: Address, amount: Amount) -> Self {
        self.transaction.output.push(TxOut {
            value: amount,
            script_pubkey: address.script_pubkey(),
        });
        self
    }

    /// Finalizes the transaction building process.
    ///
    /// # Returns
    /// The built Transaction
    pub fn build(self) -> Transaction {
        self.transaction
    }
}
