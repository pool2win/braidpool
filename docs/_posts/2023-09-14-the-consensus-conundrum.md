---
layout: post
title: DKG+TSS is the Only Consensus We Need
image: assets/BC_Logo_.png
---

Braidpool without transactions, does not need a consensus from the DAG
layer.

## Nakamoto consensus on bitcoin

Nakamoto consensus advances the bitcoin blockchain. Braidpool every
now and then wins blocks in bitcoin's Nakamoto consensus and drives the
chain forward.

## Braidpool without transactions does not need Nakamoto consensus

In Braidpool the consensus is not on the transactions - as there are
no transactions.

In Braidpool there is no consensus required for the block that extends
the chain. This is because multiple miners can find a bitcoin block
and directly broadcast it to bitcoin network. Then the bitcoin
Nakamoto consensus decides on the block that wins.

Braidpool does not decide which block will be submitted to bitcoin on
behalf of all the miners on braidpool.

Braidpool does not try to order the blocks found by miners on
braidpool.

Braidpool has no transactions that can be double spent and therefore
we don't need to reach a consensus on the UTXO set.

## Braidpool needs consensus on the coinbase key and the signature on UHPO tx

Having said that, Braidpool does need a consensus. It needs
participating miners to reach agreement on the coinbase key. At the
same time, braidpool also needs a consensus on the signatures for the
UHPO transactions.

Both the above consensus are actually part of the same protocol. The
DKG generates the coinbase pubkey and TSS signs transactions with that
public key. So in essence, there is a single consensus protocol.

This consensus protocol needs to be byzantine fault tolerant.

Thankfully, DKG and TSS are exactly that. They are byzantine fault
tolerant and there is a reduction from DKG/TSS to consensus.

## Will we weaken the Nakamoto consensus?

No, we won't because we aren't solving consensus for
bitcoin. Braidpool only solves the consensus on the coinbase and UHPO
signatures.

If Braidpool is successfully attacked, the Nakamoto consensus of
bitcoin is not impacted. In the worst possible outcome, miners are not
able to withdraw their earnings from braidpool and bitcoin remains
unaffected.

## So what do we need to do?

If we leave out the transactions from weak blocks, then:

1. We can stop thinking of solving consensus at the DAG layer.
2. We still need the DKG/TSS BFT consensus in a P2P network.

