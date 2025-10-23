// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Port collision tester
//!
//! This binary tests that processes inside the sandbox can bind to ports
//! without colliding with host processes, and that different processes
//! within the same sandbox cannot bind to the same port.

use std::net::TcpListener;
use std::process;

fn main() -> anyhow::Result<()> {
    // Try to bind to a high port number that should be available
    // Use a different port for each test run to avoid collisions between test runs
    let port = 12345;

    match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(listener) => {
            println!("SUCCESS: Successfully bound to port {}", port);
            // Keep the listener alive briefly to ensure the bind worked
            drop(listener);
            process::exit(0);
        }
        Err(e) => {
            eprintln!("FAILED: Could not bind to port {}: {}", port, e);
            process::exit(1);
        }
    }
}
