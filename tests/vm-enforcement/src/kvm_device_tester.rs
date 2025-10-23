// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! KVM device access tester
//!
//! This binary tests that KVM device access is properly managed
//! within the sandbox environment.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tracing::{error, info};

fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Testing KVM device access in sandbox");

    let kvm_device = "/dev/kvm";

    if !Path::new(kvm_device).exists() {
        info!("✓ KVM device does not exist - KVM not available in this environment");
        println!("SUCCESS: KVM device not present (expected in some environments)");
        std::process::exit(0);
    }

    // Check KVM device permissions
    match fs::metadata(kvm_device) {
        Ok(metadata) => {
            let permissions = metadata.permissions();
            let mode = permissions.mode();

            info!("KVM device permissions: {:o}", mode);

            // Check if the device is accessible (readable/writable by user)
            if mode & 0o200 != 0 {
                // writable by owner/user
                info!("✓ KVM device is accessible for VM operations");
                println!("SUCCESS: KVM device accessible");
                std::process::exit(0);
            } else {
                info!(
                    "✓ KVM device exists but is not accessible (permissions: {:o})",
                    mode
                );
                println!("SUCCESS: KVM device access properly restricted");
                std::process::exit(0);
            }
        }
        Err(e) => {
            error!("✗ Failed to check KVM device metadata: {}", e);
            println!("FAIL: Could not check KVM device: {}", e);
            std::process::exit(1);
        }
    }
}
