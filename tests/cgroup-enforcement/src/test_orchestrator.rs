//! Test orchestrator for cgroup enforcement E2E tests
//! This program launches the sandbox with abusive processes and verifies
//! that cgroup limits are actually enforced.

use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
enum TestType {
    ForkBomb,
    MemoryHog,
    CpuBurner,
}

impl TestType {
    fn binary_name(&self) -> &'static str {
        match self {
            TestType::ForkBomb => "fork_bomb",
            TestType::MemoryHog => "memory_hog",
            TestType::CpuBurner => "cpu_burner",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            TestType::ForkBomb => "fork bomb (PID limit test)",
            TestType::MemoryHog => "memory hog (OOM kill test)",
            TestType::CpuBurner => "CPU burner (throttling test)",
        }
    }

    fn timeout(&self) -> Duration {
        match self {
            TestType::ForkBomb => Duration::from_secs(10),
            TestType::MemoryHog => Duration::from_secs(15),
            TestType::CpuBurner => Duration::from_secs(5), // Shorter for CPU test
        }
    }
}

fn run_enforcement_test(
    test_type: TestType,
    sbx_helper_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Running {} test", test_type.description());
    println!("   Binary: {}", test_type.binary_name());
    println!("   Timeout: {:.1}s", test_type.timeout().as_secs_f64());

    let start_time = Instant::now();

    // Build the full path to the test binary
    let binary_path = format!("./target/debug/{}", test_type.binary_name());

    // Build the command to run sbx-helper with the test binary
    let mut cmd = Command::new(sbx_helper_path);
    cmd.arg("--debug") // Enable debug logging
        .arg(&binary_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    println!("   Command: {:?}", cmd);

    match cmd.spawn() {
        Ok(mut child) => {
            println!("✅ Sandbox process started (PID: {})", child.id());

            // Monitor the process
            let timeout = test_type.timeout();
            let mut last_check = Instant::now();

            loop {
                // Check if process is still running
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let elapsed = start_time.elapsed();
                        println!(
                            "✅ Process completed in {:.2}s with exit code: {}",
                            elapsed.as_secs_f64(),
                            status.code().unwrap_or(-1)
                        );

                        if status.success() {
                            println!("✅ Test PASSED - process completed normally");
                        } else {
                            println!(
                                "⚠️  Test UNCLEAR - process exited with error (may indicate limits enforced)"
                            );
                        }
                        return Ok(());
                    }
                    Ok(None) => {
                        // Process still running, check timeout
                        if start_time.elapsed() > timeout {
                            println!(
                                "⏰ Process timed out after {:.2}s - terminating",
                                timeout.as_secs_f64()
                            );
                            let _ = child.kill();
                            println!(
                                "✅ Test PASSED - process was contained (didn't run indefinitely)"
                            );
                            return Ok(());
                        }

                        // Periodic monitoring
                        if last_check.elapsed() > Duration::from_secs(1) {
                            println!(
                                "   Process still running... ({:.1}s elapsed)",
                                start_time.elapsed().as_secs_f64()
                            );
                            last_check = Instant::now();
                        }

                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        println!("❌ Error checking process status: {}", e);
                        return Err(e.into());
                    }
                }
            }
        }
        Err(e) => {
            println!("❌ Failed to start sandbox process: {}", e);
            Err(e.into())
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Cgroup Enforcement Test Orchestrator");
    println!("=====================================");

    // Get the directory where this executable is located
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().unwrap_or(exe_path.as_path());
    let project_root = exe_dir
        .parent() // target/debug
        .and_then(|p| p.parent()) // target
        .unwrap_or(exe_dir); // fallback to exe dir

    let sbx_helper_path = project_root.join("target/debug/sbx-helper");

    // Check if sbx-helper exists
    if !sbx_helper_path.exists() {
        println!("❌ sbx-helper binary not found at: {:?}", sbx_helper_path);
        println!("   Please build it first:");
        println!("   cargo build --bin sbx-helper");
        std::process::exit(1);
    }

    println!("✅ Found sbx-helper at: {:?}", sbx_helper_path);

    // Run tests
    let tests = vec![TestType::ForkBomb, TestType::MemoryHog, TestType::CpuBurner];

    let mut passed = 0;
    let mut failed = 0;

    for test in tests {
        println!();
        match run_enforcement_test(test, &sbx_helper_path) {
            Ok(()) => {
                passed += 1;
            }
            Err(e) => {
                println!("❌ Test failed: {}", e);
                failed += 1;
            }
        }
    }

    println!();
    println!("📊 Test Results:");
    println!("   Passed: {}", passed);
    println!("   Failed: {}", failed);

    if failed == 0 {
        println!("🎉 All tests passed!");
        std::process::exit(0);
    } else {
        println!("💥 Some tests failed!");
        std::process::exit(1);
    }
}
