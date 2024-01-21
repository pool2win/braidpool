#!/bin/sh

cd node
cargo build
RUST_LOG=debug cargo run -- --bind=localhost:6680 &
sleep 1
RUST_LOG=debug cargo run -- --bind=localhost:6680 --addpeer=localhost:6680 &
sleep 1

echo
echo ">>> Press any key to exit"
read

killall braidpool-node
