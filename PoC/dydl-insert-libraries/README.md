# DYLD Insert Libraries PoC

Proof of Concept for validating `DYLD_INSERT_LIBRARIES` as a sandboxing primitive on macOS.

## Overview

This PoC demonstrates the feasibility of using dynamic library injection via `DYLD_INSERT_LIBRARIES` for implementing user-space interposition and redirection in macOS sandboxing. The implementation includes:

1. **Basic injection harness** - Rust binary that can inject libraries into child processes
2. **Network isolation** - Interposition library implementing localhost connection strategies
3. **Filesystem redirection** - Comprehensive filesystem API interception to AgentFS RPC

## Quick Start

```bash
cd PoC/dydl-insert-libraries
./harness/basic-injection.sh
```

This will build all components and run the basic injection tests.

## Components

### Injector (`injector/`)
Rust binary that sets `DYLD_INSERT_LIBRARIES` for child processes.

**Usage:**
```bash
./injector/target/release/dyld-injector -l /path/to/library.dylib command [args...]
```

Multiple libraries can be specified with colon separation:
```bash
./injector/target/release/dyld-injector -l "lib1.dylib:lib2.dylib" command
```

### Test Library (`lib/`)
Minimal C library with logging constructor for testing injection.

**Build:**
```bash
cd lib
./build-test-lib.sh
```

### Test Harness (`harness/`)
Automated test scripts validating injection functionality.

**Run basic tests:**
```bash
./harness/basic-injection.sh
```

## Implementation Notes

- Libraries use `__attribute__((constructor))` to execute code on load
- Injector validates library existence before launching child processes
- Multiple library injection uses colon-separated paths (macOS standard)
- All components are macOS-specific and use standard C ABI
- Error handling includes clear messages for missing libraries and invalid commands

## Library Loading Verification

The PoC uses multiple layers of verification to ensure libraries are properly loaded:

### 1. Constructor Function Execution
- Libraries define `__attribute__((constructor))` functions that run when loaded
- Constructor prints verification messages to stderr with process PID
- Test harness counts messages to ensure library loads exactly once per child process

### 2. Symbol Accessibility Verification
- Libraries export a test function (`dyld_test_verify_loaded()`) that returns a known value
- Constructor uses `dlopen(NULL)` and `dlsym()` to verify the library's own symbols are accessible
- Confirms the library is fully loaded and functional, not just mapped into memory

### 3. System-Level Verification (Optional)
- Uses macOS `vmmap` tool to check if library appears in process memory maps
- Provides additional confidence that injection worked at the system level
- Skipped if process exits before verification can run

### 4. Process Isolation Verification
- Ensures libraries load only in child processes, not in the injector parent
- Verifies that `DYLD_INSERT_LIBRARIES` environment variable only affects launched processes

## Future Milestones

- **M2**: Network API interception with localhost isolation strategies
- **M3**: Filesystem API redirection to AgentFS RPC endpoints

## Related Specs

- [Local-Sandboxing-on-macOS.md](../../specs/Public/Sandboxing/Local-Sandboxing-on-macOS.md)
- [AgentFS Core.md](../../specs/Public/AgentFS/AgentFS-Core.md)
