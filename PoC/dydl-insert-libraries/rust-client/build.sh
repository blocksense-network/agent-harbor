#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -e

echo "Building Rust AgentFS client library..."
cargo build --release

echo "Built librust_client.dylib successfully"
ls -la target/release/libagentfs_rust_client.dylib
