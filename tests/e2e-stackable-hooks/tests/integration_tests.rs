// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for the stackable-hooks crate.
//!
//! These tests launch a simple test program which loads an interpose shim library.

use std::path::PathBuf;
use std::process::{Command, Stdio};

mod platform;

#[cfg(target_os = "macos")]
use crate::platform::CommandExt;

#[test]
fn test_program_runs_without_hooks() {
    let test_program = get_test_program_path();

    let output = Command::new(&test_program)
        .arg("--no-hooks")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run test program");

    assert!(output.status.success(), "Test program failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Test program started"));
    assert!(stdout.contains("No hooks enabled"));
    assert!(stdout.contains("Test program completed"));
}

#[test]
#[cfg(target_os = "macos")]
fn test_shim_library_a_loading() {
    let test_program = get_test_program_path();
    let shim_library_a = get_shim_library_a_path();

    let output = Command::new(&test_program)
        .args(["--with-hooks-priority"])
        .with_shim_libraries(&[shim_library_a])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run test program with hooks");

    assert!(
        output.status.success(),
        "Test program with hooks failed: {:?}",
        output
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("Test program started"));
    assert!(stderr.contains("SHIM_A: read() intercepted"));
    assert!(stderr.contains("SHIM_A: close() intercepted"));
    assert!(stdout.contains("Test program completed"));
}

/// Test that shim library B can be loaded individually
#[test]
#[cfg(target_os = "macos")]
fn test_shim_library_b_loading() {
    let test_program = get_test_program_path();
    let shim_library_b = get_shim_library_b_path();

    let output = Command::new(&test_program)
        .args(["--with-hooks-priority"])
        .with_shim_libraries(&[shim_library_b])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run test program with hooks");

    assert!(
        output.status.success(),
        "Test program with hooks failed: {:?}",
        output
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stdout.contains("Test program started"));
    assert!(stderr.contains("SHIM_B: open() intercepted"));
    assert!(stderr.contains("SHIM_B: close() intercepted"));
    assert!(stdout.contains("Test program completed"));
}

