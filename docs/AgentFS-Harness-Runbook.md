# AgentFS Harness Runbook

This runbook documents the supported workflows for exercising the AgentFS
provider through the external test harness that backs the First Release **R4 –
AgentFS integration** milestone. Follow these steps when validating changes
locally or when reproducing CI runs that cover the provider matrix.

## 1. Prerequisites

- Enter the repo root and allow the Nix shell to activate (`direnv allow`).
- Ensure the AgentFS daemon and interpose shim build cleanly:

  ```bash
  just build-rust-test-binaries
  ```

  The command builds the `fs-snapshots-harness-driver` binary together with the
  AgentFS daemon, shim, and supporting crates.

## 2. Building the harness driver explicitly

When iterating on the harness itself you can rebuild just the driver:

```bash
cargo build -p fs-snapshots-test-harness \
    --bin fs-snapshots-harness-driver \
    --no-default-features \
    --features git,zfs,btrfs,agentfs
```

The binary is written to `target/debug/fs-snapshots-harness-driver`.

## 3. Running the provider matrix via `cargo test`

The R4 verification matrix is wired into the `ah-fs-snapshots` crate. Run the
suite from the repo root:

```bash
cargo test -p ah-fs-snapshots --test integration -- \
  test_git_provider_matrix \
  test_zfs_provider_matrix \
  test_btrfs_provider_matrix \
  test_agentfs_provider_matrix
```

Each test shell-outs to the harness driver and streams results through the
structured logger. Per-test output is captured under
`target/tests/ah-fs-snapshots/*`.

## 4. Running the harness driver directly

When debugging a single provider it is often faster to invoke the driver
manually:

```bash
target/debug/fs-snapshots-harness-driver provider-matrix --provider git
```

Replace `git` with `zfs`, `btrfs`, or `agentfs` as needed. The driver emits a
single-line status report to stdout while detailed traces go to
`target/tests/fs-snapshots-test-harness/*`.

## 5. AgentFS-specific setup (macOS)

The AgentFS provider requires the interpose shim to be preloaded so file system
calls are routed through `agentfs-daemon`. The daemon supervisor now exposes a
`mount_agentfs_interpose` RPC, so the macOS workflow is:

```bash
just start-ah-fs-snapshots-daemon

# Launch (or reconfigure) the interpose daemon via the supervisor and pin a
# per-run socket/runtime directory so every workspace/matrix shard is isolated
SOCKET_DIR="$(mktemp -d /tmp/agentfs-interpose.XXXXXX)"
SOCKET_PATH="$SOCKET_DIR/agentfs.sock"
target/debug/ah-fs-snapshots-daemonctl \
  interpose mount \
  --repo-root "$(pwd)/tests/fixtures/repos/provider-agentfs" \
  --socket-path "$SOCKET_PATH" \
  --runtime-dir "$SOCKET_DIR" \
  --json

# Export the shim variables using the reported socket path (which should match
# the hint passed above)
export AGENTFS_INTERPOSE_SOCKET="$SOCKET_PATH"
export AH_ENABLE_AGENTFS_PROVIDER=1
export DYLD_INSERT_LIBRARIES="$(pwd)/target/debug/libagentfs_interpose_shim.dylib"

target/debug/fs-snapshots-harness-driver provider-matrix --provider agentfs
```

Use `ah-fs-snapshots-daemonctl interpose status --json` to verify PID/socket
details (the CLI prints the same schema consumed by `scripts/check-ah-fs-snapshots-daemon.sh`).

## 6. Log locations and cleanup

- Harness driver logs: `target/tests/fs-snapshots-test-harness/*`
- Provider matrix logs (from `ah-fs-snapshots` tests):
  `target/tests/ah-fs-snapshots/*`
- AgentFS daemon logs honour the `ah-logging` configuration. Set
  `RUST_LOG=agentfs-daemon=debug` to increase verbosity when required.

Temporary sockets and mount points are cleaned automatically; if a run is
interrupted, remove the leftover paths under `/tmp` or the macOS `~/Library/Caches/ah`
tree.

## 7. Troubleshooting tips

- Use `FS_SNAPSHOTS_HARNESS_DEBUG=1` to keep the driver verbose without
  increasing global log levels.
- The harness will refuse to run if the driver binary is missing—rerun
  `just build-rust-test-binaries` to resolve.
- When AgentFS requests fail, consult the daemon log for structured warnings and
  errors (all emitted via `tracing`). The harness propagates the error string
  back to the test output for quick scanning.

This runbook will be extended with Windows/Linux parity steps in the follow-up
milestones that add WinFSP and FUSE coverage. For now it captures the macOS
workflow needed to validate the R4 Milestone 1 deliverables.
