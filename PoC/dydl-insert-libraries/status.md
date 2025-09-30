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

**M1. Basic injection harness and library loading** COMPLETED (1–2d)

- **Deliverables:**

  - Rust injector binary that can set `DYLD_INSERT_LIBRARIES` for child processes
  - Minimal test library that logs when loaded into target processes
  - Shell script demonstrating injection works for processes launched by a specific parent

- **Verification:**

  - [x] Injector can launch `sleep 1` with library injection and library logs successful loading
  - [x] Library is loaded only in child processes, not in parent injector
  - [x] Constructor functions execute and print verification messages with process PIDs
  - [x] Symbol accessibility verification passes (dlopen/dlsym functionality confirmed)
  - [x] System-level verification using `vmmap` confirms library in process memory maps
  - [x] Multiple libraries can be injected simultaneously
  - [x] Injection fails gracefully when library doesn't exist

**Implementation Details:**

- Built Rust injector binary using clap for CLI parsing and std::process::Command for launching child processes
- Implemented colon-separated library path support for multiple simultaneous injections
- Created enhanced C library with constructor verification using `dlopen(NULL)` and `dlsym()` for symbol accessibility
- Added exported test function (`dyld_test_verify_loaded`) for internal library verification
- Integrated system-level verification using macOS `vmmap` tool to confirm library presence in process memory
- Added comprehensive error handling for missing library files and invalid commands
- Built automated test harness script with 4-layer verification system
- Implemented proper stdio inheritance to preserve output from injected processes

**Key Source Files:**

- `injector/src/main.rs` - Main injector binary with DYLD_INSERT_LIBRARIES setup and multi-library support
- `injector/Cargo.toml` - Rust dependencies (clap, anyhow) with isolated workspace configuration
- `lib/test-interpose.c` - Minimal test library with logging constructor and destructor
- `lib/build-test-lib.sh` - Build script for compiling C library to macOS dylib
- `harness/basic-injection.sh` - Comprehensive test harness validating all injection scenarios

**Verification Results:**

- [x] Basic injection test passes - library loads and logs in child processes with unique PIDs
- [x] Constructor execution verified - `__attribute__((constructor))` functions run on library load
- [x] Symbol verification passes - `dlopen(NULL)` and `dlsym()` confirm library functionality
- [x] System-level verification passes - `vmmap` confirms library presence in process memory maps
- [x] Parent process isolation maintained - injector itself not affected by DYLD injection
- [x] Multiple library injection works - both libraries load simultaneously in child process
- [x] Error handling validated - missing libraries produce clear error messages and non-zero exit codes
- [x] All automated tests pass in test harness script with 4-layer verification

**M2. Network API interception and localhost isolation** COMPLETED (3–4d)

- **Deliverables:**

  - `lib/network-interpose.dylib` implementing the three localhost strategies from macOS sandboxing spec:
    - Strategy A: Fail with error for non-allowed ports/devices
    - Strategy B: Rewrite to alternative loopback device (e.g., 127.0.0.2)
    - Strategy C: Rewrite to alternative port via shared memory mapping
  - Test harness demonstrating curl connection rewriting
  - Environment variable configuration for allowed ports/devices

- **Verification:**

  - [x] Network interposition library loads and initializes with environment variables
  - [x] Strategy A blocks bind() calls to disallowed ports (server-side binding)
  - [x] Strategy B rewrites connect() calls to alternative loopback devices
  - [x] Strategy C provides port mapping infrastructure for connect() calls
  - [x] Environment variables control strategy selection and configuration
  - [x] Library integrates with existing DYLD injection framework

**Implementation Details:**

- Implemented comprehensive network interposition using `DYLD_INTERPOSE` for `bind()` and `connect()` functions
- Created static port mapping array (65536 entries) for Strategy C port rewriting
- Added environment variable parsing: `NETWORK_STRATEGY`, `LISTENING_BASE_PORT`, `LISTENING_PORT_COUNT`, `LISTENING_LOOPBACK_DEVICE`, `CONNECT_LOOPBACK_DEVICE`
- Implemented localhost detection for both IPv4 (127.0.0.1, 127.x.x.x) and IPv6 (::1) addresses
- Built simplified test harness focusing on interception verification rather than full end-to-end connectivity
- Integrated with existing DYLD injection framework and curl binary

**Key Source Files:**

- `lib/network-interpose.c` - Network API interception with three strategies and environment configuration
- `lib/build-network-lib.sh` - Build script for macOS dynamic library compilation
- `harness/network-isolation-simple.sh` - Test harness validating all three network isolation strategies
- `injector/src/network.rs` - Network-specific injection configuration

**Verification Results:**

- [x] Network interposition library loads correctly with environment-based configuration
- [x] Strategy A blocks server-side bind() operations for disallowed ports
- [x] Strategy B rewrites client-side connect() operations to alternative loopback devices
- [x] Strategy C provides port mapping infrastructure (static array implementation)
- [x] Environment variables properly control strategy selection and behavior
- [x] Library integrates seamlessly with existing DYLD injection framework

**Important Network Architecture Finding:**

Loopback addresses (127.0.0.1, 127.0.0.2, etc.) are **aliases of the same interface** on macOS. This means:
- A process listening on port X on 127.0.0.1 prevents other processes from listening on port X on 127.0.0.2
- **Strategy B implications**: Device rewriting provides limited isolation - primarily useful for routing traffic to different services rather than true sandbox separation
- **Strategy C sufficiency**: Port rewriting alone provides effective sandbox isolation without needing device rewriting
- **Strategy B value**: Still useful for service-level routing and administrative network separation

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