/// Test that demonstrates the priority system by loading both shim libraries simultaneously
/// and verifying that hooks are called in the correct priority order
#[test]
#[cfg(target_os = "macos")]
fn test_priority_system_demonstration() {
    let test_program = get_test_program_path();
    let shim_library_a = get_shim_library_a_path();
    let shim_library_b = get_shim_library_b_path();

    // Load both libraries simultaneously (order doesn't matter, priority should determine execution order)
    let output = Command::new(&test_program)
        .args(["--with-hooks-priority"])
        .with_shim_libraries(&[shim_library_a, shim_library_b])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run test program with priority hooks");

    assert!(
        output.status.success(),
        "Test program with priority hooks failed: {:?}\nStdout: {}\nStderr: {}",
        output,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(stdout.contains("Test program started"));
    assert!(stdout.contains("Running with priority hooks enabled"));
    assert!(stdout.contains("Test program completed"));

    // Verify that both close hooks are called
    assert!(
        stderr.contains("SHIM_A: close() intercepted"),
        "SHIM_A close hook should be called"
    );
    assert!(
        stderr.contains("SHIM_B: close() intercepted"),
        "SHIM_B close hook should be called"
    );

    // Verify priority order for shared 'close' hook: SHIM_A (priority 5) should be called before SHIM_B (priority 20)
    let shim_a_close_pos = stderr.find("SHIM_A: close() intercepted").unwrap();
    let shim_b_close_pos = stderr.find("SHIM_B: close() intercepted").unwrap();
    assert!(
        shim_a_close_pos < shim_b_close_pos,
        "SHIM_A (priority 5) close() hook should be called before SHIM_B (priority 20) close() hook.\nStderr: {}",
        stderr
    );

    // Verify individual hooks are active
    assert!(stderr.contains("SHIM_A: read() intercepted"));
    assert!(stderr.contains("SHIM_B: open() intercepted"));

    // Verify that the real close() function was actually called
    assert!(
        stderr.contains("VERIFICATION: Real close() was called"),
        "Real close() function should have been called.\nStderr: {}",
        stderr
    );
}

/// Test that call_real! bypasses other hooks and calls the original function directly
#[test]
#[cfg(target_os = "macos")]
fn test_call_real_bypasses_hooks() {
    let test_program = get_test_program_path();
    let shim_library_a = get_shim_library_a_path();
    let shim_library_b = get_shim_library_b_path();

    // Load both libraries - set TEST_CALL_REAL=1 so library A uses call_real! for close
    let output = Command::new(&test_program)
        .args(["--with-hooks-priority"])
        .with_shim_libraries(&[shim_library_a, shim_library_b])
        .env("TEST_CALL_REAL", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run test program with call_real test");

    assert!(
        output.status.success(),
        "Test program with call_real test failed: {:?}\nStdout: {}\nStderr: {}",
        output,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify that SHIM_A's close hook is called (it uses call_real!)
    assert!(
        stderr.contains("SHIM_A: close() intercepted"),
        "SHIM_A close hook should be called"
    );

    // Verify that SHIM_B's close hook is NOT called (bypassed by call_real!)
    assert!(
        !stderr.contains("SHIM_B: close() intercepted"),
        "SHIM_B close hook should be bypassed by call_real!"
    );

    // But SHIM_B's open hook should still work
    assert!(
        stderr.contains("SHIM_B: open() intercepted"),
        "SHIM_B open hook should still work"
    );

    // Most importantly: verify that the real close() function was actually called
    assert!(
        stderr.contains("VERIFICATION: Real close() was called"),
        "Real close() function should have been called even when hooks are bypassed.\nStderr: {}",
        stderr
    );
}

/// Test that call_real! can be used from application code to bypass hooks
/// This test runs a demo binary that loads a hook library and demonstrates
/// that call_real! bypasses hooks while normal calls execute them.
#[test]
#[cfg(target_os = "macos")]
fn test_call_real_from_application_bypasses_hooks() {
    let demo_program = get_call_real_demo_path();
    let shim_library = get_call_real_shim_path();

    // Load the call_real_shim library which blocks reads from stdin
    let output = Command::new(&demo_program)
        .with_shim_libraries(&[shim_library])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run call_real demo");

    assert!(output.status.success(), "Demo program failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify that the normal read call was blocked by the hook
    assert!(
        stdout.contains("Normal read result: -1"),
        "Normal read should be blocked by hook.\nStdout: {}",
        stdout
    );

    // Verify that call_real! bypassed the hook and succeeded
    assert!(
        stdout.contains("call_real! read result: 0"),
        "call_real! should bypass the hook.\nStdout: {}",
        stdout
    );

    // Verify the success message
    assert!(
        stdout.contains("call_real! bypassed hook"),
        "Should show that call_real! bypassed the hook.\nStdout: {}",
        stdout
    );

    // Verify that the hook was actually executed for the normal call
    assert!(
        stderr.contains("SHIM: read() blocked from stdin"),
        "Hook should have executed for normal call.\nStderr: {}",
        stderr
    );
}

/// Get the path to the test program binary
fn get_test_program_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    // Go up to the workspace root (two levels up from tests/e2e-stackable-hooks)
    let mut path = PathBuf::from(manifest_dir);
    path.push("..");
    path.push("..");
    path.push("target");
    path.push(&profile);
    path.push("test-program");

    path
}

/// Get the path to shim library A
#[allow(dead_code)]
fn get_shim_library_a_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    // Go up to the workspace root (two levels up from tests/e2e-stackable-hooks)
    // Then to the shim-a package
    let mut path = PathBuf::from(manifest_dir);
    path.push("..");
    path.push("..");
    path.push("target");
    path.push(&profile);
    #[cfg(target_os = "macos")]
    path.push("libshim_library_a.dylib");
    #[cfg(not(target_os = "macos"))]
    path.push("libshim_library_a.so");

    path
}

/// Get the path to shim library B
#[allow(dead_code)]
fn get_shim_library_b_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    // Go up to the workspace root (two levels up from tests/e2e-stackable-hooks)
    // Then to the shim-b package
    let mut path = PathBuf::from(manifest_dir);
    path.push("..");
    path.push("..");
    path.push("target");
    path.push(&profile);
    #[cfg(target_os = "macos")]
    path.push("libshim_library_b.dylib");
    #[cfg(not(target_os = "macos"))]
    path.push("libshim_library_b.so");

    path
}

/// Get the path to the call_real_demo binary
#[allow(dead_code)]
fn get_call_real_demo_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let mut path = PathBuf::from(manifest_dir);
    // Go up to workspace root
    path.push("..");
    path.push("..");
    path.push("target");
    path.push(&profile);
    path.push("call_real_demo");
    path
}

