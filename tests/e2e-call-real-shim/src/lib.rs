// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shim library for testing call_real! functionality

#[cfg(target_os = "macos")]
use stackable_hooks::{enable_hooks, hook};

#[cfg(target_os = "macos")]
#[ctor::ctor]
fn init_hooks() {
    enable_hooks();
}

#[cfg(target_os = "macos")]
hook! {
    priority: 10,
    unsafe fn read(stackable_self, fd: libc::c_int, buf: *mut libc::c_void, count: libc::size_t)
        -> libc::ssize_t => my_read {
        // For call_real! testing: block reads from stdin (fd 0)
        if fd == 0 {
            // Print a message to indicate the hook is active
            let msg = "SHIM: read() blocked from stdin\n".to_string();
            let c_msg = std::ffi::CString::new(msg).unwrap();
            unsafe {
                libc::write(
                    libc::STDERR_FILENO,
                    c_msg.as_ptr() as *const libc::c_void,
                    c_msg.as_bytes().len(),
                );
            }

            // Return -1 to indicate error/blocked
            return -1;
        }

        // For other fds, allow the read
        stackable_hooks::call_next!(stackable_self, read, fd, buf, count)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn dummy_function() {}
