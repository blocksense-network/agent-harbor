#!/usr/bin/env sh
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

if [ "$(uname)" = "Linux" ]; then
  cargo build --package agentfs-fuse-host --features fuse
else
  echo "Skipping FUSE tests on non-Linux platform ($(uname))"
fi
