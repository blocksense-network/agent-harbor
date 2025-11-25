// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shim library A for testing priority-based stackable-hooks functionality
//! Priority: 5 (higher priority)

#[cfg(target_os = "macos")]
use stackable_hooks::{enable_hooks, hook};

#[cfg(target_os = "macos")]
#[ctor::ctor]
fn init_hooks() {
    enable_hooks();
}

// Removed write hook to avoid recursion issues

#[cfg(target_os = "macos")]
hook! {
    priority: 5,
    unsafe fn read(fd: libc::c_int, buf: *mut libc::c_void, count: libc::size_t)
        -> libc::ssize_t => my_read {
        // For call_real! direct test: simulate blocking reads from stdin (fd 0) by returning -1
        let test_direct = std::env::var("TEST_CALL_REAL_DIRECT").unwrap_or_else(|_| "0".to_string()) == "1";
        if test_direct && fd == 0 {
            // Print a message to indicate the hook is active (use direct libc write to avoid triggering hooks)
            let msg = "SHIM_A: read() intercepted (blocking fd 0)\n".to_string();
            let c_msg = std::ffi::CString::new(msg).unwrap();
            unsafe {
                libc::write(
                    libc::STDERR_FILENO,
                    c_msg.as_ptr() as *const libc::c_void,
                    c_msg.as_bytes().len(),
                );
            }

            // Return -1 to simulate blocking/error
            return -1;
        }

        // Print a message to indicate the hook is active (use direct libc write to avoid triggering hooks)
        let msg = format!("SHIM_A: read() intercepted (priority 5), fd={}, count={}\n", fd, count);
        let c_msg = std::ffi::CString::new(msg).unwrap();
        unsafe {
            libc::write(
                libc::STDERR_FILENO,
                c_msg.as_ptr() as *const libc::c_void,
                c_msg.as_bytes().len(),
            );
        }

        // Call the next hook in the chain or the real function
        let result = stackable_hooks::call_next!(fd, buf, count);

        // Log the result (use direct libc write to avoid triggering hooks)
        let msg = format!("SHIM_A: read() returned {}\n", result);
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
    priority: 5,
    unsafe fn close(fd: libc::c_int) -> libc::c_int => my_close {
        // Print a message to indicate the hook is active (use direct libc write to avoid triggering hooks)
        let msg = format!("SHIM_A: close() intercepted (priority 5), fd={}\n", fd);
        let c_msg = std::ffi::CString::new(msg).unwrap();
        unsafe {
            libc::write(
                libc::STDERR_FILENO,
                c_msg.as_ptr() as *const libc::c_void,
                c_msg.as_bytes().len(),
            );
        }

        // Check if we should use call_real! for testing
        let use_call_real = std::env::var("TEST_CALL_REAL").is_ok() && std::env::var("TEST_CALL_REAL").unwrap() == "1";

        let result = if use_call_real {
            // For call_real! test: bypass other hooks
            stackable_hooks::call_real!(close, fd)
        } else {
            // For priority test: continue hook chain
            stackable_hooks::call_next!(fd)
        };

        // Log the result (use direct libc write to avoid triggering hooks)
        let msg = format!("SHIM_A: close() returned {}\n", result);
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
