// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Helper program for testing stdout/stderr chunk capture.
//!
//! This program writes alternating stdout/stderr chunks of varying sizes
//! using different write functions, including ANSI control codes,
//! partial UTF-8 sequences, and binary data.

use std::ffi::CStr;
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <test_type>", args[0]);
        eprintln!(
            "Available test types: basic_chunks, dup2_test, ansi_codes, utf8_partial, binary_data"
        );
        std::process::exit(1);
    }

    let test_type = &args[1];

    match test_type.as_str() {
        "basic_chunks" => test_basic_chunks(),
        "dup2_test" => test_dup2_redirection(),
        "ansi_codes" => test_ansi_control_codes(),
        "utf8_partial" => test_partial_utf8(),
        "binary_data" => test_binary_data(),
        _ => {
            eprintln!("Unknown test type: {}", test_type);
            std::process::exit(1);
        }
    }
}

fn test_basic_chunks() {
    // Use direct libc::write calls to stdout/stderr to trigger our hooks
    use libc::{STDERR_FILENO, STDOUT_FILENO, write};

    // 1 byte chunks
    unsafe { write(STDOUT_FILENO, b"A\n".as_ptr() as *const _, 2) };
    unsafe { write(STDERR_FILENO, b"B\n".as_ptr() as *const _, 2) };

    // 4 KiB chunks
    let chunk_4k = "X".repeat(4096) + "\n";
    unsafe { write(STDOUT_FILENO, chunk_4k.as_ptr() as *const _, chunk_4k.len()) };
    unsafe { write(STDERR_FILENO, chunk_4k.as_ptr() as *const _, chunk_4k.len()) };

    // 32 KiB chunks (reduced from 128 KiB to avoid excessive output)
    let chunk_32k = "Y".repeat(32768) + "\n";
    unsafe {
        write(
            STDOUT_FILENO,
            chunk_32k.as_ptr() as *const _,
            chunk_32k.len(),
        )
    };
    unsafe {
        write(
            STDERR_FILENO,
            chunk_32k.as_ptr() as *const _,
            chunk_32k.len(),
        )
    };

    // Test using writev (vectorized writes)
    test_writev_chunks();

    // Test using sendmsg
    test_sendmsg_chunks();
}

fn test_writev_chunks() {
    use std::io::IoSlice;

    println!("Testing writev (vectorized) writes");

    let stdout_fd = io::stdout().as_raw_fd();
    let stderr_fd = io::stderr().as_raw_fd();

    let data1 = b"writev_chunk1\n";
    let data2 = b"writev_chunk2\n";

    // Write to stdout using writev
    unsafe {
        let iov = [IoSlice::new(data1), IoSlice::new(data2)];
        libc::writev(
            stdout_fd,
            iov.as_ptr() as *const libc::iovec,
            iov.len() as i32,
        );
    }

    // Write to stderr using writev
    unsafe {
        let iov = [
            IoSlice::new(b"writev_stderr1\n"),
            IoSlice::new(b"writev_stderr2\n"),
        ];
        libc::writev(
            stderr_fd,
            iov.as_ptr() as *const libc::iovec,
            iov.len() as i32,
        );
    }
}

fn test_sendmsg_chunks() {
    println!("Testing sendmsg writes");

    let stdout_fd = io::stdout().as_raw_fd();
    let stderr_fd = io::stderr().as_raw_fd();

    let msg1 = b"sendmsg_chunk1\n";
    let msg2 = b"sendmsg_chunk2\n";

    unsafe {
        let iov_stdout = [
            libc::iovec {
                iov_base: msg1.as_ptr() as *mut libc::c_void,
                iov_len: msg1.len(),
            },
            libc::iovec {
                iov_base: msg2.as_ptr() as *mut libc::c_void,
                iov_len: msg2.len(),
            },
        ];

        let msghdr_stdout = libc::msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: iov_stdout.as_ptr() as *mut libc::iovec,
            msg_iovlen: iov_stdout.len(),
            msg_control: std::ptr::null_mut(),
            msg_controllen: 0,
            msg_flags: 0,
        };

        libc::sendmsg(stdout_fd, &msghdr_stdout, 0);
    }

    unsafe {
        let iov_stderr = [
            libc::iovec {
                iov_base: b"sendmsg_stderr1\n".as_ptr() as *mut libc::c_void,
                iov_len: 14,
            },
            libc::iovec {
                iov_base: b"sendmsg_stderr2\n".as_ptr() as *mut libc::c_void,
                iov_len: 14,
            },
        ];

        let msghdr_stderr = libc::msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: iov_stderr.as_ptr() as *mut libc::iovec,
            msg_iovlen: iov_stderr.len(),
            msg_control: std::ptr::null_mut(),
            msg_controllen: 0,
            msg_flags: 0,
        };

        libc::sendmsg(stderr_fd, &msghdr_stderr, 0);
    }
}

