# stackable-hooks

`stackable-hooks` is our in-tree evolution of
[redhook](https://github.com/geofft/redhook). It keeps the familiar
`hook!` / `real!` macros but adds two critical macOS upgrades:

1. **Dead-strip proof interpose entries** – Every trampoline lives in
   `__DATA,__interpose` and is marked `#[used]`, so `-dead_strip` builds
   can’t discard our hooks.
2. **Stackable dispatch** – Multiple shim dylibs can hook the same
   symbol inside one process. Dyld still chooses a single trampoline, but
   all other shims register their hooks in a shared registry. The winning
   trampoline becomes the dispatcher and chains every registered hook
   before calling the real libc function.

## Usage

```toml
[dependencies]
stackable-hooks = { workspace = true }

[lib]
crate-type = ["cdylib"]
```

```rust
stackable_hooks::hook! {
    unsafe fn write(fd: libc::c_int, buf: *const libc::c_void, len: libc::size_t)
        -> libc::ssize_t => my_write {
        let result = stackable_hooks::call_next!(fd, buf, len);
        if result > 0 {
            forward_to_recorder(fd, buf, result as usize);
        }
        result
    }
}
```

On macOS every interposer shares the same dispatcher/registry pair, so
order is deterministic (registration order) yet resilient—any shim can
drop out without breaking the rest. On Linux / glibc targets the macros
fall back to the classic `LD_PRELOAD` behavior provided by redhook.

`stackable-hooks` inherits the BSD-2-Clause license. See
[COPYING](COPYING) for details.
