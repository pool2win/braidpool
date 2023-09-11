---
layout: home
title: ""
image: assets/BC_Logo_.png
---

## What is braidpool

Braidpool is a peer to peer bitcoin mining pool that aims to:

1. Low variance for independent miners, even when large miners join
   the pool.
2. Miners can build their own blocks.
3. Payouts in a constant size blockspace.
4. Provide tools for enabling a hashrate futures market.


## TLA+ Specifications

We are using TLA+ to specify and detect any potential errors
early. The current list of protocols we have specified are:

1. [P2P Broadcast]({{ site.baseurl }}{% link /specifications/P2PBroadcast.pdf %})
1. [Shamir Secret Sharing]({{ site.baseurl }}{% link /specifications/ShamirSecretSharing.pdf %})
1. [Miner share accounting and block generation]({{ site.baseurl }}{%link /specifications/BlockGeneration.pdf %}) We specify how
   broadcast shares are accounted for towards miner payouts. When a
   bitcoin block is found, all unaccounted for shares are added to
   miners Unspent Hasher Payout (UHPO). We do not spec out how the
   distributed key generation algorithm is run - instead we replace the
   public key for coinbase payout to simply be the concatenation of
   the miner id.
1. The above spec for block generation uses the [Bitcoin Transactions]({{ site.baseurl }}{% link /specifications/Bitcoin.pdf%}) spec. The transaction spec uses a simple scriptSig = scriptPubkey check.
