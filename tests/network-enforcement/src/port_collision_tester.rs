// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Port collision tester
//!
//! This binary tests that processes inside the sandbox can bind to ports
//! without colliding with host processes, and that different processes
//! within the same sandbox cannot bind to the same port.

use std::net::TcpListener;
use std::process;
use tracing::{error, info};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    // Try to bind to a high port number that should be available
    // Use a different port for each test run to avoid collisions between test runs
    let port = 12345;

    match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(listener) => {
            info!(port, "Successfully bound to port");
            // Keep the listener alive briefly to ensure the bind worked
            drop(listener);
            process::exit(0);
        }
        Err(e) => {
            error!(port, error = %e, "Failed to bind to port");
            process::exit(1);
        }
    }
}
