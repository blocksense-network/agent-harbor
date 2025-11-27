# stackable-hooks Design Document

This document describes the implementation details and design rationales behind
stackable-hooks.

## Overview

stackable-hooks is a process-level function interposition library that provides:

- Stackable hook dispatch (multiple libraries hooking the same function)
- Per-library auto-propagation to child processes
- Cross-platform support (macOS and Linux)

The library evolved from the original redhook concept but has diverged significantly
with our own implementations for macOS stackable dispatch and auto-propagation.

## Architecture

### Platform-Specific Backends

The library uses different mechanisms on different platforms:

#### macOS (`dyld_insert_libraries.rs`)

On macOS, we use the dyld interposition mechanism via `__DATA,__interpose` sections:

1. **Interpose Entries**: Each hook creates a static interpose entry in `__DATA,__interpose`
   marked with `#[used]` to prevent dead-stripping by the linker.

2. **Shared Registry**: All libraries share a single registry per hooked function via
   the `canonical_symbol` mechanism. This uses `dlsym(RTLD_DEFAULT)` to find the
   canonical instance of a symbol across all loaded libraries.

3. **Dispatch Function**: One library wins the dyld interpose slot and becomes the
   "dispatcher". The dispatcher traverses the shared hook registry and calls each
   registered hook in priority order before finally calling the real system function.

4. **Symbol Resolution**: Uses `NSLookupSymbolInImage` and `_dyld_image_count` to
   scan through loaded images and find the original system function, skipping any
   interpose trampolines.

**Key Benefits:**

- Deterministic ordering (by priority, then registration order)
- Resilient to libraries loading/unloading
- Only one actual interpose needed (the dispatcher)

##### Dispatcher Selection Mechanism

A crucial detail of the macOS implementation is that **all libraries carry dispatcher logic**,
but only one library's dispatcher actually intercepts calls:

1. **Code Generation**: When multiple libraries hook the same function (e.g., `write`), each
   library's `hook!` macro generates:
   - A static interpose entry: `__stackable_interpose_write` in `__DATA,__interpose`
   - A dispatcher function: `__stackable_dispatch_write`
   - A shared state symbol: `__STACKABLE_SHARED_write`
   - A registration function that runs during library initialization

2. **OS-Level Selection**: When the process starts and libraries are loaded, macOS dyld
   examines all `__DATA,__interpose` sections across all loaded libraries. For each
   hooked function, dyld picks **one** library's interpose entry to actually intercept
   calls to that function. The selection is typically based on load order (first loaded
   wins), but is ultimately determined by OS components and not under application control.

3. **Finding the Canonical State**: All libraries (both winner and non-winners) need to
   coordinate through a shared hook registry. This is accomplished via `canonical_symbol()`:

   ```rust
   canonical_symbol("__STACKABLE_SHARED_write\0", &mut __STACKABLE_SHARED_write)
   ```

   This function uses `dlsym(RTLD_DEFAULT, symbol_name)` to search across all loaded
   libraries and return the **first** instance of the symbol found in the global namespace.
   Since all libraries export the same symbol name, they all get a pointer to the same
   instance (whichever library was loaded first).

4. **Shared Registry**: Because all libraries use `canonical_symbol()` to get the shared
   state, they all see the same:
   - Hook registry (linked list of hook nodes)
   - Lock protecting the registry
   - Cached pointer to the real function

5. **Hook Registration**: During library initialization, each library:
   - Calls `canonical_symbol()` to get the shared state
   - Locks the registry
   - Adds its hook node to the linked list
   - Unlocks the registry

   This happens **regardless of whether that library won the interpose slot**.

6. **Dispatch Flow**: When a hooked function is called:
   - OS dyld redirects the call to the winning library's dispatcher
   - The dispatcher calls `canonical_symbol()` to get the shared state
   - It traverses the hook registry (which contains hooks from **all** libraries)
   - It calls each registered hook in priority order
   - Finally, it calls the real system function

**Example with 3 libraries:**

