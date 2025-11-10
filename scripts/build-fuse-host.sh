#!/usr/bin/env sh
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

if [ "$(uname)" = "Linux" ]; then
  cargo build --package agentfs-fuse-host --features fuse --bin agentfs-fuse-host
else
  echo "Skipping FUSE host build on non-Linux platform ($(uname))"
fi
