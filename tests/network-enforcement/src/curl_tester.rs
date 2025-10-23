// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Simple curl tester for network connectivity tests
//!
//! This binary attempts to connect to a given IP address using curl.
//! It's used to test network isolation and internet access within the sandbox.

use std::env;
use std::process::Command;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <ip_address>", args[0]);
        std::process::exit(1);
    }

    let ip_address = &args[1];

    // Try to curl the IP address with a short timeout
    let output = Command::new("curl")
        .args([
            "--connect-timeout",
            "5",
            "--max-time",
            "10",
            "-s", // silent
            "-o",
            "/dev/null", // don't save output
            "-w",
            "%{http_code}", // output HTTP status code
            &format!("http://{}", ip_address),
        ])
        .output()?;

    if output.status.success() {
        let status_code_str = String::from_utf8_lossy(&output.stdout);
        let status_code = status_code_str.trim();
        if status_code == "200" || status_code == "000" || status_code == "301" {
            // 000 means connection succeeded but no HTTP response (common for IP addresses)
            println!("SUCCESS: Connected to {}", ip_address);
            std::process::exit(0);
        } else {
            println!("FAILED: HTTP {} from {}", status_code, ip_address);
            std::process::exit(1);
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!(
            "FAILED: Could not connect to {}: {}",
            ip_address,
            stderr.trim()
        );
        std::process::exit(1);
    }
}
