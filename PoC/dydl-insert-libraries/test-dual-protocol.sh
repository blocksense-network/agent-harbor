#!/bin/bash
echo "Testing SSZ client..."
AGENTFS_SERVER=/tmp/agentfs.sock AGENTFS_ENABLED=0 ./injector/target/release/dyld-injector --library ./rust-client/target/release/libagentfs_rust_client.dylib echo "test"
echo "Testing C client..."  
AGENTFS_SERVER=/tmp/agentfs.sock AGENTFS_ENABLED=0 ./injector/target/release/dyld-injector --library ./lib/fs-interpose.dylib echo "test"

