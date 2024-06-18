## UHPO (Unspent Hasher PayOut) Crate

This Rust crate provides functionality to build UHPO transactions, which are used for secure payouts in Braidpool.

### Features

- Build and sign Payout Update Transactions
- Build and sign Payout Settlement Transactions
- Example implementation demonstrating the usage of the crate


### Prerequisites

- Rust and Cargo
- Docker [for nigiri]


### Installation and Setup

1. In Order to test and implement this crate locally one might need to use Regtest environment. Testcases uses nigiri to manage services like `regtest` , `electrs` and `esplora`. U can install nigiri with

```bash
$ curl https://getnigiri.vulpem.com | bash
``` 

2. Start Regtest and other services with.
```bash
$ nigiri start
```

3. One can stop, reset nigiri and restart it with
```bash
$ nigiri stop --delete && nigiri start
```

### Testing
To run the tests for this crate, use the following command:
```bash
$ cargo test --package uhpo --test standard_flow -- standard_flow --exact --show-output
```

<hr>

### Crate Structure
```text
    .
    ├── Cargo.toml
    ├── Readme.md
    ├── docs
    │   └── spec.md
    ├── src
    │   ├── error.rs
    │   ├── lib.rs
    │   ├── payout_settlement.rs
    │   └── payout_update.rs
    └── tests
        ├── standard_flow.rs
        └── utils.rs
```
