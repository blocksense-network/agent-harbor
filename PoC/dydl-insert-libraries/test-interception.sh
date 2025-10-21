#!/bin/bash
echo "Testing SSZ client with interception..."
AGENTFS_SERVER=/tmp/agentfs.sock AGENTFS_ENABLED=1 ./injector/target/release/dyld-injector --library ./rust-client/target/release/libagentfs_rust_client.dylib touch /agentfs/test.txt 2>&1
echo "Testing C client with interception..."
AGENTFS_SERVER=/tmp/agentfs.sock AGENTFS_ENABLED=1 ./injector/target/release/dyld-injector --library ./lib/fs-interpose.dylib touch /agentfs/test.txt 2>&1
