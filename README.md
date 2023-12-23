
![Build status](https://github.com/wholooks/braidpool/actions/workflows/rust-node.yml/badge.svg)

[![codecov](https://codecov.io/github/pool2win/braidpool/graph/badge.svg?token=41YE7WQS5J)](https://codecov.io/github/pool2win/braidpool)

# Proposal

Read the
[proposal](https://github.com/pool2win/braidpool/blob/8d0724f9fb191ddbd5e02afa20f847fc0249f128/proposal/proposal.pdf)
for braidpool.

For details on how delivering shares to market makers for enabling hashrate
futures, read the blog post: [Deliver Hashrate to Market
Markets](https://blog.opdup.com/2021/08/18/deliver-hashrate-to-market-makers.html).

The goals of the pool are:

1. Lower variance for independent miners, even when large miners join the pool.
2. Miners build their own blocks, just like in p2pool.
3. Payouts require a constant size blockspace, independent of the number of
   miners on the pool.
4. Provide building blocks for enabling a futures market of hash rates.

# Developer

To run tests for the braidpool node, use the usual `cargo` test utility.

```
cd node
cargo test
```

We are using codecov in the CI workflow. To set codecov locally, you
will need to install llvm-codecov and then you'll be able to run it.

Installation `cargo +stable install cargo-llvm-cov --locked` or follow
instructions here: https://github.com/taiki-e/cargo-llvm-cov.

Running test with coverage reports `cargo llvm-code`.

# Running the node

For the moment, the node runs a simple p2p broadcast. To run it you need to do
the usual cargo things

```
cd node
cargo build

# run the first seed node on port 8989
cargo run localhost:8989 localhost:8989 

# run other nodes pointing to the seeding node and specify their own port as 9899
cargo run localhost:8989 localhost:9899
```

# Progress

The [project on github](https://github.com/wholooks/braidpool/projects/1)
tracks the main components to build. Here's a list to keep us focused:

- [ ] P2P gossip based broadcast of miner blocks and shares.
- [ ] Use FROST rust implementation for providing threshold schnorr
      signatures. Use mock clock for identifying rounds.
- [ ] A DAG of shares to track contributions for miners.
- [ ] Validate received blocks and shares.
- [ ] Single script installer (limited to Linux variants, potentially using
      docker).


Matrix chat: [https://matrix.to/#/#braidpool:matrix.org](https://matrix.to/#/#braidpool:matrix.org)

Mailing list: [https://sourceforge.net/p/braidpool/mailman/braidpool-discuss/](https://sourceforge.net/p/braidpool/mailman/braidpool-discuss/)

Development blog: [https://blog.opdup.com/](https://blog.opdup.com/)
