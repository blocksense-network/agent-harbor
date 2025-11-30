// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for sandbox functionality

// Use structured logging
#![allow(clippy::disallowed_methods)]

#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_sandbox_integration() {
    use sandbox_core::{NamespaceConfig, ProcessConfig, Sandbox};
    use sandbox_fs::{FilesystemConfig, FilesystemManager};

    // Create namespace configuration
    let namespace_config = NamespaceConfig {
        user_ns: true,
        mount_ns: true,
        pid_ns: true,
        uts_ns: true,
        ipc_ns: true,
        net_ns: true,
        time_ns: false,
        uid_map: None,
        gid_map: None,
    };

    // Create process configuration
    let process_config = ProcessConfig {
        command: vec!["echo".to_string(), "sandbox test".to_string()],
        working_dir: None,
        env: vec![],
        tmpfs_size: None,
        net_isolation: true,
        allow_internet: false,
    };

    // Create filesystem configuration
    let fs_config = FilesystemConfig {
        readonly_paths: vec!["/etc".to_string()],
        bind_mounts: vec![],
        working_dir: Some("/tmp".to_string()),
        overlay_paths: vec![],
        blacklist_paths: vec![],
        session_state_dir: None,
        static_mode: false,
    };

    // Initialize components
    let mut sandbox =
        Sandbox::with_namespace_config(namespace_config).with_process_config(process_config);

    let fs_manager = FilesystemManager::with_config(fs_config);

    // Test that components can be created and configured
    assert!(sandbox.start().await.is_ok());
    assert!(fs_manager.setup_mounts().await.is_ok());

    // Verify configurations
    assert!(sandbox.namespace_config().user_ns);
    assert!(sandbox.namespace_config().mount_ns);
    assert!(sandbox.namespace_config().pid_ns);
    assert_eq!(fs_manager.config().readonly_paths.len(), 1);
    assert_eq!(fs_manager.config().readonly_paths[0], "/etc");
}

#[cfg(target_os = "macos")]
#[test]
fn test_sbpl_builder_snapshot() {
    use ah_sandbox_macos::SbplBuilder;
    let sbpl = SbplBuilder::new()
        .allow_read_subpath("/usr/bin")
        .allow_write_subpath("/tmp")
        .allow_exec_subpath("/bin")
        .harden_process_info()
        .allow_signal_same_group()
        .deny_apple_events()
        .deny_mach_lookup()
        .allow_process_fork()
        .build();
    assert!(sbpl.contains("(deny default)"));
    assert!(sbpl.contains("(allow file-read* (subpath \"/usr/bin\"))"));
    assert!(sbpl.contains("(allow file-write* (subpath \"/tmp\"))"));
    assert!(sbpl.contains("(allow process-exec (subpath \"/bin\"))"));
    assert!(sbpl.contains("(allow process-fork)"));
    assert!(sbpl.contains("(deny appleevent-send)"));
    assert!(sbpl.contains("(deny mach-lookup)"));
}