- Library A, B, C all hook `write` with different priorities
- All three generate identical dispatcher structure
- macOS dyld picks Library A's interpose entry (assuming A loaded first)
- All three libraries call `canonical_symbol()` and get Library A's shared state
- B and C register their hooks in A's registry during initialization
- When `write` is called, Library A's dispatcher runs
- Library A's dispatcher traverses the registry and calls hooks from A, B, and C in priority order

This design ensures that:

- No coordination between libraries is needed at compile time
- Libraries can be loaded in any order
- All hooks execute regardless of which library won the interpose slot
- The hook execution order is deterministic (based on priorities, not load order)

#### Linux (`ld_preload.rs`)

On Linux with glibc, we use the classic `LD_PRELOAD` mechanism:

1. **dlsym(RTLD_NEXT)**: Each hook uses `dlsym(RTLD_NEXT)` to find the next function
   in the symbol resolution chain.

2. **No Shared Registry**: Unlike macOS, there's no shared registry. Each library's
   hook wraps the next in line according to `LD_PRELOAD` order.

3. **Simpler Dispatch**: The `call_next!` macro simply calls the function pointer
   obtained from `dlsym(RTLD_NEXT)`.

**Limitations:**

- Hook order determined by `LD_PRELOAD` order, not priorities
- No built-in mechanism for multiple hooks from different libraries to coordinate

### Hook Macro (`hook!`)

The `hook!` macro generates:

**On macOS:**

- A node type for the hook linked list
- A shared state type (lock + head pointer + cached real function pointer)
- Registration function (runs via `#[ctor::ctor]`)
- Dispatch function (the interpose target)
- Call-next function (traverses the chain)
- Call-real function (directly calls the original)
- The actual hook implementation
- Static interpose entry in `__DATA,__interpose`

**On Linux:**

- A function pointer fetched via `dlsym(RTLD_NEXT)`
- The hook implementation as a public `#[no_mangle]` function
- Call-next uses the saved function pointer

### Priority System

Hooks can specify a priority (integer value):

- Lower numbers = higher priority (execute first)
- Default priority is 0
- Auto-propagation hooks use priority 1000 (run last)

Priorities ensure predictable hook ordering even when libraries load in different orders.

### Reentrancy Protection

To prevent infinite recursion when hooks call system functions internally:

**macOS:**

- Thread-local `HOOK_DEPTH` counter
- `HookGuard` increments on entry, decrements on drop
- `hooks_allowed()` checks if depth is 0
- `with_reentrancy()` temporarily decrements depth for a specific call

**Usage Pattern:**

```rust
if !stackable_hooks::hooks_allowed() {
    // Call real function directly to avoid recursion
}
```

## Auto-Propagation System

### Design Goals

1. **Per-library granularity**: Each library independently chooses propagation
2. **Zero allocations**: Use static linked list nodes (like hook registry)
3. **Thread-safe**: Use pthread mutex and atomic flags
4. **Cross-library coordination**: Share registry via canonical symbols

### Implementation

#### Propagation Registry

```rust
struct PropagationNode {
    next: *mut PropagationNode,
    library_path: *const libc::c_char,
    enabled: AtomicBool,
}

struct PropagationRegistry {
    lock: libc::pthread_mutex_t,
    head: *mut PropagationNode,
}
```

Each library gets one static `__STACKABLE_PROPAGATION_NODE`. During library
initialization (`#[ctor::ctor]`):

