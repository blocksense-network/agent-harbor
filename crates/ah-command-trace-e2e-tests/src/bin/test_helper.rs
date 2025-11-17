// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::io::{self, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <command> [args...]", args[0]);
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
            println!("Dummy command executed");
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("Available commands: print_pid, write_stdout, write_stderr, dummy");
            std::process::exit(1);
        }
    }
}

fn test_print_pid(_args: &[String]) {
    println!("PID: {}", std::process::id());
    // Write a single byte to stdout as required by M0
    io::stdout().write_all(&[42]).unwrap();
}

fn test_write_stdout(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: write_stdout <message>");
        std::process::exit(1);
    }

    let message = &args[0];
    println!("{}", message);
}

fn test_write_stderr(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: write_stderr <message>");
        std::process::exit(1);
    }

    let message = &args[0];
    eprintln!("{}", message);
}
