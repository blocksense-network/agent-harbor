//! CPU burner program to test CPU limit enforcement
//! This program performs CPU-intensive computations
//! to trigger cgroup CPU throttling.
//!
//! SAFETY: This program only performs the CPU attack when run inside
//! the sandbox with the SANDBOX_TEST_MODE environment variable set.

use std::time::Instant;

const SANDBOX_TEST_ENV: &str = "SANDBOX_TEST_MODE";

fn main() {
    // Safety check: only run the attack if we're in a sandboxed test environment
    if std::env::var(SANDBOX_TEST_ENV).is_err() {
        println!("❌ Safety: cpu_burner should only be run inside the sandbox for testing.");
        println!(
            "   Set {} environment variable to enable the attack.",
            SANDBOX_TEST_ENV
        );
        println!("   This prevents accidental CPU exhaustion during development.");
        std::process::exit(1);
    }

    println!("✅ Running in sandbox test mode - proceeding with CPU burn attack");
    println!("Starting CPU burner - performing intensive computations...");

    let start_time = Instant::now();
    let mut iterations = 0u64;

    loop {
        // Perform CPU-intensive work
        let mut result = 0u64;
        for i in 0..100000 {
            result = result.wrapping_add(i * i);
            result = result.wrapping_mul(31);
            result = result.wrapping_add(0x123456789ABCDEF);
        }

        iterations += 1;

        // Report progress every 100 iterations
        if iterations % 100 == 0 {
            let elapsed = start_time.elapsed();
            println!(
                "Completed {} iterations in {:.2}s (avg: {:.0} iter/s)",
                iterations,
                elapsed.as_secs_f64(),
                iterations as f64 / elapsed.as_secs_f64()
            );
        }

        // Continue indefinitely to test CPU throttling
        // In a real test, this would be killed by timeout or signal
    }
}