1. **Capture Library Path**: Use `dladdr()` on a symbol from the library to get
   `dli_fname` (the library's filesystem path)

2. **Register in Shared Registry**: Get the canonical registry via `canonical_symbol()`
   (macOS) or `dlsym(RTLD_DEFAULT)` (Linux), lock it, and prepend our node to the list

3. **Node Stays Resident**: The node lives in static memory for the library's lifetime

#### Enabling Auto-Propagation

When a library calls `enable_auto_propagation()`:

- Sets the `enabled` atomic flag in its own node to `true`
- Other libraries are unaffected

#### Subprocess Spawning Hooks

Low-priority hooks (priority 1000) on subprocess functions:

- `execve`, `execvp`, `execv`, `execvpe` (Linux), `execveat` (Linux)
- `posix_spawn`, `posix_spawnp`

**Hook Flow:**

1. Lock the propagation registry
2. Traverse the linked list of nodes
3. Collect paths where `enabled.load(Ordering::Acquire)` is `true`
4. Unlock the registry
5. Join paths with `:` (Unix) or `;` (Windows)
6. Create new environment array with modified `LD_PRELOAD`/`DYLD_INSERT_LIBRARIES`
7. Call next hook or real function with modified environment
8. Free the temporary environment array

#### Memory Management

Environment modification:

- `modify_envp_with_injection()` - Allocates new envp array via `libc::malloc`
- Duplicates each string via `libc::strdup`
- Returns new NULL-terminated array
- `free_modified_envp()` - Frees each string and the array itself

This ensures the modified environment doesn't reference temporary Rust data.

### Rationale for Per-Library Tracking

**Why not just propagate everything?**

- Some shims may be debugging/development tools that shouldn't propagate
- Performance: Avoid injecting unnecessary libraries
- Security: Minimize attack surface in child processes
- Flexibility: Libraries can dynamically enable/disable propagation

**Example:**

- Library A: Production monitoring - wants propagation
- Library B: Debug tracer - doesn't want propagation
- Library C: Security audit - wants propagation

With per-library tracking, A and C propagate while B doesn't.

## Testing Strategy

### Unit Tests

- Atomic flag operations (enable/disable)
- Build separate test shims to verify isolation

### Integration Tests (`tests/e2e-stackable-hooks`)

- Multiple shims with different priorities
- Verify hook execution order
- Test `call_real!` bypassing remaining hooks
- Test auto-propagation enabled vs disabled
- Verify per-library propagation

### Test Programs

- `test-program`: Exercises hooks on file operations
- `propagation-test`: Parent spawns child to test propagation
- `call_real_demo`: Demonstrates bypassing hooks

## Performance Considerations

### Hook Dispatch Overhead (macOS)

Per hooked function call:

1. Atomic load to check if hooks enabled (~1-2 ns)
2. Traverse linked list of hooks (N \* pointer deref)
3. Each hook's custom logic
4. Final real function call

With 3 hooks, typical overhead: ~50-100ns (vs ~10ns for direct call).

### Auto-Propagation Overhead

Per subprocess spawn:

1. Lock acquisition
2. Linked list traversal (M libraries)
3. String concatenation
4. Environment array duplication (K environment variables)
5. Unlock

Typical overhead for 3 libraries, 50 env vars: ~10-20μs.
This is negligible compared to process spawn time (~1-10ms).

### Memory Footprint

Per library using stackable-hooks:

- One `PropagationNode`: ~24 bytes (3 pointers + atomic bool)
- Per hooked function: ~80-120 bytes (node + shared state)

For a library with 10 hooks: ~1KB static memory.

## Platform-Specific Notes

### macOS SIP (System Integrity Protection)

SIP prevents `DYLD_INSERT_LIBRARIES` from affecting system binaries.
Workarounds:

- Disable SIP (not recommended for production)
- Use code signing entitlements
- Only hook non-system processes

### Linux seccomp

Some applications use seccomp to restrict syscalls. Hook installation may fail or
behave unexpectedly if:

- `dlopen` is blocked
- `mprotect` is restricted
- Syscalls are filtered

Test thoroughly in the target environment.

## Future Enhancements

Potential improvements:

1. **Async-signal-safe hooks**: Support hooking from signal handlers
2. **Hook statistics**: Track call counts, timing per hook
3. **Conditional hooks**: Enable/disable hooks based on runtime conditions
4. **Windows support**: Implement using Detours or similar
5. **Hot patching**: Add/remove hooks at runtime (currently only via library load/unload)

## References

- [dyld interposition documentation](https://github.com/apple-oss-distributions/dyld/blob/main/include/mach-o/dyld-interposing.h)
- [LD_PRELOAD on Linux](https://man7.org/linux/man-pages/man8/ld.so.8.html)
- [Original redhook concept](https://github.com/geofft/redhook)
- [POSIX exec family](https://pubs.opengroup.org/onlinepubs/9699919799/functions/exec.html)
- [POSIX spawn](https://pubs.opengroup.org/onlinepubs/9699919799/functions/posix_spawn.html)
