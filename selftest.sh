#!/bin/sh

cd node
cargo build
RUST_LOG=debug cargo run -- --bind=localhost:25188 &
sleep 1
RUST_LOG=debug cargo run -- --bind=localhost:25189 --addpeer=localhost:25188 &
sleep 1

echo
echo ">>> Press any key to exit"
read

killall braidpool-node
