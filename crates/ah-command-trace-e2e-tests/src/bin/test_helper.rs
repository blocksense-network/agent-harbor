// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::PathBuf;

#[allow(clippy::print_stdout, clippy::disallowed_methods)]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        let _ = writeln!(io::stderr(), "Usage: {} <command> [args...]", args[0]);
        std::process::exit(1);
    }

    let command = &args[1];
    let test_args = &args[2..];

    match command.as_str() {
        "print_pid" => test_print_pid(test_args),
        "write_stdout" => test_write_stdout(test_args),
        "write_stderr" => test_write_stderr(test_args),
        "dummy" => {
            // Do nothing, just exit successfully to test interposition loading
            let _ = writeln!(io::stdout(), "Dummy command executed");
        }
        "shell_and_interpreter" => test_shell_and_interpreter(test_args),
        _ => {
            let _ = writeln!(io::stderr(), "Unknown command: {}", command);
            let _ = writeln!(
                io::stderr(),
                "Available commands: print_pid, write_stdout, write_stderr, dummy, shell_and_interpreter"
            );
            std::process::exit(1);
        }
    }
}

fn test_print_pid(_args: &[String]) {
    let _ = writeln!(io::stdout(), "PID: {}", std::process::id());
    // Write a single byte to stdout as required by M0
    io::stdout().write_all(&[42]).unwrap();
}

fn test_write_stdout(args: &[String]) {
    if args.is_empty() {
        let _ = writeln!(io::stderr(), "Usage: write_stdout <message>");
        std::process::exit(1);
    }

    let message = &args[0];
    let _ = writeln!(io::stdout(), "{}", message);
}

fn test_write_stderr(args: &[String]) {
    if args.is_empty() {
        let _ = writeln!(io::stderr(), "Usage: write_stderr <message>");
        std::process::exit(1);
    }

    let message = &args[0];
    let _ = writeln!(io::stderr(), "{}", message);
}

#[allow(clippy::print_stdout, clippy::disallowed_methods)]
fn test_shell_and_interpreter(_args: &[String]) {
    use std::process::Command;

    fn log_step(step: &str) {
        if let Some(path) = std::env::var_os("AH_SHELL_TEST_LOG") {
            let path_buf = PathBuf::from(path);
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path_buf) {
                let _ = writeln!(file, "[pid {}] {}", std::process::id(), step);
            }
        }
    }

    println!("Testing shell and interpreter subprocess execution");
    log_step("start shell_and_interpreter");

    // Test 1: Execute a shell script that launches subprocesses
    // This tests that the shim can capture processes launched by bash/sh
    //
    // IMPORTANT: Use PATH lookup instead of hardcoded nix store paths!
    // On macOS with Apple Silicon (M-series chips), system binaries in /bin and /usr/bin
    // are compiled as arm64e binaries (Apple's "pointer authentication" variant).
    // However, our Rust shim library is compiled as arm64, not arm64e.
    // When DYLD_INSERT_LIBRARIES tries to inject an arm64 dylib into an arm64e process,
    // macOS's dynamic linker (dyld) rejects it with "incompatible architecture" error.
    //
    // The nix dev shell provides arm64-compatible versions of these utilities
    // (sh, bash, python3, etc.) in the PATH. By using PATH lookup instead of
    // hardcoded /bin/sh or /usr/bin/python3, we ensure we get the nix-provided
    // arm64 binaries that are compatible with our shim.
    //
    // DO NOT change this back to hardcoded system paths - it will break on macOS!
    match Command::new("sh")
        .args([
            "-c",
            "
        echo 'Shell launching subprocess...'
        echo 'subprocess launched by shell'
        true
    ",
        ])
        .status()
    {
        Ok(status) if status.success() => {
            let _ = writeln!(io::stdout(), "Shell subprocess execution successful");
        }
        Ok(status) => {
            let _ = writeln!(
                io::stderr(),
                "Shell subprocess execution failed with status: {}",
                status
            );
        }
        Err(e) => {
            let _ = writeln!(io::stderr(), "Failed to execute shell subprocess: {}", e);
        }
    }
    log_step("after first shell script");

    // Test 2: Execute a Python script that launches subprocesses
    // This tests that the shim can capture processes launched by python
    // NOTE: At M1, we may not yet capture subprocess.run() calls from python,
    // but this test establishes the requirement for future milestones
    //
    // IMPORTANT: Use PATH lookup for the same arm64e compatibility reasons
    // as explained in Test 1 above. The nix dev shell provides an arm64
    // python3 binary that works with our arm64 shim library.
    // DO NOT change this back to hardcoded /usr/bin/python3!
    match Command::new("python3")
        .args([
            "-c",
            "
import subprocess
import sys
import os
print('Python launching subprocess...', file=sys.stderr)
# Try multiple approaches to subprocess launching
subprocess.run(['echo', 'subprocess launched by python'])
subprocess.run(['true'])
# Also try os.system for comparison
os.system('echo subprocess launched by python os.system > /dev/null 2>&1')
    ",
        ])
        .status()
    {
        Ok(status) if status.success() => {
            let _ = writeln!(io::stdout(), "Python subprocess execution successful");
        }
        Ok(status) => {
            let _ = writeln!(
                io::stderr(),
                "Python subprocess execution failed with status: {}",
                status
            );
        }
        Err(e) => {
            let _ = writeln!(io::stderr(), "Failed to execute python subprocess: {}", e);
        }
    }
    log_step("after python script");

    // Test 3: Execute a shell script that launches another direct subprocess
    // Note: Pipeline commands may not be captured at M1 as shells optimize them
    //
    // IMPORTANT: Same arm64e compatibility reasoning as above.
    // Use PATH lookup to get nix-provided arm64 binaries instead of /bin/sh.
    // DO NOT change this back to hardcoded system paths!
    match Command::new("sh")
        .args([
            "-c",
            "
        echo 'testing direct subprocess from shell'
        echo 'another direct subprocess'
    ",
        ])
        .status()
    {
        Ok(status) if status.success() => {
            let _ = writeln!(io::stdout(), "Piped shell subprocess execution successful");
        }
        Ok(status) => {
            let _ = writeln!(
                io::stderr(),
                "Piped shell subprocess execution failed with status: {}",
                status
            );
        }
        Err(e) => {
            let _ = writeln!(
                io::stderr(),
                "Failed to execute piped shell subprocess: {}",
                e
            );
        }
    }

    println!("Shell and interpreter subprocess test complete");
    log_step("completed shell_and_interpreter");
}
