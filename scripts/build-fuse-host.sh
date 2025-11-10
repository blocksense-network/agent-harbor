#!/usr/bin/env sh
if [ "$(uname)" = "Linux" ]; then
    cargo build --package agentfs-fuse-host --features fuse --bin agentfs-fuse-host
else
    echo "Skipping FUSE host build on non-Linux platform ($(uname))"
fi
