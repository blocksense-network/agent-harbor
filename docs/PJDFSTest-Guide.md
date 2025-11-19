# PJDFSTest Guide

This guide covers the essential workflows for running POSIX filesystem tests against AgentFS FUSE. The mount helper now passes `--allow-other` by default so every pjdfstest process (which runs as many different UIDs) can reach the filesystem; export `AGENTFS_FUSE_ALLOW_OTHER=0` before invoking a `just` target if you need to revert to the legacy single-user behavior.

## Quick Start

### Run the Full Test Suite

```bash
just test-pjdfstest-suite
```

This automatically:

- Sets up pjdfstest (if needed)
- Builds FUSE binaries
- Mounts AgentFS
- Runs all 18 test categories
- Unmounts and cleans up

### Smart Auto-Mounting

Individual test commands automatically mount AgentFS if the mount point isn't already mounted:

```bash
# Auto-mounts if /tmp/agentfs is not mounted
just pjdfs-file unlink/00.t

# Uses existing mount if /tmp/agentfs is already mounted
just pjdfs-file chmod/01.t

# Pre-mount once for multiple tests (avoids mounting overhead)
just mount-fuse /tmp/agentfs
just pjdfs-file unlink/00.t  # Uses existing mount
just pjdfs-cat chmod        # Uses existing mount
just umount-fuse /tmp/agentfs
```

### Run Individual Failing Tests

When the full suite fails, identify and run individual failing tests:

#### 1. Find Failing Tests

Look for test failures in the output. Failed tests show like:

```
not ok 7 - tried '-u 65534 -g 65534 symlink test pjdfstest_...', expected EACCES, got EIO
```

#### 2. Run Specific Test Files

```bash
# Run a specific failing test
just pjdfs-file symlink/05.t

# Run multiple failing tests
just pjdfs-file symlink/05.t
just pjdfs-file symlink/06.t
```

#### 3. Run Entire Failing Categories

```bash
# Run all symlink tests (if multiple symlink tests fail)
just pjdfs-cat symlink
```

### Privileged Subset (chmod/12.t)

Linux automatically applies `nosuid`/`nodev` to any FUSE mount created by an unprivileged user, so `chmod/12.t` will always fail with `EPERM` before AgentFS can clear SUID bits. The full harness documents this in two phases:

1. The main run mounts AgentFS normally and skips the SUID-sensitive tests.
2. `scripts/test-pjdfstest-full.sh` then **remounts via `sudo`** (passwordless sudo required) and runs only the files listed in `PJDFSTEST_SUDO_TESTS` (defaults to `chmod/12.t`). The kernel still rejects those operations, but the privileged log captures the expected behavior.

If you need to trace the privileged pass, open `logs/pjdfstest-full-<ts>/fuse-host-priv.log` or rerun just the SUID files with:

```bash
PJDFSTEST_SUDO_TESTS="chmod/12.t" just test-pjdfstest-full
```

## Available Commands

### Primary Workflows

- `just test-pjdfstest-suite` - Full suite with auto-setup
- `just pjdfs-file <test-file>` - Single test file (auto-mounts if needed)
- `just pjdfs-cat <category>` - Test category (auto-mounts if needed)

### Utility Commands

- `just list-pjdfstest-categories` - List all test categories

### Advanced Usage

- `just run-pjdfstest /mount/point` - Manual control (requires mounted FS)
- `just run-pjdfstest /mount/point chmod/` - Run specific category
- `just run-pjdfstest /mount/point --all` - Run all categories

## Test Categories

The suite includes 18 test categories:

- `chflags chmod chown ftruncate granular link mkdir mkfifo mknod`
- `open posix_fallocate rename rmdir symlink truncate unlink utimensat`

## Debugging Workflow

1. **Run full suite**: `just test-pjdfstest-suite`
2. **Identify failing tests** from the TAP output (look for "not ok" lines)
3. **Run individual failing tests**: `just pjdfs-file <category>/<test>.t`
4. **Run failing categories**: `just pjdfs-cat <category>` (if multiple tests in a category fail)
5. **Fix issues** in AgentFS code
6. **Re-run failing tests** to verify fixes
7. **Re-run full suite** to ensure no regressions

## Examples of Common Failures

### Permission Errors

```
not ok 7 - tried '-u 65533 -g 65533 chmod ...', expected EPERM, got EACCES
not ok 10 - tried '-u 65534 -g 65534 chmod ...', expected EPERM, got EACCES
```

→ Test: `just pjdfs-file chmod/07.t`

### Missing Operations

```
not ok 3 - tried 'truncate ...', expected 0, got EOPNOTSUPP
not ok 8 - tried 'truncate ...', expected 0, got EOPNOTSUPP
not ok 14 - tried 'truncate ...', expected 0, got EOPNOTSUPP
```

→ Test: `just pjdfs-file truncate/00.t`

### Incorrect Error Codes

```
not ok 7 - tried '-u 65534 -g 65534 symlink ...', expected EACCES, got EIO
not ok 9 - tried '-u 65534 -g 65534 symlink ...', expected 0, got EEXIST
```

→ Test: `just pjdfs-file symlink/05.t`

## Tips

- Auto-mount commands (`pjdfs-*`) are most convenient for development
- Smart mounting: commands auto-mount if needed, or reuse existing mounts
- For efficiency: pre-mount once, then run multiple tests without remounting
- Test output shows TAP format results for easy parsing
- Categories help isolate issues (permission vs. operations vs. errors)
- All commands require root privileges (handled automatically via sudo)
