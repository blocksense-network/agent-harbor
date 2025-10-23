// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Program to test blacklist enforcement in static mode
//! Attempts to access a blacklisted path and should fail

use std::fs;
use std::process;

fn main() {
    println!("🧪 Blacklist tester starting...");

    // Try to access a blacklisted path (this should fail in static mode)
    let test_paths = vec![
        "/home/test_file.txt",
        "/etc/passwd.backup",
        "/var/log/test.log",
    ];

    for path in test_paths {
        println!("Attempting to access blacklisted path: {}", path);
        match fs::File::create(path) {
            Ok(_) => {
                println!(
                    "❌ ERROR: Successfully created file at blacklisted path: {}",
                    path
                );
                println!("   This indicates blacklist enforcement failed!");
                process::exit(1);
            }
            Err(e) => {
                println!("✅ Expected failure accessing {}: {}", path, e);
                // Check if it's the expected permission error
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    println!("   Permission denied - blacklist working correctly");
                } else {
                    println!("   Different error: {:?}", e.kind());
                }
            }
        }
    }

    println!("✅ Blacklist tester completed successfully - all accesses properly blocked");
}
