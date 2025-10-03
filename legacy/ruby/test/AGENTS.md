# Test guidelines

- Always include a brief comment before each assertion explaining why the assertion is made. This helps future contributors understand the intent of the test.

Tests using `start_agent_task` should use filesystem-based Git remotes instead of internet URLs to ensure they can run offline without network dependencies. To simulate a remote repos, create temporary local bare repositories.

## Test Environment Setup

Before running tests that require filesystem snapshots or daemon functionality, ensure the test infrastructure is properly set up:

### Filesystem Testing Setup

Some tests require ZFS or Btrfs test filesystems to be available. Check the status with:

```bash
just check-test-filesystems
```

If the test filesystems are not set up, create them with:

```bash
just create-test-filesystems
```

### Daemon Testing Setup

Tests that interact with privileged filesystem operations require the AH filesystem snapshots daemon to be running. Check the daemon status with:

```bash
just legacy-check-ah-fs-snapshots-daemon
```

If the daemon is not running, start it with:

```bash
just legacy-start-ah-fs-snapshots-daemon
```

Stop the daemon when done:

```bash
just legacy-stop-ah-fs-snapshots-daemon
```

**Note:** The daemon requires sudo privileges and runs in the background. It handles privileged filesystem snapshot operations on behalf of the test suite.
