// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::io::{self, Write};

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
            eprintln!("Unknown command: {}", command);
            eprintln!(
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

fn test_shell_and_interpreter(_args: &[String]) {
    use std::process::Command;

    println!("Testing shell and interpreter subprocess execution");

    // Test 1: Execute a shell script that launches subprocesses
    // This tests that the shim can capture processes launched by bash
    match Command::new("/bin/sh")
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
            println!("Shell subprocess execution successful");
        }
        Ok(status) => {
            eprintln!("Shell subprocess execution failed with status: {}", status);
        }
        Err(e) => {
            eprintln!("Failed to execute shell subprocess: {}", e);
        }
    }

    // Test 2: Execute a Python script that launches subprocesses
    // This tests that the shim can capture processes launched by python
    // NOTE: At M1, we may not yet capture subprocess.run() calls from python,
    // but this test establishes the requirement for future milestones
    match Command::new("/usr/bin/env")
        .args([
            "python3",
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
            println!("Python subprocess execution successful");
        }
        Ok(status) => {
            eprintln!("Python subprocess execution failed with status: {}", status);
        }
        Err(e) => {
            eprintln!("Failed to execute python subprocess: {}", e);
        }
    }

    // Test 3: Execute a shell script that launches another direct subprocess
    // Note: Pipeline commands may not be captured at M1 as shells optimize them
    match Command::new("/bin/sh")
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
            println!("Piped shell subprocess execution successful");
        }
        Ok(status) => {
            eprintln!(
                "Piped shell subprocess execution failed with status: {}",
                status
            );
        }
        Err(e) => {
            eprintln!("Failed to execute piped shell subprocess: {}", e);
        }
    }

    println!("Shell and interpreter subprocess test complete");
}
