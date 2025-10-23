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

const SANDBOX_TEST_ENV: &str = "SANDBOX_TEST_MODE";

fn main() {
    // Safety check: only run the attack if we're in a sandboxed test environment
    if std::env::var(SANDBOX_TEST_ENV).is_err() {
        println!("❌ Safety: memory_hog should only be run inside the sandbox for testing.");
        println!(
            "   Set {} environment variable to enable the attack.",
            SANDBOX_TEST_ENV
        );
        println!("   This prevents accidental system memory exhaustion during development.");
        std::process::exit(1);
    }

    println!("✅ Running in sandbox test mode - proceeding with memory hog attack");
    println!("Starting memory hog - attempting to allocate unlimited memory...");

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
                    println!("Unable to allocate even 1KB - likely at memory limit");
                    break;
                }
                continue;
            }

            // Write to the memory to ensure it's actually allocated
            ptr::write_bytes(ptr, 0xAA, allocation_size);

            allocations.push((ptr, layout));
            total_allocated += allocation_size as u64;

            if allocations.len() % 10 == 0 {
                println!(
                    "Allocated {} chunks, total: {} MB",
                    allocations.len(),
                    total_allocated / (1024 * 1024)
                );
            }
        }
    }

    println!(
        "Memory hog completed. Allocated {} MB in {} chunks.",
        total_allocated / (1024 * 1024),
        allocations.len()
    );

    // Clean up allocations
    for (ptr, layout) in allocations {
        unsafe {
            std::alloc::dealloc(ptr, layout);
        }
    }

    std::process::exit(0);
}
