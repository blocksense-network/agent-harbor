# stackable-hooks

Process-level function interposition library for Unix systems, providing
stackable hook dispatch and automatic propagation to child processes.

## Features

- **Stackable dispatch** – Multiple libraries can hook the same function,
  and all hooks execute in priority order
- **Dead-strip proof** – Works reliably even with aggressive linker optimizations
- **Auto-propagation** – Per-library control over propagating hooks to child processes
- **Cross-platform** – Supports macOS (via `DYLD_INSERT_LIBRARIES`) and Linux (via `LD_PRELOAD`)

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
stackable-hooks = { path = "crates/stackable-hooks" }
ctor = "0.2"

[lib]
crate-type = ["cdylib"]
```

### Basic Hook

```rust
use stackable_hooks::hook;

hook! {
    unsafe fn write(
        fd: libc::c_int,
        buf: *const libc::c_void,
        len: libc::size_t
    ) -> libc::ssize_t => my_write {
        // Call the next hook in the chain (or the real function)
        let result = stackable_hooks::call_next!(fd, buf, len);

        // Your custom logic here
        if result > 0 {
            log_write(fd, buf, result as usize);
        }

        result
    }
}
```

### Priority Hooks

Control execution order with priorities (lower numbers execute first):

```rust
hook! {
    priority: 10,
    unsafe fn open(
        path: *const libc::c_char,
        flags: libc::c_int
    ) -> libc::c_int => my_open {
        stackable_hooks::call_next!(path, flags)
    }
}
```

### Attaching to Existing Dispatch

When another crate (such as the built-in auto-propagation hooks) already
defines the interposed symbol, use `hook!` to register additional logic
without redefining the exported function. This avoids duplicate symbol
definitions on Linux while keeping macOS behavior identical.

```rust
hook! {
    priority: 5,
    unsafe fn execve(
        pathname: *const libc::c_char,
        argv: *const *mut libc::c_char,
        envp: *const *mut libc::c_char
    ) -> libc::c_int => my_execve_observer {
        log_spawn(pathname);
        stackable_hooks::call_next!(pathname, argv, envp)
    }
}
```

### Calling Through the Chain

- `call_next!(args...)` - Calls the next hook in the chain, or the real function if no more hooks
- `call_real!(function_name, args...)` - Bypasses all remaining hooks and calls the original function directly

**Important:** `call_real!` can be called from **anywhere** in your code, not just inside hooks:

```rust
use stackable_hooks::call_real;

// From application code - bypasses all hooks
unsafe {
    let result = call_real!(read, fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
}
```

This is useful when you need to call the original function without triggering hooks, for example:

- Performance-critical paths that should bypass instrumentation
- Avoiding recursion when hooks internally need to call the same function
- Testing or debugging scenarios

### Controlling Hook Execution (macOS & Linux)

You can dynamically enable or disable all hooks at runtime:

```rust
// Disable all hooks temporarily
stackable_hooks::disable_hooks();

// ... perform operations without any hook interception ...

// Re-enable hooks
stackable_hooks::enable_hooks();
```

When hooks are disabled, all hooked functions call the original system functions directly.
This is useful for:

- Performance-critical sections where hook overhead is unacceptable
- Cleanup or shutdown sequences
- Debugging or troubleshooting

This API is available on both macOS (`DYLD_INSERT_LIBRARIES`) and Linux
(`LD_PRELOAD`). Hooks are enabled automatically during library initialization.
When disabled, calls immediately fall through to the original system functions.

## Auto-Propagation

Each library can independently choose whether to propagate to child processes.
This is useful for ensuring hooks apply across an entire process tree.

```rust
#[ctor::ctor]
fn init() {
    // Enable propagation for THIS library only
    stackable_hooks::enable_auto_propagation();
}
```

When enabled:

- Your library's path is registered in a shared registry
- Subprocess spawning functions (`execve`, `posix_spawn`, etc.) are hooked at low priority
- Only libraries that opted in are added to `LD_PRELOAD`/`DYLD_INSERT_LIBRARIES` for child processes
- Other libraries can independently choose their propagation preference

To disable:

```rust
stackable_hooks::disable_auto_propagation();
```

### Propagation Hook Configuration

- `propagation-hooks` (enabled by default) builds the low-priority subprocess
  hooks that keep `LD_PRELOAD`/`DYLD_INSERT_LIBRARIES` in sync for children.
- `propagation-hooks-env-control` (opt-in) enables a runtime guard that honors
  the `STACKABLE_PROPAGATION_HOOKS` environment variable. Set it to `0`, `false`,
  or `off` to disable the propagation hooks without rebuilding; omit it or set
  any other value to keep propagation enabled.

## Injecting Your Library

### macOS

```bash
DYLD_INSERT_LIBRARIES=/path/to/your/lib.dylib ./your-program
```

### Linux

```bash
LD_PRELOAD=/path/to/your/lib.so ./your-program
```

## License

BSD 2-Clause License. See [COPYING](COPYING) for details.

Original interposition concept inspired by similar libraries in the ecosystem.
Extended with stackable dispatch, auto-propagation, and macOS optimizations.
