---
layout: post
title: Connection management, broadcasts and more Rust
---

Last week I focused on setting up components for managing connections
and adding support for broadcasting messages to all connected nodes. I
also revised the configuration parsing component to handle defaults
cleanly with less redundant code.

## Managing connections

We are now able to track all the peers a node is connected to in a
list and can track the metadata about each connection. The component
for handling this change is called connection manager. The connection
manager provides a convenient place to limit the number of
connections - we are already doing this now. In the future we will
track the quality of each connections and make decisions on how to
rotate connections based on the metadata tracked by the connection
manager component.

## Send Broadcasts to all connected peers

I also set up async tasks and internal channels (message queues) for
sending messages to all connected peers. This allows us to respond to
any received messages or any other messages a node needs to broadcast
to the network. For example, a share received from the mining hardware
can be broadcast to all connected peers - which then will be forwarded
by connected peers using our gossip protocol.

## Configuration

We also have simpler and cleaner configuration file parsing. We now
parse the configuration file using toml and serde crates, with
simplified ways to fill in defaults.

Rust ecosystem has evolved a lot and there are crates for parsing and
writing configurations, but I don't think we really need those any
more, given well supported and tested toml and serde libraries.

## Note on Rust ecosystem

There are a fair few libraries/crates out there with very poor test
coverage. One has to be very wary on which crates one adds as a
dependency. However, this also means there is a lot to be build for
the Rust ecosystem.
