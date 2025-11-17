#!/usr/bin/env sh
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

PROFILE="${FUSE_BUILD_PROFILE:-debug}"

if [ "$(uname)" = "Linux" ]; then
  if [ "$PROFILE" = "release" ]; then
    cargo build --release --package agentfs-fuse-host --features fuse --bin agentfs-fuse-host
  else
    cargo build --package agentfs-fuse-host --features fuse --bin agentfs-fuse-host
  fi
else
  echo "Skipping FUSE host build on non-Linux platform ($(uname))"
fi
