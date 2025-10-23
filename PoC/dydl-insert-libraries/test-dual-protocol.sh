#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

echo "Testing SSZ client..."
AGENTFS_SERVER=/tmp/agentfs.sock AGENTFS_ENABLED=0 ./injector/target/release/dyld-injector --library ./rust-client/target/release/libagentfs_rust_client.dylib echo "test"
echo "Testing C client..."
AGENTFS_SERVER=/tmp/agentfs.sock AGENTFS_ENABLED=0 ./injector/target/release/dyld-injector --library ./lib/fs-interpose.dylib echo "test"
