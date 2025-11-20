// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Simple test to verify mount operations work within user namespaces.
//!
//! This test isolates the mount capability issue by creating minimal namespaces
//! and attempting a simple mount operation.
//!
//! Note: This test is Linux-specific as macOS doesn't support user namespaces.

#[cfg(not(target_os = "linux"))]
use tracing::info;
#[cfg(target_os = "linux")]
use tracing::{error, info};

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting simple mount test in user namespace");

    // Test 1: Can we create user namespaces?
    info!("Testing user namespace creation...");
    match create_user_namespace() {
        Ok(pid) => {
            info!("✅ User namespace creation successful, child PID: {}", pid);
            // Wait for child to complete
            let status = nix::sys::wait::waitpid(pid, None)?;
            info!("Child exited with status: {:?}", status);
        }
        Err(e) => {
            error!("❌ User namespace creation failed: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    info!("Mount test skipped - user namespaces not supported on this platform");
    info!("This test is designed for Linux systems with user namespace support");

    Ok(())
}

#[cfg(target_os = "linux")]
fn create_user_namespace() -> anyhow::Result<nix::unistd::Pid> {
    // Fork to create child process that will enter namespaces
    match unsafe { nix::unistd::fork() } {
        Ok(nix::unistd::ForkResult::Parent { child }) => {
            info!("Parent: Created child process {}", child);
            Ok(child)
        }
        Ok(nix::unistd::ForkResult::Child) => {
            info!("Child: Starting namespace creation...");

            // Create user namespace using syscall
            let result = unsafe { libc::syscall(libc::SYS_unshare, libc::CLONE_NEWUSER) };
            if result == 0 {
                info!("Child: User namespace created successfully");
            } else {
                error!(
                    "Child: Failed to create user namespace: {}",
                    std::io::Error::last_os_error()
                );
                std::process::exit(1);
            }

            // Test simple mount operation
            test_simple_mount();

            info!("Child: Exiting");
            std::process::exit(0);
        }
        Err(e) => Err(anyhow::anyhow!("Fork failed: {}", e)),
    }
}

#[cfg(target_os = "linux")]
fn test_simple_mount() {
    info!("Testing simple mount operation within user namespace...");

    // Create mount point directory
    if let Err(e) = std::fs::create_dir_all("/tmp/test_mount") {
        error!("Failed to create mount point directory: {}", e);
        return;
    }

    // Try to mount a simple tmpfs using libc
    let source = std::ffi::CString::new("tmpfs").unwrap();
    let target = std::ffi::CString::new("/tmp/test_mount").unwrap();
    let fstype = std::ffi::CString::new("tmpfs").unwrap();
    let data = std::ffi::CString::new("size=1m").unwrap();

    let flags = libc::MS_NOSUID | libc::MS_NOEXEC | libc::MS_NODEV;

    let result = unsafe {
        libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            fstype.as_ptr(),
            flags as libc::c_ulong,
            data.as_ptr() as *const libc::c_void,
        )
    };

    if result == 0 {
        info!("✅ Simple mount operation succeeded!");
        info!("   This means CAP_SYS_ADMIN IS available in user namespace");

        // Try to unmount it
        let umount_target = std::ffi::CString::new("/tmp/test_mount").unwrap();
        unsafe { libc::umount(umount_target.as_ptr()) };

        // Clean up directory
        let _ = std::fs::remove_dir("/tmp/test_mount");
    } else {
        let err = std::io::Error::last_os_error();
        error!("❌ Simple mount operation failed: {}", err);
        if err.raw_os_error() == Some(libc::EPERM) {
            error!("   This confirms CAP_SYS_ADMIN is not available in user namespace");
        } else {
            error!("   Unexpected error - may be mount point or other issue");
        }

        // Clean up directory
        let _ = std::fs::remove_dir("/tmp/test_mount");
    }
}
