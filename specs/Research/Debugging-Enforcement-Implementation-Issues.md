# Debugging Enforcement Implementation Issues Report

## Executive Summary

The implementation of Milestone 7 (Debugging toggles) for the Local Sandboxing on Linux project has encountered critical blocking issues related to Linux namespace capabilities and `/proc` filesystem mounting. Despite successful user namespace creation, the sandbox fails during filesystem setup due to insufficient privileges for mounting operations within the user namespace context.

## Problem Context

### Milestone 7 Objectives

- **Deliverables:**
  - Default deny ptrace/process*vm*\*; debug mode enables ptrace within sandbox only
- **Verification:**
  - E2E test: gdb attach works in debug mode
  - E2E test: gdb attach fails in normal mode (EPERM)
  - E2E test: host processes remain invisible from within sandbox (cannot ptrace host processes)
  - Unit tests: seccomp filter rules applied correctly in debug vs normal modes

### Implementation Status

- ✅ **Completed:** seccomp filter rules for debug mode, test binaries, unit tests, Justfile integration
- ❌ **Blocked:** E2E tests fail due to `/proc` mounting permission errors

## Technical Background

### Sandbox Architecture

The sandbox uses a multi-layer isolation approach:

1. **Namespace Creation:** `sandbox.start()` creates user, mount, PID, UTS, and IPC namespaces
2. **Filesystem Setup:** `fs_manager.setup_mounts()` configures filesystem isolation
3. **Process Execution:** `process_manager.exec_as_pid1()` executes the target command as PID 1

### The `/proc` Mounting Issue

In PID namespaces, a private `/proc` filesystem must be mounted to provide correct process visibility. The implementation attempts this in `crates/sandbox-core/src/process/mod.rs`:

```rust
fn mount_proc(&self) -> Result<()> {
    info!("Mounting /proc for PID namespace");

    // Unmount existing /proc (from host namespace)
    let _ = nix::mount::umount("/proc");

    // Mount new /proc (scoped to this PID namespace)
    mount(
        Some("proc"),
        "/proc",
        Some("proc"),
        MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC | MsFlags::MS_NODEV,
        None::<&str>,
    )
}
```

## Observed Problems

### Primary Issue: CAP_SYS_ADMIN Required

**Error:** `Failed to mount /proc: EPERM: Operation not permitted`

**Root Cause:** The `mount()` system call requires `CAP_SYS_ADMIN` capability, which is granted within user namespaces but not available when running the sandbox unprivileged.

**Sequence of Operations:**

1. ✅ `unshare(CLONE_NEWUSER | CLONE_NEWNS | CLONE_NEWPID | CLONE_NEWUTS | CLONE_NEWIPC)` succeeds
2. ✅ User namespace UID/GID mappings are established
3. ❌ `mount("proc", "/proc", "proc", ...)` fails with EPERM

### Namespace Creation Succeeds

Despite the mounting failure, namespace creation works correctly:

```bash
# This works fine (creates all namespaces)
unshare --user --mount --pid --uts --ipc sleep 1

# But mounting within the namespace context fails
```

### Existing Tests Handle This Gracefully

The overlay enforcement tests have logic to treat permission errors as "SKIPPED":

```rust
if let Some(1) = status.code() {
    println!("⚠️  {} test SKIPPED - likely due to insufficient privileges");
    Ok(()) // Treat as success (skipped)
}
```

So the overlay tests appear to "work" but actually skip the actual enforcement checks due to permissions.

### Definitive Mount Capability Test

Created a minimal test (`mount_test.rs`) that isolates the mount capability issue:

**Test Results:**

- ✅ User namespace creation: **SUCCESS**
- ❌ Mount operation (tmpfs): **EPERM: Operation not permitted**

**Code:**

```rust
// Creates user namespace, then immediately tries to mount tmpfs
match nix::sched::unshare(CloneFlags::CLONE_NEWUSER) {
    Ok(_) => info!("User namespace created successfully"),
    Err(e) => error!("Failed to create user namespace: {}", e),
}

// Try mount operation
let result = nix::mount::mount(Some("tmpfs"), "/tmp/test_mount", Some("tmpfs"), ...);
match result {
    Ok(_) => info!("✅ CAP_SYS_ADMIN available"),
    Err(e) if e.to_string().contains("EPERM") => error!("❌ CAP_SYS_ADMIN not available"),
    Err(e) => error!("Unexpected error: {}", e),
}
```

This definitively confirms that mount operations fail with EPERM within user namespaces when running unprivileged.

### Fork Implementation Results

Implemented the guru's recommended solution - moving `/proc` mounting to the child process after fork:

**Fork + Mount Results:**

