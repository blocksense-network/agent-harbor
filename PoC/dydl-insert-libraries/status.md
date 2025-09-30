### Overview

This document tracks the implementation status of the DYLD Insert Libraries Proof of Concept and serves as the single source of truth for the execution plan, milestones, automated success criteria, and cross‑team integration points.

Goal: Validate that `DYLD_INSERT_LIBRARIES` works as expected for implementing user-space interception and redirection in macOS sandboxing. Demonstrate network isolation mechanisms and filesystem API redirection to AgentFS RPC endpoints, proving the viability of dynamic library injection as a sandboxing primitive.

Approach: Build a minimal test harness that can inject custom libraries into processes launched by a parent, then expand to intercept network APIs (for localhost isolation) and filesystem APIs (for AgentFS integration). Use Rust for the injection harness and C for the interposed libraries to ensure ABI compatibility.

### Component and file layout (parallel tracks)

- `injector/`: Rust binary that sets `DYLD_INSERT_LIBRARIES` and launches target processes
- `lib/network-interpose.dylib`: C library that intercepts `bind()`, `connect()` for network isolation
- `lib/fs-interpose.dylib`: C library that intercepts filesystem APIs (`open()`, `read()`, etc.) and redirects to AgentFS RPC
- `harness/`: Test harness scripts for launching processes with injection
- `tests/`: Integration tests validating interception behavior
- `agentfs-rpc-client/`: Minimal client for communicating with AgentFS RPC server

All components target macOS with standard C ABI. Libraries are built as dynamic libraries with proper macOS linkage.

### Milestones and tasks (with automated success criteria)

**M1. Basic injection harness and library loading** (1–2d)

- **Deliverables:**

  - Rust injector binary that can set `DYLD_INSERT_LIBRARIES` for child processes
  - Minimal test library that logs when loaded into target processes
  - Shell script demonstrating injection works for processes launched by a specific parent

- **Verification:**

  - [ ] Injector can launch `sleep 1` with library injection and library logs successful loading
  - [ ] Library is loaded only in child processes, not in parent injector
  - [ ] Multiple libraries can be injected simultaneously
  - [ ] Injection fails gracefully when library doesn't exist

**Implementation Details:**

- Built Rust injector using `std::process::Command` with environment variable setting
- Created minimal C library with constructor function that logs to stderr
- Added proper macOS dynamic library compilation with `-dynamiclib` flag
- Implemented process tree tracking to ensure injection only affects child processes

**Key Source Files:**

- `injector/src/main.rs` - Main injector binary with DYLD_INSERT_LIBRARIES setup
- `lib/test-interpose.c` - Minimal test library with logging constructor
- `harness/basic-injection.sh` - Shell script demonstrating injection workflow

**Verification Results:**

- [ ] Basic injection test passes - library loads and logs in child processes
- [ ] Parent process isolation maintained - injector itself not affected
- [ ] Error handling validated - missing libraries produce clear errors

**M2. Network API interception and localhost isolation** (3–4d)

- **Deliverables:**

  - `lib/network-interpose.dylib` implementing the three localhost strategies from macOS sandboxing spec:
    - Strategy A: Fail with error for non-allowed ports/devices
    - Strategy B: Rewrite to alternative loopback device (e.g., 127.0.0.2)
    - Strategy C: Rewrite to alternative port via shared memory mapping
  - Test harness demonstrating curl connection rewriting
  - Environment variable configuration for allowed ports/devices

- **Verification:**

  - [ ] `curl http://127.0.0.1:8080` fails with clear error message when using Strategy A
  - [ ] `curl http://127.0.0.1:8080` gets rewritten to `127.0.0.2:8080` with Strategy B
  - [ ] Port mapping via shared memory works for Strategy C
  - [ ] Existing tools like curl work transparently with injection

**Implementation Details:**

- Implemented interposition library using macOS `DYLD_INTERPOSE` mechanism
- Created shared memory segment for port mapping in Strategy C
- Added environment variable parsing for configuration (LISTENING_BASE_PORT, LISTENING_LOOPBACK_DEVICE)
- Built test server on alternative loopback device to validate rewriting
- Integrated with existing `curl` binary without modification

**Key Source Files:**

- `lib/network-interpose.c` - Network API interception with three strategies
- `harness/network-test.sh` - Test script with curl and test servers
- `injector/src/network.rs` - Network-specific injection configuration

**Verification Results:**

- [ ] Strategy A error messages work - curl fails with descriptive error
- [ ] Strategy B rewriting works - connections redirected to alternative device
- [ ] Strategy C port mapping works - shared memory lookup functional
- [ ] Real tools integration validated - curl operates transparently

**M3. Filesystem API redirection to AgentFS RPC** (5–7d)

- **Deliverables:**

  - `lib/fs-interpose.dylib` intercepting major filesystem APIs (`open`, `read`, `write`, `close`, `stat`, etc.)
  - Minimal AgentFS RPC client library for communicating with AgentFS server
  - Complete redirection of filesystem operations to AgentFS RPC endpoints
  - Integration test showing processes can operate entirely through AgentFS

- **Verification:**

  - [ ] File creation/open operations redirected to AgentFS RPC
  - [ ] Read/write operations flow through AgentFS instead of host filesystem
  - [ ] Directory operations (readdir, mkdir, rmdir) work via RPC
  - [ ] Existing binaries like `ls`, `cat` work transparently with AgentFS backend
  - [ ] Performance acceptable for basic operations (within 10x of native)

**Implementation Details:**

- Implemented comprehensive filesystem API interposition using DYLD_INTERPOSE
- Created AgentFS RPC client with SSZ serialization matching AgentFS protocol
- Built file descriptor mapping layer to translate between local FDs and AgentFS handles
- Added proper error code translation from AgentFS Result types to POSIX errno
- Integrated with AgentFS control plane for session and branch management

**Key Source Files:**

- `lib/fs-interpose.c` - Filesystem API interception and redirection
- `agentfs-rpc-client/src/lib.rs` - Rust client for AgentFS RPC communication
- `harness/fs-redirection-test.sh` - Filesystem operation tests with AgentFS backend

**Verification Results:**

- [ ] Basic file operations redirected - open/read/write/close work through AgentFS
- [ ] Directory operations functional - ls, mkdir, etc. operate via RPC
- [ ] Real binary compatibility - standard tools work with AgentFS backend
- [ ] Performance benchmarks meet targets - acceptable overhead for sandboxing

### Risks & mitigations

- **DYLD restrictions:** `DYLD_INSERT_LIBRARIES` may not work with all system binaries or SIP-protected processes. Mitigate by documenting limitations and testing with common development tools.
- **ABI compatibility:** C library interposition must match macOS system call signatures exactly. Use system headers and extensive testing.
- **Performance overhead:** RPC redirection adds latency. Mitigate with caching and batching where possible.
- **Security boundaries:** Injected libraries run with target process privileges. Ensure libraries are minimal and don't introduce escalation paths.

### Parallelization notes

- M1 can proceed independently as foundation
- M2 depends on M1 completion for injection harness
- M3 can start in parallel with M2 once basic injection works
- All milestones can share the same test harness infrastructure

### References

- See [Local-Sandboxing-on-macOS.md](../specs/Public/Sandboxing/Local-Sandboxing-on-macOS.md) for localhost isolation strategies
- See [AgentFS Core.md](../specs/Public/AgentFS/AgentFS-Core.md) for RPC protocol details
- Reference code in [Using-DYLD_INSERT_LIBRARIES-for-Sandboxing.md](../specs/Research/Sandbox/macOS/Using-DYLD_INSERT_LIBRARIES-for-Sandboxing.md)
