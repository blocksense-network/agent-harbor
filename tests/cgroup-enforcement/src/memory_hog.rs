// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Memory hog program to test memory limit enforcement
//! This program tries to allocate as much memory as possible
//! to trigger cgroup memory limits and OOM kills.
//!
//! SAFETY: This program only performs the memory attack when run inside
//! the sandbox with the SANDBOX_TEST_MODE environment variable set.

use std::alloc::{Layout, alloc};
use std::ptr;
use tracing::{debug, info, warn};

const SANDBOX_TEST_ENV: &str = "SANDBOX_TEST_MODE";

fn main() {
    // Initialize tracing
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt::try_init();
    });

    // Safety check: only run the attack if we're in a sandboxed test environment
    if std::env::var(SANDBOX_TEST_ENV).is_err() {
        warn!("safety: memory_hog should only be run inside the sandbox for testing");
        warn!(
            env = SANDBOX_TEST_ENV,
            "set environment variable to enable the attack"
        );
        warn!("this prevents accidental system memory exhaustion during development");
        std::process::exit(1);
    }

    info!("running in sandbox test mode - proceeding with memory hog attack");
    info!("starting memory hog - attempting to allocate unlimited memory");

    let mut allocations = Vec::new();
    let mut total_allocated = 0u64;
    let mut allocation_size = 1024 * 1024; // Start with 1MB chunks

    loop {
        unsafe {
            let layout = Layout::from_size_align(allocation_size, 8).unwrap();
            let ptr = alloc(layout);

            if ptr.is_null() {
                // Allocation failed - try smaller chunks
                allocation_size /= 2;
                if allocation_size < 1024 {
                    info!("unable to allocate even 1KB - likely at memory limit");
                    break;
                }
                continue;
            }

            // Write to the memory to ensure it's actually allocated
            ptr::write_bytes(ptr, 0xAA, allocation_size);

            allocations.push((ptr, layout));
            total_allocated += allocation_size as u64;

            if allocations.len() % 10 == 0 {
                debug!(
                    chunks = allocations.len(),
                    total_mb = total_allocated / (1024 * 1024),
                    "allocation progress"
                );
            }
        }
    }

    info!(
        total_mb = total_allocated / (1024 * 1024),
        chunks = allocations.len(),
        "memory hog completed"
    );

    // Clean up allocations
    for (ptr, layout) in allocations {
        unsafe {
            std::alloc::dealloc(ptr, layout);
        }
    }

    std::process::exit(0);
}