- ✅ Fork succeeds (child enters PID namespace)
- ✅ `/proc` mounting attempted in child process
- ❌ `/proc` mounting still fails with EPERM

**Analysis:** Even though the child process is in the new PID namespace and has CAP*SYS_ADMIN in the user namespace, mounting `/proc` still fails. This suggests that user namespaces provide CAP_SYS_ADMIN for \_some* operations, but PID-namespace-specific mounts like `/proc` may have additional restrictions.

**Key Finding:** The issue is not just timing (pre-fork vs post-fork), but a fundamental limitation of what CAP_SYS_ADMIN allows within user namespaces for PID namespace operations.

## Attempts Made

### 1. Privilege Escalation Testing

**Attempt:** Run tests with `sudo just test-debugging`
**Result:** Tests pass completely, confirming the implementation works when privileged
**Limitation:** Not suitable for CI/CD or production deployment scenarios

### 2. Capability Analysis

**Attempt:** Investigate which specific capabilities are required
**Findings:**

- `CAP_SYS_ADMIN` is needed for `mount()` operations
- User namespaces provide this capability within their scope
- However, the parent process (running unprivileged) cannot grant this capability

### 3. Alternative Mounting Approaches

**Attempt:** Research if `/proc` mounting can be avoided or deferred
**Findings:**

- `/proc` mounting is mandatory for PID namespace isolation
- Without it, processes see host's global process table
- This breaks core sandbox security guarantees

### 4. Filesystem Setup Reordering

**Attempt:** Move `/proc` mounting to occur before user namespace creation
**Result:** Impossible - PID namespace creation must happen before `/proc` can be mounted for that namespace

### 5. Minimal Privilege Testing

**Attempt:** Test with minimal required privileges using `capsh`
**Result:** Even with carefully controlled capabilities, mounting still fails

## Impact Assessment

### Security Implications

- **No security degradation:** The failure prevents sandbox execution entirely
- **No privilege escalation risk:** Unprivileged processes cannot create functional sandboxes

### Development Workflow

- **CI/CD blocked:** Tests cannot run in standard environments
- **Local development:** Developers must use `sudo` for testing
- **Debugging milestone:** Cannot complete E2E verification requirements

### Alternative Approaches Considered

#### 1. Pre-mount `/proc` in Parent

**Feasibility:** Low

- Parent process cannot predict child namespace requirements
- Would break namespace isolation principles

#### 2. Use `pivot_root` Instead of Mount

**Feasibility:** Medium

- More complex filesystem setup
- Still requires mount capabilities
- May not solve the core privilege issue

#### 3. Kernel Module or setuid Helper

**Feasibility:** High but complex

- Create privileged helper for mount operations
- Maintains security through restricted interfaces
- Significant implementation overhead

#### 4. Skip `/proc` Mounting in Test Mode

**Feasibility:** High but compromises security

- Allow tests to run without full isolation
- Defeats purpose of security testing
- Not acceptable for production validation

## Environment Details

### System Information

- **OS:** Linux (NixOS)
- **Kernel:** 6.12.44
- **Architecture:** x86_64
- **User Namespaces:** Enabled
- **Cgroups:** v2 available

### Test Environment

- **Runner:** Standard user account (non-root)
- **Privileges:** No special capabilities granted
- **Filesystem:** Standard Linux filesystem permissions

### Build Environment

- **Rust:** 1.90.0
- **Dependencies:** nix crate for system calls
- **Testing Framework:** Standard Rust test harness

## Recommendations

### Immediate Actions

1. **Document the privilege requirement** in milestone status
2. **Update test expectations** to account for permission limitations
3. **Add environment detection** to skip tests when capabilities unavailable

### Long-term Solutions

1. **Investigate privileged helper daemon** for mount operations
2. **Research Linux capability delegation** mechanisms
3. **Consider alternative isolation approaches** that don't require mount privileges

### Implementation Guidance Needed

- How should we handle capability requirements in CI/CD?
- Is there a way to delegate mount capabilities from parent to child namespace?
- Should we implement a privileged helper service for sandbox setup?

## Files Affected

### Core Implementation

- `crates/sandbox-core/src/process/mod.rs` - `/proc` mounting logic
- `crates/sandbox-core/src/namespaces/mod.rs` - Namespace creation
- `crates/sbx-helper/src/main.rs` - Sandbox orchestration

### Tests

- `tests/debugging-enforcement/src/test_orchestrator.rs` - E2E test orchestration
- `tests/sandbox-integration/main.rs` - Integration test framework

### Build System

- `Justfile` - Test targets and build configuration
- `Cargo.toml` - Workspace package management

This report provides comprehensive context for senior developers to evaluate alternative approaches for resolving the capability requirements while maintaining sandbox security guarantees.
