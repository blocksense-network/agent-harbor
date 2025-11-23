# e2e-stackable-hooks

Comprehensive end-to-end tests for the `stackable-interpose` crate, demonstrating priority-based hook ordering.

## Overview

This test crate provides integration tests that verify the `stackable-interpose` library's functionality, including its priority system for ordering multiple hooks:

1. **Test Program**: A binary that performs system calls (`write`, `read`, `open`)
2. **Shim Libraries**: Two shared libraries with different priorities that hook system calls
3. **Integration Tests**: Tests that verify hook loading, execution order, and priority handling

## Priority System

The `stackable-interpose` library supports priority-based hook ordering:

- **Lower priority numbers = Higher priority** (priority 10 executes before priority 20)
- Priority is specified: `hook! { priority: 10, unsafe fn write(...) => ... }`
- When multiple libraries hook the same function, hooks execute in priority order
- Each hook can call:
  - `stackable_interpose::call_next!()` to continue to the next hook in priority order
  - `stackable_interpose::call_real!()` to bypass all remaining hooks and call the original function directly

## Components

- `src/bin/test_program.rs`: Test program that performs various system calls
- `tests/e2e-shim-a/src/lib.rs`: Library A (priority 5) - hooks `read()` and `close()`
- `tests/e2e-shim-b/src/lib.rs`: Library B (priority 20) - hooks `open()` and `close()`
- `tests/integration_tests.rs`: Integration tests verifying hook behavior

## Running Tests

```bash
# Run just these tests
cargo test

# Run with the project's test infrastructure
just test-rust
```

## Test Scenarios

1. **Without Hooks**: Verifies normal program execution
2. **With Library A**: Tests priority 5 hooks
3. **With Library B**: Tests priority 20 hooks
4. **Priority Demonstration**: Shows correct priority assignment and ordering with `call_next!`
5. **Call Real Bypass**: Demonstrates `call_real!` bypassing other hooks in the chain
6. **Application Call Real**: Demonstrates `call_real!` usage from application code to bypass loaded hooks

## Hook Execution Order

### Priority Chain Execution (`call_next!`)

For the `close()` function in priority tests (hooked by both libraries):

- Library A (priority 5) executes first
- Library B (priority 20) executes second
- Final hook calls the original `close()` system function

### Hook Bypass Execution (`call_real!`)

For the `close()` function in call_real tests:

- Library A (priority 5) executes first and calls `call_real!()` to bypass Library B
- Library B's hook is never executed
- Original `close()` system function is called directly
- Verification confirms the real system call succeeded (file descriptor becomes invalid)

### Direct Bypass from Application Code

The `call_real!` macro can also be used directly from application code (not just within hooks) to bypass all interposed functions:

```rust
use stackable_interpose;

// This will call the real system function, bypassing any hooks
let result = call_real!(read, fd, buf.as_mut_ptr(), count);
```

This requires that hook libraries are loaded (via `DYLD_INSERT_LIBRARIES` or linked at compile time) since they export the necessary infrastructure functions. It's useful when application code needs to ensure it's calling the original system function without any hook interference.

Both tests verify that the real system functions are actually called, not just that hooks execute correctly.
