# Recorder E2E Test Scenarios

This directory contains end-to-end test scenarios for the `ah agent record` command and its integration with filesystem snapshots via IPC.

## Test Scenarios

### 1. `recorder_ipc_integration.yaml`

**Purpose**: Validate IPC communication between `ah agent record` and `ah agent fs snapshot`.

**Tests**:

- IPC server initialization when recording starts
- Snapshot notifications sent via Unix domain socket
- `REC_SNAPSHOT` records written to .ahr file
- Snapshot metadata written to .snapshots.jsonl sidecar
- Proper byte offset anchoring for each snapshot

**Expected Outcomes**:

- 4 snapshots created (write, write, append, replace)
- Each snapshot has a corresponding REC_SNAPSHOT record
- Byte offsets are monotonically increasing
- .snapshots.jsonl contains all 4 snapshot entries

**Running**:

```bash
# With mock agent
ah agent record -- python -m src.cli run \
  --scenario scenarios/recorder_ipc_integration.yaml \
  --workspace /tmp/test-ws \
  --fast-mode

# Verify recording
ah agent replay recording-*.ahr --fast
```

### 2. `recorder_replay_test.yaml`

**Purpose**: Validate recording format integrity and replay functionality.

**Tests**:

- PTY byte capture with transparent input forwarding
- Brotli block compression and writing
- vt100 terminal state tracking
- Deterministic replay from .ahr file
- Snapshot records interleaved with data records

**Expected Outcomes**:

- .ahr file created with valid block headers
- At least 1 Brotli-compressed block
- Data records for PTY output
- Snapshot records for filesystem operations
- Replay produces identical output

**Running**:

```bash
# Record session
ah agent record --out-file test.ahr -- python -m src.cli run \
  --scenario scenarios/recorder_replay_test.yaml \
  --workspace /tmp/test-ws \
  --fast-mode

# Replay session
ah agent replay test.ahr --fast

# Verify block structure
ah agent replay test.ahr --print-meta
```

### 3. `snapshot_anchoring_precision.yaml`

**Purpose**: Validate byte-level anchoring precision for snapshots.

**Tests**:

- Precise byte offset tracking for each snapshot
- Monotonic byte offset progression
- Temporal ordering of snapshot anchors
- Correlation between PTY output and snapshot timing

**Expected Outcomes**:

- 4 snapshots with distinct byte offsets
- Each snapshot anchor_byte > previous snapshot's anchor_byte
- Timestamp spacing matches scenario timing (>= 50ms)
- No anchor collisions or out-of-order anchors

**Running**:

```bash
# Record with detailed logging
RUST_LOG=ah_recorder=debug ah agent record -- python -m src.cli run \
  --scenario scenarios/snapshot_anchoring_precision.yaml \
  --workspace /tmp/test-ws \
  --fast-mode

# Check anchor precision
jq '.anchor_byte' recording-*.snapshots.jsonl | sort -n
```

### 4. `recorder_error_handling.yaml`

**Purpose**: Validate error handling and resilience in recording system.

**Tests**:

- Graceful handling of IPC connection failures
- Continued recording when snapshot notifications fail
- Non-blocking snapshot operations
- Recovery from transient errors

**Expected Outcomes**:

- Recording continues even if IPC fails
- All file operations complete successfully
- .ahr file remains valid and replayable
- Warning logs for IPC failures (not errors)

**Running**:

```bash
# Test with simulated IPC failures (remove socket mid-recording)
ah agent record -- bash -c 'python -m src.cli run --scenario scenarios/recorder_error_handling.yaml --workspace /tmp/test-ws --fast-mode & sleep 1 && rm -f /tmp/*.sock'

# Verify recording integrity despite IPC issues
ah agent replay recording-*.ahr --fast
```

## Running All Tests

Use the integration test runner to execute all recorder scenarios:

```bash
cd tests/tools/mock-agent
python run_integration_tests.py --filter recorder_
```

## Test Infrastructure

### IPC Server

The IPC server is automatically started by `ah agent record`:

- **Default socket path**: `<recording-dir>/ipc.sock`
- **Protocol**: Length-prefixed SSZ with tagged unions
- **Environment variable**: `AH_RECORDER_IPC_SOCKET` (set by record command)

### Mock Agent Integration

The mock agent uses the checkpoint command feature to trigger snapshots:

```bash
# Internal mechanism (automatic in scenarios)
python -m src.cli run \
  --scenario <scenario.yaml> \
  --checkpoint-cmd "ah agent fs snapshot"
```

When running under `ah agent record`, the checkpoint command detects the `AH_RECORDER_IPC_SOCKET` environment variable and sends IPC notifications.

## Verification

### Manual Verification Steps

1. **Check .ahr file structure**:

   ```bash
   hexdump -C recording-*.ahr | head -20
   # Should see: 41 48 52 43 (AHRC magic bytes)
   ```

2. **Verify snapshot records**:

   ```bash
   # Count REC_SNAPSHOT records (tag=0x04)
   ah agent replay recording-*.ahr --print-meta | grep -c "REC_SNAPSHOT"
   ```

3. **Check .snapshots.jsonl sidecar**:

   ```bash
   cat recording-*.snapshots.jsonl | jq '.id, .anchor_byte, .label'
   ```

4. **Validate byte offsets**:
   ```bash
   # Ensure monotonic progression
   jq -s 'map(.anchor_byte) | sort | . == (. | unique)' recording-*.snapshots.jsonl
   ```

### Automated Test Assertions

Integration tests should verify:

- [ ] .ahr file exists and has correct magic bytes
- [ ] At least N blocks written (based on scenario)
- [ ] REC_SNAPSHOT records count matches expected snapshots
- [ ] .snapshots.jsonl has correct number of entries
- [ ] Byte offsets are strictly monotonic
- [ ] Timestamps correlate with scenario timeline
- [ ] Replay produces matching output
- [ ] IPC failures don't crash the recorder

## Future Test Scenarios

Additional scenarios to implement:

- **Large file operations**: Test with files > block flush threshold (256KB)
- **Concurrent snapshots**: Rapid-fire snapshot requests
- **Terminal resizing**: Window size changes during recording
- **Multi-provider**: Test ZFS, Btrfs, and Git providers in sequence
- **Time travel branching**: Snapshot → branch → resume workflow
