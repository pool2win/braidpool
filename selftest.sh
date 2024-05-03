#!/bin/sh

cd node
cargo build
RUST_LOG=info cargo run &
sleep 1
RUST_LOG=info cargo run -- --config-file=config.1.toml &
sleep 1

echo
echo ">>> Press any key to exit"
read

killall target/debug/run
