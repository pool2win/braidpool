# Braidpool UHPO Spec

## Objective

---

- rust crate to build uhpo transactions.
- Include unit and integration tests for the crate.

Reference Image 

![https://gist.githubusercontent.com/pool2win/77bb9b98f9f3b8c0f90963343c3c840f/raw/8fa2481728e7c12d553608af23ca6e551c7c90c1//uhpo-success.png](https://gist.githubusercontent.com/pool2win/77bb9b98f9f3b8c0f90963343c3c840f/raw/8fa2481728e7c12d553608af23ca6e551c7c90c1//uhpo-success.png)

## Functional Requirements

- Should be able to build `payout-update` and `payout-settlement` Transaction.

## **Specification**

---

### Building Payout-Update/Payout-Settlement Transaction

`payout-update`  is transaction which merges coinbase funds after they achieve maturity. and accumulates in a `eltoo` fashion. payout-update transaction will be cached and broadcasted onchain when coinbase attains maturity..

```rust
pub struct PayoutUpdate {
    transaction: Transaction,
    fee: u64 
}
// Q. let's try to put in the fields here, so we develop an understanding of what is required here.
// A. I could come up with only two params for now. I will add further params as we build.

[KP] The fee can be derived from the transaction, so we don't need the fee field.

// Q. We should use the builder pattern here. You'll find writeups on Rust builder pattern. I think rust-bitcoin uses it too?
// I tried modifying to match it to follow builder pattern.
// new() - builds base templete for payout-update transaction
// add-sig() : signs and adds signature to txs
// builds : returns entire transaction.

[KP] - s/add-sig/add_sig and s/builds/build

[KP] - generally it is not good to shorten names. next_out_address. Do you want to say next_output_address?

[KP] - I haven't reviewed the body of the functions below yet. We just want to focus on the signatures of the functions atm.

[KP] -  Re add_coinbase_sig: Rust doesn't do function overloading. So we'll need two functions with different names.

impl PayoutUpdate {

    pub fn new(
        prev_update_tx: Option<PayoutUpdateTransaction>, // optional because initial payout-update tx is null.
        coinbase: Transaction,
        next_out_address: Address,
    ) -> Self {
		    /*
				    Tx | Inputs       | Output |
				    *****************************
				       | coinbase     | new_payout
				       | prev_update? |
		    */
        let mut tx = Transaction::new();

        let fee = avg_fee.now(24);
        let total_output_value: u64 = coinbase.outputs[0].value - fee;

        if let Some(prev_tx) = prev_update_tx {
            total_output_value += prev_tx.outputs[0].value;
        }

        let output = TxOut::new(total_output_value, next_out_address.script_pubkey());
        tx.output.extend(output);
	
				 let coinbase_input = TxIn {
			            OutPoint: (coinbase.txid , 0),
            };
        tx.input.extend(coinbase_input);

   
        if let Some(prev_tx) = prev_update_tx {
            let prev_input = TxIn {
			            OutPoint: (prev_tx.txid , 0),
            }
            tx.input.extend(prev_input);
        }

      
        PayoutUpdate {
            transaction: tx,
            fee: fee
        }
    }
    
    //  add_coinbase_sig
    pub fn add_coinbase_sig(&mut self, private_key: &SecretKey) -> Result<(), Error> {
		    
		    // get taproot script pubkey for coinbase
		    let taproot_script_pub_key = coinbase.output[0].script;
		    				
				let funding_output = TxOut {
            value: self.transaction.output[0].value,
            script_pubkey: taproot_script_pub_key,
        };
        
        // compute sigHash
        let prevout = vec![&funding_output];
        let mut sighash_cache = SighashCache::new(&self.transaction);
        let sighash = sighash_cache
            .taproot_key_spend_signature_hash(0, &Prevouts::All(&prevout), sighash::TapSighashType::All)
            .unwrap();
           
        let message = Message::from_digest(sighash.as_raw_hash().to_byte_array());
        let tweaked_keypair = private_key.keypair(&secp).add_xonly_tweak(&secp, &taproot_script_pub_key.tap_tweak().to_scalar()).unwrap();
				
				// Sign messege Digest and add signature to witness
        let signature = secp.sign_schnorr(&message, &tweaked_keypair);
        secp.verify_schnorr(
            &signature,
            &message,
            &taproot_script_pub_key.output_key().to_inner(),
        ).unwrap();

        let mut vec_sig = signature.serialize().to_vec();
        vec_sig.push(0x01);
				
        keypath_tx.input[0].witness.push(vec_sig);
        
        Ok(())
    }

    pub fn add_coinbase_sig(&mut self, private_key: &SecretKey) -> Result<(), Error> {
				// follows same pseudo code as earlier function but signs second input!
		}
		
		pub fn build(self) Transaction {
						self.transaction
		}
}
```

### Implementation Code for Payout Update Struct


---

```rust

let coinbase_tx = Transaction::new();

let coinbase_output = TxOut {
    value: 50 * 100_000_000, // 50 BTC
    script_pubkey: POOL_KEY,
};
coinbase_tx.output.push(coinbase_output);

block.txdata.push(coinbase_tx);
block.mine();

// Create a random previous output transaction (prev_update_tx)
let prev_update_tx = Transaction {
    version: 2,
    lock_time: 0,
    input: vec![TxIn {
        previous_output: OutPoint::from("prev_update_out"),
        sequence: 0xFFFFFFFF,
        witness: vec![],
    }],
    output: vec![TxOut {
        value: 100 * 100_000_000, // 100 BTC
        script_pubkey: TAPROOT_KEY,
    }],
};

let payout_update = PayoutUpdate::new(
    Some(prevout_tx),
    coinbase_tx,
    Address::from_string("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq").unwrap(),
);

// Generate random private keys
let secp = Secp256k1::new();
let privkey1 = SecretKey::new(&mut rng);
let privkey2 = SecretKey::new(&mut rng);

// Add signatures with random private keys
payout_update.add_coinbase_sig(&privkey1).unwrap();
payout_update.add_prevout_sig(&privkey2).unwrap();

// Build the transaction
let final_tx = payout_update.build();

```

## Payout Settlement

---

`payout-settlement`  is a transaction which spends funds from from latest `poolkey` which was used as output in `payout-update` transaction.

```rust
pub struct PayoutSettlement {
       transaction: Transaction,
		   fee: u64 
}

impl PayoutSettlement {
    pub fn new(
        latest_eltoo_out_address: Address,
        payout_map: HashMap<Address, Amount>,
    ) -> Self {
        let mut tx = Transaction::new();

        let fee = avg_fee.now(24);
        
        tx.input.extend(TxIn {
		        outpoint : latest_eltoo_out_address,
		        vout: 0
        })
        
        payout_map.iter().for_each(|address, amount| {
			        tx.output.extend({
						        TxOut {
								        script : ScriptBuf::from(Address),
								        amount: amount
						        }
			        })
        });
        
        PayoutSettlement {
            transaction: tx,
            fee: fee
        }
	    }
	    
	  pub fn add_sig(&mut self, private_key: &SecretKey) -> Result<(), Error> {
				// follows same pseudo code as earlier payyout-update struct.
			}

    pub fn build(self) Transaction {
						self.transaction
		}
} 
```

### Implementation Code for Payout Settlement Struct

---

```rust

// Create a random previous output transaction (prev_update_tx)
let prev_update_tx = Transaction {
    version: 2,
    lock_time: 0,
    input: vec![TxIn {
        previous_output: OutPoint::from("prev_update_out"),
        sequence: 0xFFFFFFFF,
        witness: vec![],
    }],
    output: vec![TxOut {
        value: 100 * 100_000_000, // 100 BTC
        script_pubkey: TAPROOT_KEY,
    }],
};

let (_, latest_eltoo_out_address) = generate_random_pubkey();

let payout_map = HashMap<Address , Amount>::new([
									(ALICE , 20),
									(BOB , 30),
									(CHARLIE , 10),
									(DAVID , 40),
									]);

let payout_settlement = PayoutSettlement::new(latest_eltoo_out_address, payout_map);

// Generate random private keys
let secp = Secp256k1::new();
let privkey1 = SecretKey::new(&mut rng);

// Add signatures with random private keys
payout_settlement.add_sig(&privkey1).unwrap();

// Build and print the transaction
let final_tx = payout_settlement.build();
```

## Dependencies

- Rust-bitcoin crate
- zmq and other networking crates.

## Testing and Documentation

- Unit tests for individual modules and integration tests for the overall system.
    - includes spining up local regtest node and executing a chain of transaction flows.
- Example usages in the `examples/` directory

## Expected File Structure

```rust
rust-uhpo/
├── src/
│   ├── lib.rs
│   ├── transaction/
│   │   ├── mod.rs
│   │   ├── payout_update.rs
│   │   ├── payout_settlement.rs
│   ├── network/
│   └── utils/
│       ├── mod.rs
│       └── ... (utility modules)
├── tests/
│   └── ... (integration tests)
├── examples/
│   └── ... (example usages)
├── Cargo.toml
└── README.md
```