/// Get the path to the call_real_shim library
#[allow(dead_code)]
fn get_call_real_shim_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    // Go up to the workspace root, then to the shim
    let mut path = PathBuf::from(manifest_dir);
    path.push("..");
    path.push("..");
    path.push("target");
    path.push(&profile);
    #[cfg(target_os = "macos")]
    path.push("libcall_real_shim.dylib");
    #[cfg(not(target_os = "macos"))]
    path.push("libcall_real_shim.so");

    path
}

/// Get the path to the propagation test program
#[allow(dead_code)]
fn get_propagation_test_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let mut path = PathBuf::from(manifest_dir);
    path.push("..");
    path.push("..");
    path.push("target");
    path.push(&profile);
    path.push("propagation-test");
    path
}

/// Get the path to the auto-propagation shim library
#[allow(dead_code)]
fn get_auto_propagation_shim_path() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let mut path = PathBuf::from(manifest_dir);
    path.push("..");
    path.push("..");
    path.push("target");
    path.push(&profile);
    #[cfg(target_os = "macos")]
    path.push("libauto_propagation_shim.dylib");
    #[cfg(not(target_os = "macos"))]
    path.push("libauto_propagation_shim.so");
    path
}

/// Test that auto-propagation works when enabled
#[test]
#[cfg(target_os = "macos")]
fn test_auto_propagation_enabled() {
    let test_program = get_propagation_test_path();
    let shim_library = get_auto_propagation_shim_path();

    let output = Command::new(&test_program)
        .with_shim_libraries(&[shim_library])
        .env("TEST_AUTO_PROPAGATION", "1") // Enable auto-propagation
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run propagation test");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify the shim was loaded in the parent
    assert!(
        stderr.contains("[PROPAGATION-SHIM] Library loaded"),
        "Shim should be loaded in parent.\nStderr: {}",
        stderr
    );

    // Verify auto-propagation was enabled
    assert!(
        stderr.contains("[PROPAGATION-SHIM] Auto-propagation enabled"),
        "Auto-propagation should be enabled.\nStderr: {}",
        stderr
    );

    // Verify the hook was called in the parent
    assert!(
        stderr.contains("[PROPAGATION-SHIM] getpid() hooked"),
        "Hook should be called in parent.\nStderr: {}",
        stderr
    );

    // Verify the shim was loaded in the child (due to auto-propagation)
    // The library loads BEFORE the child main() starts, so we look for a second occurrence
    let load_count = stderr.matches("[PROPAGATION-SHIM] Library loaded").count();
    assert!(
        load_count >= 2,
        "Library should be loaded in both parent and child (expected 2+ loads, found {}).\nStderr: {}",
        load_count,
        stderr
    );

    // Verify the child marker exists
    assert!(
        stderr.contains("[CHILD]"),
        "Could not find child process marker.\nStderr: {}",
        stderr
    );

    assert!(
        output.status.success(),
        "Test should succeed.\nStderr: {}",
        stderr
    );
}

/// Test that auto-propagation doesn't happen when disabled
#[test]
#[cfg(target_os = "macos")]
fn test_auto_propagation_disabled() {
    let test_program = get_propagation_test_path();
    let shim_library = get_auto_propagation_shim_path();

    let output = Command::new(&test_program)
        .with_shim_libraries(&[shim_library])
        // Don't set TEST_AUTO_PROPAGATION, so it remains disabled
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to run propagation test");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Verify the shim was loaded in the parent
    assert!(
        stderr.contains("[PROPAGATION-SHIM] Library loaded"),
        "Shim should be loaded in parent.\nStderr: {}",
        stderr
    );

    // Verify auto-propagation was NOT enabled
    assert!(
        !stderr.contains("[PROPAGATION-SHIM] Auto-propagation enabled"),
        "Auto-propagation should not be enabled.\nStderr: {}",
        stderr
    );

    // Count how many times the library was loaded
    let load_count = stderr.matches("[PROPAGATION-SHIM] Library loaded").count();

    // With auto-propagation disabled, the library should only be loaded once (parent)
    assert_eq!(
        load_count, 1,
        "Library should only be loaded once (in parent) when auto-propagation is disabled.\nStderr: {}",
        stderr
    );

    assert!(
        output.status.success(),
        "Test should succeed.\nStderr: {}",
        stderr
    );
}
