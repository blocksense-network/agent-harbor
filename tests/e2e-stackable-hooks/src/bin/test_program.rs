// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Simple test program that performs basic system calls for interpose testing

use std::env;
use std::ffi::CString;
use std::io::{self, Write};

#[allow(clippy::print_stdout, clippy::print_stderr, clippy::disallowed_methods)]
fn main() {
    println!("Test program started");

    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "--no-hooks" {
        println!("No hooks enabled");
        perform_basic_operations();
    } else if args.len() > 1 && args[1] == "--with-hooks" {
        println!("Running with hooks enabled");
        perform_basic_operations();
    } else if args.len() > 1 && args[1] == "--with-hooks-priority" {
        println!("Running with priority hooks enabled");
        perform_priority_test_operations();
    } else {
        eprintln!(
            "Usage: {} [--no-hooks|--with-hooks|--with-hooks-priority]",
            args[0]
        );
        std::process::exit(1);
    }

    println!("Test program completed");
}

fn perform_basic_operations() {
    // Perform a simple write operation to stdout
    let message = "Hello from test program\n";
    let c_message = CString::new(message).expect("CString::new failed");

    unsafe {
        libc::write(
            libc::STDOUT_FILENO,
            c_message.as_ptr() as *const libc::c_void,
            message.len(),
        );
    }

    // Perform another write operation
    let second_message = "Second write operation\n";
    let c_second_message = CString::new(second_message).expect("CString::new failed");

    unsafe {
        libc::write(
            libc::STDOUT_FILENO,
            c_second_message.as_ptr() as *const libc::c_void,
            second_message.len(),
        );
    }

    // Flush stdout to ensure output is visible
    io::stdout().flush().expect("Failed to flush stdout");
}

#[allow(clippy::print_stderr, clippy::disallowed_methods)]
fn perform_priority_test_operations() {
    // Perform write operations (hooked by both SHIM_A and SHIM_B)
    let message = "Priority test: write operation\n";
    let c_message = CString::new(message).expect("CString::new failed");

    unsafe {
        libc::write(
            libc::STDOUT_FILENO,
            c_message.as_ptr() as *const libc::c_void,
            message.len(),
        );
    }

    // Perform read operation (hooked only by SHIM_A)
    let mut buffer = [0u8; 10];
    unsafe {
        libc::read(
            libc::STDIN_FILENO,
            buffer.as_mut_ptr() as *mut libc::c_void,
            0, // Read 0 bytes to avoid blocking
        );
    }

    // Perform open operation (hooked only by SHIM_B)
    let path = CString::new("/dev/null").expect("CString::new failed");
    unsafe {
        let fd = libc::open(path.as_ptr(), libc::O_RDONLY, 0);
        if fd >= 0 {
            libc::close(fd);

            // Verify that the real close() was actually called by checking if fd is now invalid
            // dup() should fail with EBADF if the fd is not valid
            let dup_result = libc::dup(fd);
            if dup_result == -1 {
                eprintln!(
                    "VERIFICATION: Real close() was called - fd {} is now invalid (dup failed as expected)",
                    fd
                );
            } else {
                libc::close(dup_result); // Clean up the duplicated fd
                eprintln!(
                    "VERIFICATION: WARNING - dup of fd {} succeeded (real close may not have been called)",
                    fd
                );
            }
        }
    }

    // Flush stdout to ensure output is visible
    io::stdout().flush().expect("Failed to flush stdout");
}
