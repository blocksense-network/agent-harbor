#!/bin/bash
set -e

echo "Building Rust AgentFS client library..."
cargo build --release

echo "Built librust_client.dylib successfully"
ls -la target/release/libagentfs_rust_client.dylib
