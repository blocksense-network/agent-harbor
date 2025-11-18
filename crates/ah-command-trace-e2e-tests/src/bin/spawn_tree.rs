// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Helper binary for testing subprocess detection in the command trace shim
//!
//! This binary creates a process tree with various types of subprocess launches
//! to test the shim's ability to detect and report command execution.

use std::ffi::CString;
use std::io::{self, Write};
use std::process::{Command, Stdio};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        let _ = writeln!(io::stderr(), "Usage: {} <command> [args...]", args[0]);
        let _ = writeln!(io::stderr(), "Available commands:");
        let _ = writeln!(
            io::stderr(),
            "  spawn_tree - Create a process tree with children and grandchildren"
        );
        let _ = writeln!(
            io::stderr(),
            "  stress_test <iterations> - Run spawn_tree N times for stress testing"
        );
        std::process::exit(1);
    }

    let command = &args[1];
    let test_args = &args[2..];

    match command.as_str() {
        "spawn_tree" => spawn_tree(test_args),
        "stress_test" => stress_test(test_args),
        _ => {
            let _ = writeln!(io::stderr(), "Unknown command: {}", command);
            std::process::exit(1);
        }
    }
}

/// Create a process tree with various subprocess launch patterns
fn spawn_tree(_args: &[String]) {
    let _ = writeln!(io::stderr(), "[spawn_tree] Starting process tree creation");
    let _ = writeln!(
        io::stderr(),
        "[spawn_tree] Parent PID: {}",
        std::process::id()
    );

    // IMPORTANT: Use PATH lookup for arm64e compatibility (see test_helper.rs for details).
    // The nix dev shell provides arm64-compatible binaries that work with our arm64 shim.
    // DO NOT use hardcoded system paths like /bin/true or /usr/bin/sleep!
    // Child 1: Short-lived process using Command::spawn
    match Command::new("true").spawn() {
        Ok(mut child) => {
            let _ = writeln!(
                io::stderr(),
                "[spawn_tree] Spawned short-lived child (true)"
            );
            let _ = child.wait();
            let _ = writeln!(io::stderr(), "[spawn_tree] Short-lived child exited");
        }
        Err(e) => {
            let _ = writeln!(
                io::stderr(),
                "[spawn_tree] Failed to spawn short-lived child: {}",
                e
            );
        }
    }

    // Child 2: Longer-running process using Command::spawn
    match Command::new("sleep").arg("0.1").spawn() {
        Ok(mut child) => {
            let _ = writeln!(
                io::stderr(),
                "[spawn_tree] Spawned longer child (sleep 0.1)"
            );
            let _ = child.wait();
            let _ = writeln!(io::stderr(), "[spawn_tree] Longer child exited");
        }
        Err(e) => {
            let _ = writeln!(
                io::stderr(),
                "[spawn_tree] Failed to spawn longer child: {}",
                e
            );
        }
    }

    // Test direct libc calls that should be intercepted
    test_direct_libc_calls();

    // Also test nested child processes
    spawn_nested_child();
    let _ = writeln!(io::stderr(), "[spawn_tree] Process tree creation complete");
}

/// Test direct libc calls that should be intercepted by the shim
fn test_direct_libc_calls() {
    let _ = writeln!(io::stderr(), "[spawn_tree] Testing direct libc calls");

    // IMPORTANT: Use PATH lookup for arm64e compatibility (see test_helper.rs).
    // The nix dev shell provides arm64-compatible "echo" binary for libc execvp().
    // DO NOT use hardcoded /bin/echo!
    // Test fork
    match unsafe { libc::fork() } {
        -1 => {
            let _ = writeln!(io::stderr(), "[spawn_tree] Fork failed");
        }
        0 => {
            // Child process - test execve
            let _ = writeln!(
                io::stderr(),
                "[spawn_tree] In child process, testing execve"
            );

            let path = std::ffi::CString::new("echo").unwrap();
            let arg1 = std::ffi::CString::new("echo").unwrap();
            let arg2 = std::ffi::CString::new("test").unwrap();

            let args = [
                arg1.as_ptr(),
                arg2.as_ptr(),
                std::ptr::null::<libc::c_char>(),
            ];
            let env = [std::ptr::null::<libc::c_char>()];

            unsafe {
                libc::execvp(path.as_ptr(), args.as_ptr());
            }

            eprintln!("[spawn_tree] execvp failed");
            unsafe {
                libc::_exit(1);
            }
        }
        pid => {
            // Parent process
            let _ = writeln!(io::stderr(), "[spawn_tree] Forked child {}", pid);
            let mut status: libc::c_int = 0;
            unsafe {
                libc::waitpid(pid, &mut status, 0);
            }
            let _ = writeln!(io::stderr(), "[spawn_tree] Child {} exited", pid);
        }
    }
}