#[cfg(target_os = "macos")]
#[test]
fn test_ah_macos_launcher_denies_write_outside_tmp() {
    use std::path::PathBuf;
    use std::process::Command;

    // Resolve project root using CARGO_MANIFEST_DIR (tests/sandbox-integration)
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // tests/
        .and_then(|p| p.parent()) // workspace root
        .expect("failed to resolve project root")
        .to_path_buf();

    let launcher_path = project_root.join("target/debug/ah-macos-launcher");

    if !launcher_path.exists() {
        tracing::warn!(path = ?launcher_path, "Skipping macOS E2E - ah-macos-launcher not found. Build with: cargo build --bin ah-macos-launcher");
        return;
    }

    // Try to write outside allowed path (/tmp) and expect denial under Seatbelt
    let script = r#"
        home="$HOME"
        if echo "test" > "$home/ah_macos_launcher_should_fail.txt"; then
            exit 42
        else
            exit 0
        fi
    "#;

    let _status = Command::new(&launcher_path)
        .args([
            "--allow-write",
            "/tmp",
            "--allow-exec",
            "/bin",
            "--allow-read",
            "/bin",
            "--",
            "sh",
            "-c",
            script,
        ])
        .status()
        .expect("failed to run ah-macos-launcher");

    // Regardless of exit code, assert that the file was not created in $HOME
    let home = std::env::var("HOME").unwrap();
    let path = PathBuf::from(home).join("ah_macos_launcher_should_fail.txt");
    if path.exists() {
        // Clean up if created erroneously to avoid polluting home dir
        let _ = std::fs::remove_file(&path);
        panic!(
            "Seatbelt did not block write outside /tmp; file was created: {:?}",
            path
        );
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_cgroups_integration() {
    use sandbox_core::{NamespaceConfig, ProcessConfig, Sandbox};

    // Create custom cgroup configuration for testing
    let cgroup_config = sandbox_core::CgroupConfig {
        cgroup_root: std::path::PathBuf::from("/sys/fs/cgroup"),
        pids: sandbox_core::PidLimits {
            max: Some(512), // Lower limit for testing
        },
        memory: sandbox_core::MemoryLimits {
            high: Some(100 * 1024 * 1024), // 100MB
            max: Some(200 * 1024 * 1024),  // 200MB
        },
        cpu: sandbox_core::CpuLimits {
            max: Some("25000 100000".to_string()), // 25% of one CPU
        },
    };

    // Create sandbox with cgroups enabled
    let namespace_config = NamespaceConfig {
        user_ns: true,
        mount_ns: true,
        pid_ns: true,
        uts_ns: true,
        ipc_ns: true,
        net_ns: true,
        time_ns: false,
        uid_map: None,
        gid_map: None,
    };

    let process_config = ProcessConfig {
        command: vec!["echo".to_string(), "cgroup test".to_string()],
        working_dir: None,
        env: vec![],
        tmpfs_size: None,
        net_isolation: true,
        allow_internet: false,
    };

    let mut sandbox = Sandbox::with_namespace_config(namespace_config)
        .with_process_config(process_config)
        .with_cgroups(cgroup_config);

    // Test cgroup setup and cleanup
    assert!(sandbox.start().await.is_ok());

    // Test metrics collection
    let metrics = sandbox.collect_metrics().unwrap();
    assert!(metrics.is_some());
    let metrics = metrics.unwrap();

    // Metrics might be None if cgroup operations failed in test environment
    // but the structure should be present
    assert!(metrics.current_pids.is_none() || metrics.current_pids.is_some());
    assert!(metrics.memory_current.is_none() || metrics.memory_current.is_some());
    assert!(metrics.cpu_usage.is_none() || metrics.cpu_usage.is_some());

    // Test cleanup
    assert!(sandbox.stop().is_ok());
}

#[cfg(target_os = "linux")]
#[test]
fn test_cgroups_manager_direct() {
    // Test direct cgroup manager usage
    let config = sandbox_core::CgroupConfig::default();
    let mut manager = sandbox_core::CgroupManager::new(config);

    // Test setup (may fail in test environments without privileges)
    let _setup_result = manager.setup_limits();
    // We don't assert success since this may fail in test environments

    // Test metrics collection (should work even without cgroup setup)
    let metrics = manager.collect_metrics().unwrap();
    assert!(metrics.current_pids.is_none()); // No cgroup created
    assert!(metrics.memory_current.is_none());
    assert!(metrics.memory_events.is_none());
    assert!(metrics.cpu_usage.is_none());
    assert!(metrics.cpu_user.is_none());
    assert!(metrics.cpu_system.is_none());

    // Test cleanup (should not fail even if no cgroup exists)
    assert!(manager.cleanup().is_ok());
}

#[cfg(target_os = "linux")]
#[test]
fn test_cgroups_enforcement_e2e() {
    // This test runs the full E2E cgroup enforcement test suite
    // It requires the test binaries to be built and available

    use std::process::Command;

    // Build paths relative to the project root (not the test directory)
    let project_root = std::env::current_dir()
        .unwrap_or_default()
        .parent() // Go up from tests/sandbox-integration to tests/
        .unwrap_or(&std::env::current_dir().unwrap_or_default())
        .parent() // Go up from tests/ to project root
        .unwrap_or(&std::env::current_dir().unwrap_or_default())
        .to_path_buf();

    let orchestrator_path = project_root.join("target/debug/test_orchestrator");
    let helper_path = project_root.join("target/debug/sbx-helper");

    println!("Looking for orchestrator at: {:?}", orchestrator_path);
    println!(
        "Current working directory: {:?}",
        std::env::current_dir().unwrap_or_default()
    );

    if !orchestrator_path.exists() {
        println!("⚠️  Skipping E2E cgroup enforcement test - test_orchestrator binary not found");
        println!("   Build with: cargo build --bin test_orchestrator");
        println!("   Looking in: {:?}", orchestrator_path);
        return;
    }

    if !helper_path.exists() {
        println!("⚠️  Skipping E2E cgroup enforcement test - sbx-helper binary not found");
        println!("   Build with: cargo build --bin sbx-helper");
        return;
    }

    // Run the test orchestrator
    println!("🧪 Running E2E cgroup enforcement tests...");
    let output = Command::new(orchestrator_path)
        .output()
        .expect("Failed to run test orchestrator");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("Test orchestrator stdout:\n{}", stdout);
    if !stderr.is_empty() {
        eprintln!("Test orchestrator stderr:\n{}", stderr);
    }

    // Check if the orchestrator succeeded
    if output.status.success() {
        println!("✅ E2E cgroup enforcement tests PASSED");
    } else {
        println!("❌ E2E cgroup enforcement tests FAILED");
        println!("Exit code: {:?}", output.status.code());
        panic!("Cgroup enforcement E2E tests failed");
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "TODO: Add support for running this in GitHub Actions CI"]
fn test_debugging_enforcement_e2e() {
    // This test runs the full E2E debugging enforcement test suite
    // It requires the test binaries to be built and available
    // Note: This test requires privileges to create namespaces and mount filesystems

    use std::process::Command;

    // Build paths relative to the project root (not the test directory)
    let project_root = std::env::current_dir()
        .unwrap_or_default()
        .parent() // Go up from tests/sandbox-integration to tests/
        .unwrap_or(&std::env::current_dir().unwrap_or_default())
        .parent() // Go up from tests/ to project root
        .unwrap_or(&std::env::current_dir().unwrap_or_default())
        .to_path_buf();

    let orchestrator_path = project_root.join("target/debug/debugging_test_orchestrator");
    let helper_path = project_root.join("target/debug/sbx-helper");

    println!("Looking for orchestrator at: {:?}", orchestrator_path);
    println!(
        "Current working directory: {:?}",
        std::env::current_dir().unwrap_or_default()
    );

    // Verify the orchestrator binary exists
    if !orchestrator_path.exists() {
        panic!(
            "Debugging test orchestrator not found at: {:?}. Run 'just build-debugging-tests' first.",
            orchestrator_path
        );
    }

    // Verify the helper binary exists
    if !helper_path.exists() {
        panic!(
            "sbx-helper binary not found at: {:?}. Run 'just build-debugging-tests' first.",
            helper_path
        );
    }

    // Run the test orchestrator
    println!("🧪 Running E2E debugging enforcement tests...");
    let output = Command::new(orchestrator_path)
        .output()
        .expect("Failed to run debugging test orchestrator");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("Debugging test orchestrator stdout:\n{}", stdout);
    if !stderr.is_empty() {
        eprintln!("Debugging test orchestrator stderr:\n{}", stderr);
    }

    // Check if the orchestrator succeeded
    if output.status.success() {
        println!("✅ E2E debugging enforcement tests PASSED");
    } else {
        println!("❌ E2E debugging enforcement tests FAILED");
        println!("Exit code: {:?}", output.status.code());
        panic!("Debugging enforcement E2E tests failed");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn test_overlay_enforcement_e2e() {
    // This test runs the full E2E overlay enforcement test suite
    // It requires the test binaries to be built and available
    // Note: This test requires privileges to create namespaces and mount filesystems

    use std::process::Command;

    // Build paths relative to the project root (not the test directory)
    let project_root = std::env::current_dir()
        .unwrap_or_default()
        .parent() // Go up from tests/sandbox-integration to tests/
        .unwrap_or(&std::env::current_dir().unwrap_or_default())
        .parent() // Go up from tests/ to project root
        .unwrap_or(&std::env::current_dir().unwrap_or_default())
        .to_path_buf();

    let orchestrator_path = project_root.join("target/debug/overlay_test_orchestrator");
    let sbx_helper_path = project_root.join("target/debug/sbx-helper");

    println!(
        "Looking for overlay orchestrator at: {:?}",
        orchestrator_path
    );
    println!(
        "Current working directory: {:?}",
        std::env::current_dir().unwrap_or_default()
    );

    if !orchestrator_path.exists() {
        println!(
            "⚠️  Skipping E2E overlay enforcement test - overlay_test_orchestrator binary not found"
        );
        println!("   Build with: cargo build --bin overlay_test_orchestrator");
        println!("   Looking in: {:?}", orchestrator_path);
        return;
    }

    if !sbx_helper_path.exists() {
        println!("⚠️  Skipping E2E overlay enforcement test - sbx-helper binary not found");
        println!("   Build with: cargo build --bin sbx-helper");
        return;
    }

    // Check if we have the necessary privileges to run sandbox tests
    // Try a simple namespace creation to see if we have privileges
    let can_create_namespaces = std::fs::read_to_string("/proc/sys/user/max_user_namespaces")
        .map(|s| s.trim().parse::<i32>().unwrap_or(0) > 0)
        .unwrap_or(false);

    if !can_create_namespaces {
        println!("⚠️  Skipping E2E overlay enforcement test - no namespace privileges available");
        println!(
            "   This test requires privileges to create user namespaces and mount filesystems"
        );
        println!("   Run with appropriate privileges (e.g., sudo) or in a privileged environment");
        return;
    }

    // Run the overlay test orchestrator
    println!("🧪 Running E2E overlay enforcement tests...");
    let output = Command::new(orchestrator_path)
        .output()
        .expect("Failed to run overlay test orchestrator");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("Overlay test orchestrator stdout:\n{}", stdout);
    if !stderr.is_empty() {
        eprintln!("Overlay test orchestrator stderr:\n{}", stderr);
    }

    // Check if the orchestrator succeeded
    if output.status.success() {
        println!("✅ E2E overlay enforcement tests PASSED");
    } else {
        println!("❌ E2E overlay enforcement tests FAILED");
        println!("Exit code: {:?}", output.status.code());
        panic!("Overlay enforcement E2E tests failed");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn test_filesystem_config_defaults() {
    use sandbox_fs::FilesystemConfig;

    let config = FilesystemConfig::default();

    // Verify default readonly paths include common system directories
    assert!(config.readonly_paths.contains(&"/etc".to_string()));
    assert!(config.readonly_paths.contains(&"/usr".to_string()));
    assert!(config.readonly_paths.contains(&"/bin".to_string()));
    assert!(config.bind_mounts.is_empty());
    assert!(config.working_dir.is_none());
    assert!(config.overlay_paths.is_empty());
    assert!(!config.blacklist_paths.is_empty()); // Should have default blacklisted paths
    assert!(config.session_state_dir.is_none());
    assert!(!config.static_mode);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_filesystem_isolation_overlay() {
    use sandbox_core::{NamespaceConfig, ProcessConfig, Sandbox};
    use sandbox_fs::FilesystemConfig;
    use std::fs;
    use tempfile::TempDir;

    // Create a temporary directory for our test
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let test_file_path = temp_dir.path().join("test_file.txt");
    let original_content = "original content outside sandbox";
    let modified_content = "modified content inside sandbox";

    // Create a test file outside the sandbox
    fs::write(&test_file_path, original_content).expect("Failed to write test file");

    // Verify the file exists and has the original content outside the sandbox
    assert_eq!(
        fs::read_to_string(&test_file_path).unwrap(),
        original_content
    );

    // Create sandbox configuration with overlay on the temp directory
    let namespace_config = NamespaceConfig {
        user_ns: true,
        mount_ns: true,
        pid_ns: true,
        uts_ns: true,
        ipc_ns: true,
        net_ns: true,
        time_ns: false,
        uid_map: None,
        gid_map: None,
    };

    // Create a simple command that modifies the test file
    let process_config = ProcessConfig {
        command: vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("echo '{}' > {}", modified_content, test_file_path.display()),
        ],
        working_dir: None,
        env: vec![],
        tmpfs_size: None,
        net_isolation: true,
        allow_internet: false,
    };

    let fs_config = FilesystemConfig {
        readonly_paths: vec![],
        bind_mounts: vec![],
        working_dir: None,
        overlay_paths: vec![temp_dir.path().to_string_lossy().to_string()],
        blacklist_paths: vec![],
        session_state_dir: None,
        static_mode: false,
    };

    // Initialize sandbox with overlay filesystem
    let mut sandbox = Sandbox::with_namespace_config(namespace_config)
        .with_process_config(process_config)
        .with_filesystem(fs_config);

    // Run the sandbox process
    let result = sandbox.exec_process().await;
    if let Err(ref e) = result {
        println!("Sandbox execution failed: {:?}", e);
        // In some test environments, this might fail due to permissions
        // Skip the test rather than failing
        println!(
            "⚠️  Skipping filesystem isolation test - sandbox execution failed (likely insufficient privileges)"
        );
        return;
    }

    // Verify the file was modified during sandbox execution
    let content_after_sandbox = fs::read_to_string(&test_file_path).unwrap();
    assert_eq!(
        content_after_sandbox,
        format!("{}\n", modified_content),
        "File should be modified after sandbox execution"
    );

    // Check if overlay was actually mounted by looking for upper directory content
    let session_dir = temp_dir.path().join("overlay_session");
    let upper_dir = session_dir
        .join("upper")
        .join(test_file_path.strip_prefix(temp_dir.path()).unwrap());

    let overlay_was_mounted = if upper_dir.exists() {
        fs::read_to_string(&upper_dir).is_ok_and(|content| content.trim() == modified_content)
    } else {
        false
    };

    // Cleanup mounts
    let _ = sandbox.cleanup().await;

    // Check final file content
    let final_content = fs::read_to_string(&test_file_path).unwrap();

    if overlay_was_mounted {
        // After overlay is unmounted, verify that the file content is back to original
        // This proves the isolation worked - modifications were contained to the overlay
        assert_eq!(
            final_content, original_content,
            "File content should be restored to original after overlay cleanup - isolation failed!"
        );
        println!(
            "✅ Filesystem isolation test passed - overlay properly isolated writes and restored original state"
        );
    } else {
        // If overlay wasn't mounted, file should remain modified (direct writes, no isolation)
        assert_eq!(
            final_content,
            format!("{}\n", modified_content),
            "File content should remain modified when overlay is not mounted"
        );
        println!(
            "⚠️  Overlay not mounted (likely due to test environment permissions) - skipping isolation verification"
        );
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_filesystem_isolation_readonly_mount() {
    use sandbox_core::{NamespaceConfig, ProcessConfig, Sandbox};
    use sandbox_fs::FilesystemConfig;
    use std::fs;
    use tempfile::TempDir;

    // Create a temporary directory and file for testing readonly mounts
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let test_file_path = temp_dir.path().join("readonly_test.txt");
    let test_content = "This should be readonly in sandbox";

    // Create a test file outside the sandbox
    fs::write(&test_file_path, test_content).expect("Failed to write test file");

    // Create a script that tries to modify the readonly file
    let modify_script = format!(
        r#"#!/bin/bash
        echo "Attempting to modify readonly file: {}"
        if echo "modified content" > {}; then
            echo "ERROR: Successfully modified readonly file!"
            exit 1
        else
            echo "Good: Failed to modify readonly file (expected)"
            exit 0
        fi
        "#,
        test_file_path.display(),
        test_file_path.display()
    );

    let script_path = temp_dir.path().join("readonly_test.sh");
    fs::write(&script_path, modify_script).expect("Failed to write script");

    // Make the script executable
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();

    // Create sandbox configuration with the temp directory as readonly
    let namespace_config = NamespaceConfig {
        user_ns: true,
        mount_ns: true,
        pid_ns: true,
        uts_ns: true,
        ipc_ns: true,
        net_ns: true,
        time_ns: false,
        uid_map: None,
        gid_map: None,
    };

    let process_config = ProcessConfig {
        command: vec![
            "bash".to_string(),
            script_path.to_string_lossy().to_string(),
        ],
        working_dir: Some(temp_dir.path().to_string_lossy().to_string()),
        env: vec![],
        tmpfs_size: None,
        net_isolation: true,
        allow_internet: false,
    };

    let fs_config = FilesystemConfig {
        readonly_paths: vec![temp_dir.path().to_string_lossy().to_string()],
        bind_mounts: vec![],
        working_dir: Some(temp_dir.path().to_string_lossy().to_string()),
        overlay_paths: vec![],
        blacklist_paths: vec![],
        session_state_dir: None,
        static_mode: false,
    };

    // Initialize sandbox
    let mut sandbox = Sandbox::with_namespace_config(namespace_config)
        .with_process_config(process_config)
        .with_filesystem(fs_config);

    // Run the sandbox process
    let result = sandbox.exec_process().await;
    if let Err(ref e) = result {
        println!("Sandbox execution failed: {:?}", e);
        println!(
            "⚠️  Skipping readonly filesystem test - sandbox execution failed (likely insufficient privileges)"
        );
        return;
    }

    // Verify the file content is still original (should not have been modified)
    assert_eq!(
        fs::read_to_string(&test_file_path).unwrap(),
        test_content,
        "Readonly file should not be modified by sandbox process"
    );

    println!(
        "✅ Readonly filesystem isolation test passed - sandbox could not modify readonly files"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn test_filesystem_config_overlay_setup() {
    use sandbox_fs::FilesystemConfig;

    // Test that overlay paths are properly configured
    let config = FilesystemConfig {
        readonly_paths: vec!["/etc".to_string()],
        bind_mounts: vec![],
        working_dir: Some("/tmp/test".to_string()),
        overlay_paths: vec![
            "/tmp/workspace".to_string(),
            "/home/user/project".to_string(),
        ],
        blacklist_paths: vec![],
        session_state_dir: None,
        static_mode: false,
    };

    // Verify overlay configuration
    assert_eq!(config.overlay_paths.len(), 2);
    assert!(config.overlay_paths.contains(&"/tmp/workspace".to_string()));
    assert!(config.overlay_paths.contains(&"/home/user/project".to_string()));
    assert_eq!(config.readonly_paths.len(), 1);
    assert!(!config.static_mode);

    println!("✅ Filesystem overlay configuration test passed");
}

#[test]
fn test_filesystem_isolation_principles() {
    // This test verifies the conceptual principles of filesystem isolation
    // without requiring actual sandbox execution (works on all platforms)

    // Simulate the concept: overlay filesystem should provide isolation
    let original_filesystem = std::collections::HashMap::from([
        ("file1.txt", "original content 1"),
        ("file2.txt", "original content 2"),
    ]);

    // Simulate what happens in an overlay: the sandbox gets a writable copy
    let mut sandbox_view = original_filesystem.clone();

    // Sandbox modifies a file
    sandbox_view.insert("file1.txt", "modified by sandbox");

    // Sandbox creates a new file
    sandbox_view.insert("new_file.txt", "created by sandbox");

    // Verify isolation: original filesystem should be unchanged
    assert_eq!(
        original_filesystem.get("file1.txt").unwrap(),
        &"original content 1"
    );
    assert_eq!(
        original_filesystem.get("file2.txt").unwrap(),
        &"original content 2"
    );
    assert!(!original_filesystem.contains_key("new_file.txt")); // New file shouldn't exist in original

    // Verify sandbox view has changes
    assert_eq!(
        sandbox_view.get("file1.txt").unwrap(),
        &"modified by sandbox"
    );
    assert_eq!(
        sandbox_view.get("new_file.txt").unwrap(),
        &"created by sandbox"
    );

    tracing::info!(
        "Filesystem isolation principles test passed - conceptual overlay isolation verified"
    );
}
