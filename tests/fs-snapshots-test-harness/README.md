# Filesystem Snapshots Test Harness

This crate provides integration tests for filesystem snapshot providers. The tests exercise the full provider lifecycle including workspace preparation, snapshot creation, readonly mounting, and cleanup.

## Debugging Provider-Matrix Tests

When provider-matrix tests fail, follow these steps to debug:

### 1. Run the Failing Test Individually

```bash
just test-rust-single <test_name>
```

For example:

```bash
just test-rust-single agentfs_provider_matrix_runs_successfully
```

### 2. Examine the Test Output for Session IDs

The test will print the provider-matrix stdout/stderr output, which includes session IDs generated during the test run. Look for lines like:

```
SESSION_ID_LOGGING: ah-fs-snapshots-daemon generated session_id=corr-0
SESSION_ID_LOGGING: agentfs-daemon received session_id=corr-0
```

### 3. Check Daemon Log Files

Session IDs help correlate operations across the two daemon log files:

**Platform-specific log locations:**

- **macOS**: `~/Library/Logs/agent-harbor/`
- **Linux**: `~/.local/share/agent-harbor/`
- **Windows**: `%APPDATA%\agent-harbor\`

**Log files:**

- **FS Snapshots Daemon**: `{log-dir}/ah-fs-snapshots-daemon.log`
- **AgentFS Daemon**: `{log-dir}/agentfs-daemon.log`

Search for the session ID in both files (examples):

**macOS:**

```bash
grep "corr-0" ~/Library/Logs/agent-harbor/*.log
```

**Linux:**

```bash
grep "corr-0" ~/.local/share/agent-harbor/*.log
```

**Windows:**

```cmd
findstr "corr-0" %APPDATA%\agent-harbor\*.log
```

### 4. Analyze the Operation Timeline

The logs will show:

- Client connections and request handling
- Daemon startup and configuration
- File system operations (snapshot creation, mounting, cleanup)
- Error conditions and failure points

### 5. Common Failure Patterns

- **"matrix readonly mount missing workspace marker"**: The readonly mount didn't preserve the test file created in the workspace
- **Daemon handshake failures**: Check that daemons are properly configured and can communicate
- **Permission errors**: Ensure the test has appropriate privileges for filesystem operations

### 6. Environment Variables for Debugging

Set `FS_SNAPSHOTS_HARNESS_DEBUG=1` to enable additional debug output from the harness driver.

## Test Structure

- `tests/smoke.rs`: Basic provider-matrix tests for each supported filesystem
- `src/scenarios.rs`: Provider-matrix test logic shared across providers
- `src/lib.rs`: Helper functions for test setup and validation

## Running Tests

```bash
# Run all provider-matrix tests
just test-rust

# Run specific provider test
just test-rust-single <provider>_provider_matrix_runs_successfully

# Run with verbose output
RUST_LOG=debug just test-rust-single agentfs_provider_matrix_runs_successfully
```
