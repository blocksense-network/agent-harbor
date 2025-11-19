// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shim library B for testing priority-based stackable-hooks functionality
//! Priority: 20 (lower priority than A)

#[cfg(target_os = "macos")]
use stackable_hooks::{enable_hooks, hook};
#[cfg(target_os = "macos")]
use tracing::info;

#[cfg(target_os = "macos")]
#[ctor::ctor]
fn init_hooks() {
    enable_hooks();
}

#[cfg(target_os = "macos")]
hook! {
    priority: 20,
    unsafe fn write(stackable_self, fd: libc::c_int, buf: *const libc::c_void, count: libc::size_t)
        -> libc::ssize_t => my_write {
        // Print a message to indicate the hook is active
        info!("SHIM_B: write() intercepted (priority 20), fd={}, count={}", fd, count);

        // Call the next hook in the chain or the real function
        let result = stackable_hooks::call_next!(stackable_self, write, fd, buf, count);

        // Log the result
        info!("SHIM_B: write() returned {}", result);

        result
    }
}

#[cfg(target_os = "macos")]
hook! {
    priority: 20,
    unsafe fn open(stackable_self, path: *const libc::c_char, flags: libc::c_int, mode: libc::mode_t)
        -> libc::c_int => my_open {
        // Print a message to indicate the hook is active
        info!("SHIM_B: open() intercepted (priority 20), flags={}, mode={}", flags, mode);

        // Call the next hook in the chain or the real function
        let result = stackable_hooks::call_next!(stackable_self, open, path, flags, mode);

        // Log the result
        info!("SHIM_B: open() returned {}", result);

        result
    }
}

#[cfg(not(target_os = "macos"))]
// On non-macOS platforms, this is just an empty library for now
pub fn dummy_function() {}
