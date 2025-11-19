// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Program to test blacklist enforcement in static mode
//! Attempts to access a blacklisted path and should fail

use std::fs;
use std::process;
use tracing::{error, info, warn};

fn main() {
    tracing_subscriber::fmt::init();
    info!("Blacklist tester starting");

    // Try to access a blacklisted path (this should fail in static mode)
    let test_paths = vec![
        "/home/test_file.txt",
        "/etc/passwd.backup",
        "/var/log/test.log",
    ];

    for path in test_paths {
        info!(path, "Attempting to access blacklisted path");
        match fs::File::create(path) {
            Ok(_) => {
                error!(
                    path,
                    "Successfully created file at blacklisted path - enforcement failed"
                );
                process::exit(1);
            }
            Err(e) => {
                info!(path, error = %e, "Expected failure accessing path");
                // Check if it's the expected permission error
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    info!(path, "Permission denied - blacklist working correctly");
                } else {
                    warn!(path, kind = ?e.kind(), "Different error kind when accessing path");
                }
            }
        }
    }
    info!("Blacklist tester completed successfully - all accesses properly blocked");
}
