---
layout: post
title: GhostDAG Assumes a Synchronous Network Model
image: assets/BC_Logo_.png
---

The GhostDAG protocol implemented by Kaspa network uses a partial
synchrony network assumption. The maximum message delay is an input to
a function that approximates the maximum width of the network. This
maximum width is then used as a configuration option for a network
instantiation. The authors claim that the message delay is not know a
priori, however, this maximum width is.

In fairness, the assumption of maximum delay to message is actually a
synchronous network mode. The authors are aware of this and leave it
for the academic community accept their claim that their model is a
partially synchronous network model.

## SPECTRE Network Model

![SPECTRE Network Model](/assets/spectre-network-model.png).

## GHOSTDAG Network Model

![GhostDAG Network Model](/assets/ghostdag-network-model-1.png).
![GhostDAG Network Model](/assets/ghostdag-network-model-2.png).


In the implementation they use a [default k of
18](https://github.com/kaspanet/kaspad/blob/bd1420220a1c9f7ab253b2b120240351e9440146/domain/dagconfig/consensus_defaults.go#L38). The
network message delay is thus obfuscated, but it is still there.

It seems to me that these protocols use the DAG to track time and
leave out the explicit requirement of synchronised clocks. The problem
is that using time derived from messages delivered to figure out if
any new messages are being delivered within a message delay introduces
a circular dependency. I'll soon write another post about this.

## Bitcoin Does Not Assume a Message Delay

I wrote about this in the post [Synchrony in bitcoin]({% post_url
2023-04-02-synchrony-in-bitcoin %}). The need for speed has to have us
make some compromise, and this post calls a spade a spade.
