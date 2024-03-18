---
layout: post
title: Lighting Contracts using Bitcoin DSL
---

I spent the last couple of weeks working on the details of [Bitcoin
DSL]({% post_url 2024-03-01-bitcoin-dsl %}) and working on a jupyter
notebook that can be started from a docker image. The docker jupyter
notebook will make it easy for others - and my future self - to use
the DSL.

## Notebook

Here's a sneak peak at a notebook running the DSL. We are still in the
process of documentation on how to use the DSL from a
notebook. Meanwhile, everything is up on the repo, the Dockerfile has
all the details we are working on.

[![Jupyter notebook running Bitcoin DSL](/assets/bitcoin-dsl-jupyter-notebook.png)](/assets/bitcoin-dsl-jupyter-notebook.png)

## Lightning Contracts

We can now describe lightning contracts as seen in the [examples
contracts directory of the
repository](https://github.com/pool2win/bitcoin-dsl/tree/main/lib/contracts/lightning).

Here is a sample where commitment transaction is closed unilaterally by Alice.

```ruby
# Alice broadcasts her commitment transaction unilaterally
broadcast @alice_commitment_tx
confirm transaction: @alice_commitment_tx, to: @alice

# Bob's commitment transaction can no longer be broadcast
assert_not_mempool_accept @bob_commitment_tx

# Alice sweeps her fund from the commitment output
@alice_sweep_tx = transaction inputs: [
                                { tx: @alice_commitment_tx,
                                  vout: 0,
                                  script_sig: 'sig:@alice ""',
                                  csv: @local_delay }
                              ],
                              outputs: [
                                { descriptor: 'wpkh(@alice)', amount: 49.998.sats }
                              ]

# Alice can't sweep until local_delay blocks have been generated
assert_not_mempool_accept @alice_sweep_tx

extend_chain num_blocks: @local_delay, to: @alice

# Now alice can sweep the output from commitment transaction
broadcast @alice_sweep_tx
confirm transaction: @alice_sweep_tx, to: @alice
```

Similarly we have a penalty transaction where Bob is able to sweep
outputs from the commitment transaction if Alice spends a revoked
commitment transaction.

The details of such a transaction are coming soon on a dedicated
documentation website. The website will provide a reference for all
DSL commands as well as examples showing contracts from LN, ARK, RGB,
and others.

