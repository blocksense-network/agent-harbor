#!/bin/bash
echo "Testing loading both C and Rust libraries simultaneously..."
AGENTFS_ENABLED=0 ./injector/target/release/dyld-injector --library ./lib/fs-interpose.dylib --library ./rust-client/target/release/libagentfs_rust_client.dylib echo "Both libraries loaded successfully"