fn test_dup2_redirection() {
    println!("Testing dup2 redirection");

    // Duplicate stdout to FD 7
    unsafe {
        libc::dup2(1, 7); // Duplicate stdout (FD 1) to FD 7
    }

    // Write to original stdout
    println!("Original stdout");

    // Write to duplicated FD 7 (should go to stdout)
    unsafe {
        let msg = b"Duplicated FD 7 stdout\n";
        libc::write(7, msg.as_ptr() as *const libc::c_void, msg.len());
    }

    // Now duplicate stderr to FD 8
    unsafe {
        libc::dup2(2, 8); // Duplicate stderr (FD 2) to FD 8
    }

    // Write to original stderr
    eprintln!("Original stderr");

    // Write to duplicated FD 8 (should go to stderr)
    unsafe {
        let msg = b"Duplicated FD 8 stderr\n";
        libc::write(8, msg.as_ptr() as *const libc::c_void, msg.len());
    }

    // Close the duplicated FDs
    unsafe {
        libc::close(7);
        libc::close(8);
    }

    println!("dup2 redirection test complete");
}

fn test_ansi_control_codes() {
    println!("Testing ANSI control codes and escape sequences");

    // ANSI color codes
    println!("\x1b[31mRed text\x1b[0m");
    eprintln!("\x1b[32mGreen text on stderr\x1b[0m");

    // Cursor movement
    println!("Line with\x1b[5Gcursor jump");
    eprintln!("Another line\x1b[10Gwith jump");

    // Mixed ANSI and regular text
    println!("\x1b[1mBold\x1b[0m normal text");
    eprintln!("\x1b[4mUnderlined\x1b[0m normal text");

    // Partial escape sequences (should be handled gracefully)
    println!("Incomplete escape\x1b[31"); // Missing 'm'
    eprintln!("Another incomplete\x1b[32"); // Missing 'm'
}

fn test_partial_utf8() {
    println!("Testing partial UTF-8 sequences");

    // Valid multi-byte UTF-8 sequences
    println!("Valid UTF-8: 你好世界 🌍");
    eprintln!("Valid UTF-8 stderr: 🚀✨");

    // Partial UTF-8 sequences (split across writes)
    // 3-byte UTF-8 character: € (0xE2 0x82 0xAC)

    // Write first byte to stdout
    unsafe {
        libc::write(1, &0xE2u8 as *const u8 as *const libc::c_void, 1);
    }

    // Write second byte to stdout
    unsafe {
        libc::write(1, &0x82u8 as *const u8 as *const libc::c_void, 1);
    }

    // Write third byte to stdout (completing the € character)
    unsafe {
        libc::write(1, &0xACu8 as *const u8 as *const libc::c_void, 1);
    }

    // Now complete with newline
    println!(""); // This should be a valid UTF-8 line ending with €

    // Similar test for stderr
    eprintln!("Partial UTF-8 on stderr:");

    // 4-byte UTF-8 character: 🚀 (0xF0 0x9F 0x9A 0x80)
    unsafe {
        libc::write(2, &0xF0u8 as *const u8 as *const libc::c_void, 1);
        libc::write(2, &0x9Fu8 as *const u8 as *const libc::c_void, 1);
        libc::write(2, &0x9Au8 as *const u8 as *const libc::c_void, 1);
        libc::write(2, &0x80u8 as *const u8 as *const libc::c_void, 1);
    }

    eprintln!(""); // Complete the line
}

fn test_binary_data() {
    println!("Testing binary data output");

    // Write null bytes
    unsafe {
        let nulls = [0u8; 10];
        libc::write(1, nulls.as_ptr() as *const libc::c_void, nulls.len());
    }
    println!(""); // Newline after nulls

    // Write binary data with various byte values
    unsafe {
        let binary_data = [0x00, 0x01, 0x02, 0xFF, 0xFE, 0xFD, 0x80, 0x7F];
        libc::write(
            1,
            binary_data.as_ptr() as *const libc::c_void,
            binary_data.len(),
        );
    }
    println!(""); // Newline after binary

    // Same for stderr
    eprintln!("Binary data on stderr:");

    unsafe {
        let binary_data = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        libc::write(
            2,
            binary_data.as_ptr() as *const libc::c_void,
            binary_data.len(),
        );
    }

    eprintln!(""); // Complete the line
}
