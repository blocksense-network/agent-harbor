#!/usr/bin/env sh
if [ "$(uname)" = "Linux" ]; then
    cargo build --package agentfs-fuse-host --features fuse
else
    echo "Skipping FUSE tests on non-Linux platform ($(uname))"
fi
