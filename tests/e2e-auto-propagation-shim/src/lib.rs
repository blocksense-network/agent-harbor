// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Test shim for auto-propagation functionality.
//!
//! This shim logs when it's loaded and hooks getpid to verify it's active.

use std::sync::atomic::{AtomicBool, Ordering};

static HOOKS_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[ctor::ctor]
fn init_propagation_test_shim() {
    // Write to stderr to confirm the shim is loaded
    let msg = b"[PROPAGATION-SHIM] Library loaded\n";
    unsafe {
        libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
    }

    // Enable auto-propagation if requested via environment variable
    if std::env::var("TEST_AUTO_PROPAGATION").ok().as_deref() == Some("1") {
        stackable_hooks::enable_auto_propagation();
        let msg = b"[PROPAGATION-SHIM] Auto-propagation enabled\n";
        unsafe {
            libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
        }
    }

    // Enable hooks on macOS
    #[cfg(target_os = "macos")]
    stackable_hooks::enable_hooks();

    HOOKS_INITIALIZED.store(true, Ordering::Release);
}

// Hook getpid to verify the shim is active
stackable_hooks::hook! {
    unsafe fn getpid() -> libc::pid_t => my_getpid {
        let pid = stackable_hooks::call_next!();

        // Only log if hooks are initialized to avoid recursion during init
        if HOOKS_INITIALIZED.load(Ordering::Acquire) {
            // Use a simple log message to verify this hook is called
            let msg = b"[PROPAGATION-SHIM] getpid() hooked\n";
            libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
        }

        pid
    }
}
