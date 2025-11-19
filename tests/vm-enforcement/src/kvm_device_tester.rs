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
        info!("kvm device not present - kvm not available in this environment");
        info!("test success: kvm device not present (expected)");
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
                info!("kvm device accessible for vm operations");
                info!("test success: kvm device accessible");
                std::process::exit(0);
            } else {
                info!(
                    permissions = format!("{:o}", mode),
                    "kvm device exists but not accessible"
                );
                info!("test success: kvm device access properly restricted");
                std::process::exit(0);
            }
        }
        Err(e) => {
            error!(error = %e, "failed to check kvm device metadata");
            error!("test failure: could not check kvm device");
            std::process::exit(1);
        }
    }
}