/// Spawn a nested child process that itself spawns a grandchild
fn spawn_nested_child() {
    match unsafe { libc::fork() } {
        -1 => {
            let _ = writeln!(io::stderr(), "[spawn_tree] Fork failed");
        }
        0 => {
            // Child process - become the grandchild spawner
            let _ = writeln!(
                io::stderr(),
                "[spawn_tree] Child process started, PID: {}",
                std::process::id()
            );

            // Execute a simple command that should be detectable
            let args = [
                CString::new("echo").unwrap(),
                CString::new("grandchild").unwrap(),
            ];

            let env: Vec<CString> = std::env::vars()
                .map(|(k, v)| CString::new(format!("{}={}", k, v)).unwrap())
                .collect();

            // Use execve to replace this process
            let path = CString::new("/bin/echo").unwrap();
            let argv: Vec<*const libc::c_char> = args
                .iter()
                .map(|s| s.as_ptr())
                .chain(std::iter::once(std::ptr::null()))
                .collect();
            let envp: Vec<*const libc::c_char> = env
                .iter()
                .map(|s| s.as_ptr())
                .chain(std::iter::once(std::ptr::null()))
                .collect();

            unsafe {
                libc::execve(path.as_ptr(), argv.as_ptr(), envp.as_ptr());
            }

            // If execve fails, exit
            let _ = writeln!(io::stderr(), "[spawn_tree] execve failed");
            std::process::exit(1);
        }
        pid => {
            // Parent process - wait for child
            let _ = writeln!(
                io::stderr(),
                "[spawn_tree] Waiting for child process {}",
                pid
            );
            let mut status: libc::c_int = 0;
            unsafe {
                libc::waitpid(pid, &mut status, 0);
            }
            let _ = writeln!(io::stderr(), "[spawn_tree] Child process {} exited", pid);
        }
    }
}

/// Run stress test by spawning many process trees
fn stress_test(args: &[String]) {
    if args.is_empty() {
        let _ = writeln!(io::stderr(), "Usage: stress_test <iterations>");
        std::process::exit(1);
    }

    let iterations: usize = match args[0].parse() {
        Ok(n) => n,
        Err(_) => {
            let _ = writeln!(io::stderr(), "Invalid number of iterations: {}", args[0]);
            std::process::exit(1);
        }
    };

    let _ = writeln!(
        io::stderr(),
        "[stress_test] Running {} iterations",
        iterations
    );

    for i in 0..iterations {
        if i % 10 == 0 {
            let _ = writeln!(
                io::stderr(),
                "[stress_test] Iteration {}/{}",
                i + 1,
                iterations
            );
        }

        // Re-exec ourselves to create a new process tree
        let current_exe = std::env::current_exe().expect("Failed to get current exe");
        let mut cmd = Command::new(&current_exe);
        cmd.arg("spawn_tree").stdout(Stdio::null()).stderr(Stdio::null());

        match cmd.spawn() {
            Ok(mut child) => {
                let _ = child.wait();
            }
            Err(e) => {
                let _ = writeln!(
                    io::stderr(),
                    "[stress_test] Failed to spawn iteration {}: {}",
                    i,
                    e
                );
                std::process::exit(1);
            }
        }
    }

    let _ = writeln!(io::stderr(), "[stress_test] Stress test complete");
}
