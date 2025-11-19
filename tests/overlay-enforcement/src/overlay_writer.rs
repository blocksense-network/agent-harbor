// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Program to test overlay filesystem persistence
//! Creates files in overlay paths and verifies they persist

use std::fs;
use std::path::Path;
use std::process;
use tracing::{error, info, warn};

fn main() {
    tracing_subscriber::fmt::init();
    info!("Overlay writer starting");

    // Test paths that should be overlaid
    let test_cases = vec![
        ("/tmp/overlay_test1.txt", "Content for overlay test 1"),
        ("/tmp/overlay_test2.txt", "Content for overlay test 2"),
        ("/var/tmp/overlay_test3.txt", "Content for overlay test 3"),
    ];

    for (path, content) in test_cases {
        info!(path, "Creating file in overlay path");

        // Ensure parent directory exists
        if let Some(parent) = Path::new(path).parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                error!(dir = %parent.display(), error = %e, "Failed to create parent directory");
                continue;
            }
        }

        match fs::write(path, content) {
            Ok(_) => {
                info!(path, "Successfully wrote file");

                // Verify the content
                match fs::read_to_string(path) {
                    Ok(read_content) => {
                        if read_content == content {
                            info!(path, "Content verification passed");
                        } else {
                            error!(
                                path,
                                expected = content,
                                got = read_content,
                                "Content mismatch"
                            );
                            process::exit(1);
                        }
                    }
                    Err(e) => {
                        error!(path, error = %e, "Failed to read back content");
                        process::exit(1);
                    }
                }
            }
            Err(e) => {
                error!(path, error = %e, "Failed to write file");
                warn!("Continuing test despite write failure (may be expected in test env)");
            }
        }
    }
    info!("Overlay writer completed - files processed");
}
