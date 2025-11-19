// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shim library B for testing priority-based stackable-interpose functionality
//! Priority: 20 (lower priority than A)

#[cfg(target_os = "macos")]
use stackable_interpose::{enable_hooks, hook};

#[cfg(target_os = "macos")]
#[ctor::ctor]
fn init_hooks() {
    enable_hooks();
}

// Removed write hook to avoid recursion issues

#[cfg(target_os = "macos")]
hook! {
    priority: 20,
    unsafe fn open(stackable_self, path: *const libc::c_char, flags: libc::c_int, mode: libc::mode_t)
        -> libc::c_int => my_open {
        // Print a message to indicate the hook is active (use direct libc write to avoid triggering hooks)
        let msg = format!("SHIM_B: open() intercepted (priority 20), flags={}, mode={}\n", flags, mode);
        let c_msg = std::ffi::CString::new(msg).unwrap();
        unsafe {
            libc::write(
                libc::STDERR_FILENO,
                c_msg.as_ptr() as *const libc::c_void,
                c_msg.as_bytes().len(),
            );
        }

        // Call the next hook in the chain or the real function
        let result = stackable_interpose::call_next!(stackable_self, open, path, flags, mode);

        // Log the result (use direct libc write to avoid triggering hooks)
        let msg = format!("SHIM_B: open() returned {}\n", result);
        let c_msg = std::ffi::CString::new(msg).unwrap();
        unsafe {
            libc::write(
                libc::STDERR_FILENO,
                c_msg.as_ptr() as *const libc::c_void,
                c_msg.as_bytes().len(),
            );
        }

        result
    }
}

#[cfg(target_os = "macos")]
hook! {
    priority: 20,
    unsafe fn close(stackable_self, fd: libc::c_int) -> libc::c_int => my_close {
        // Print a message to indicate the hook is active (use direct libc write to avoid triggering hooks)
        let msg = format!("SHIM_B: close() intercepted (priority 20), fd={}\n", fd);
        let c_msg = std::ffi::CString::new(msg).unwrap();
        unsafe {
            libc::write(
                libc::STDERR_FILENO,
                c_msg.as_ptr() as *const libc::c_void,
                c_msg.as_bytes().len(),
            );
        }

        // Call the next hook in the chain or the real function
        let result = stackable_interpose::call_next!(stackable_self, close, fd);

        // Log the result (use direct libc write to avoid triggering hooks)
        let msg = format!("SHIM_B: close() returned {}\n", result);
        let c_msg = std::ffi::CString::new(msg).unwrap();
        unsafe {
            libc::write(
                libc::STDERR_FILENO,
                c_msg.as_ptr() as *const libc::c_void,
                c_msg.as_bytes().len(),
            );
        }

        result
    }
}

#[cfg(not(target_os = "macos"))]
pub fn dummy_function() {}
