// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Process execution and lifecycle management for sandboxing.
//!
//! # Architecture Note: Namespace Creation in Forked Child
//!
//! Linux user namespaces combined with PID namespaces have a restriction:
//! If the calling process is multi-threaded (e.g., running in a tokio runtime),
//! CLONE_NEWPID cannot be specified with CLONE_NEWUSER in the same unshare() call.
//!
//! To work around this, we:
//! 1. Fork first to get a single-threaded child process
//! 2. In the child, call unshare() with all namespace flags
//! 3. Set up UID/GID mappings (the child becomes root inside the namespace)
//! 4. Do mount operations (bind mounts, /proc, overlays, etc.)
//! 5. Execute the target command
//!
//! See: https://man7.org/linux/man-pages/man2/unshare.2.html

use nix::mount::{MsFlags, mount};
use nix::sched::{CloneFlags, unshare};
use nix::sys::wait;
use nix::unistd::{
    ForkResult, Gid, Pid, Uid, close, fork, getgid, getuid, pipe, read, setresgid, setresuid,
    write as nix_write,
};
use std::ffi::CString;
use std::fs::OpenOptions;
use std::io::Write;
use tokio::process::Command as TokioCommand;
use tracing::{debug, error, info, warn};

use crate::Result;
use crate::error::Error;
use crate::namespaces::NamespaceConfig;

/// Configuration for process execution
#[derive(Debug, Clone)]
pub struct ProcessConfig {
    /// Command to execute
    pub command: Vec<String>,
    /// Working directory
    pub working_dir: Option<String>,
    /// Environment variables
    pub env: Vec<(String, String)>,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            command: vec!["/bin/sh".to_string()],
            working_dir: None,
            env: Vec::new(),
        }
    }
}

/// Process manager for executing commands in sandboxed environment
pub struct ProcessManager {
    config: ProcessConfig,
    /// Namespace configuration for sandbox isolation (used in forked child)
    namespace_config: Option<NamespaceConfig>,
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessManager {
    /// Create a new process manager with default configuration
    pub fn new() -> Self {
        Self {
            config: ProcessConfig::default(),
            namespace_config: None,
        }
    }

    /// Create a process manager with custom configuration
    pub fn with_config(config: ProcessConfig) -> Self {
        Self {
            config,
            namespace_config: None,
        }
    }

    /// Set namespace configuration for the sandboxed process
    ///
    /// When set, namespaces will be created in the forked child process,
    /// avoiding the multi-threading restriction of combining CLONE_NEWPID with CLONE_NEWUSER.
    pub fn with_namespace_config(mut self, config: NamespaceConfig) -> Self {
        self.namespace_config = Some(config);
        self
    }

    /// Execute the configured command as PID 1 in the sandbox
    ///
    /// This uses a double-fork pattern to properly enter PID namespaces:
    ///
    /// # Double-Fork Pattern for PID Namespace
    ///
    /// After `unshare(CLONE_NEWPID)`, the calling process is NOT in the new PID namespace -
    /// only its children will be. To execute a command as PID 1, we must fork again after
    /// creating the namespace. This is equivalent to `unshare --fork --pid --mount-proc`.
    ///
    /// # UID/GID Mapping Protocol
    ///
    /// After `unshare(CLONE_NEWUSER)`, the child process cannot write its own `/proc/self/uid_map`.
    /// The parent must write to `/proc/<child_pid>/uid_map` from the parent user namespace.
    ///
    /// # Process Flow
    ///
    /// 1. First fork: Create single-threaded child (escape tokio's multi-threaded runtime)
    /// 2. Child: calls `unshare(CLONE_NEWUSER | CLONE_NEWPID | CLONE_NEWNS | ...)`
    /// 3. Child: signals parent via pipe
    /// 4. Parent: writes uid_map/gid_map to `/proc/<child>/...`
    /// 5. Parent: signals child
    /// 6. Child: second fork to enter PID namespace
    /// 7. Grandchild (PID 1): make root private, mount /proc, exec command
    /// 8. Child: wait for grandchild, exit with grandchild's status
    /// 9. Parent: wait for child
    pub fn exec_as_pid1(&self) -> Result<()> {
        info!(
            "Forking to enter PID namespace and execute as PID 1: {:?}",
            self.config.command
        );

        // Prepare the command and arguments before forking
        if self.config.command.is_empty() {
            return Err(Error::Execution("No command specified".to_string()));
        }

        let program = &self.config.command[0];
        let args: Vec<CString> =
            self.config.command.iter().map(|s| CString::new(s.as_str()).unwrap()).collect();

        // Create pipes for parent-child synchronization
        // child_to_parent: child signals when unshare() is complete
        // parent_to_child: parent signals when uid_map/gid_map are written
        let (child_to_parent_read, child_to_parent_write) = pipe()
            .map_err(|e| Error::Execution(format!("Failed to create child->parent pipe: {}", e)))?;
        let (parent_to_child_read, parent_to_child_write) = pipe()
            .map_err(|e| Error::Execution(format!("Failed to create parent->child pipe: {}", e)))?;

        // Get current UID/GID before forking (needed for mapping)
        let uid = getuid().as_raw();
        let gid = getgid().as_raw();

        // First fork: escape tokio's multi-threaded runtime
        // This is necessary because CLONE_NEWPID + CLONE_NEWUSER cannot be combined
        // in multi-threaded processes.
        match unsafe { nix::unistd::fork() } {
            Ok(nix::unistd::ForkResult::Parent { child }) => {
                // Parent process: close unused pipe ends
                let _ = close(child_to_parent_write);
                let _ = close(parent_to_child_read);

                // Handle namespace setup if configured
                if let Some(ref ns_config) = self.namespace_config {
                    if ns_config.user_ns {
                        // Wait for child to signal that unshare() is complete
                        let mut buf = [0u8; 1];
                        debug!("Parent waiting for child to complete unshare()");
                        if let Err(e) = read(child_to_parent_read, &mut buf) {
                            let _ = close(child_to_parent_read);
                            let _ = close(parent_to_child_write);
                            return Err(Error::Execution(format!(
                                "Failed to read from child sync pipe: {}",
                                e
                            )));
                        }
                        debug!("Parent received signal from child, writing uid_map/gid_map");

                        // Write UID/GID mappings for the child from the parent namespace
                        if let Err(e) = self.write_child_mappings(child, ns_config, uid, gid) {
                            error!("Parent failed to write child mappings: {}", e);
                            // Signal child that we failed (write 'F')
                            let _ = nix_write(parent_to_child_write, b"F");
                            let _ = close(child_to_parent_read);
                            let _ = close(parent_to_child_write);
                            // Still wait for child to avoid zombie
                            let _ = self.wait_for_child(child);
                            return Err(e);
                        }

                        // Signal child that mappings are complete (write 'O' for OK)
                        if let Err(e) = nix_write(parent_to_child_write, b"O") {
                            let _ = close(child_to_parent_read);
                            let _ = close(parent_to_child_write);
                            return Err(Error::Execution(format!(
                                "Failed to signal child that mappings are complete: {}",
                                e
                            )));
                        }
                        debug!("Parent completed uid_map/gid_map setup for child");
                    }
                }

                // Close remaining pipe ends
                let _ = close(child_to_parent_read);
                let _ = close(parent_to_child_write);

                // Wait for child to complete
                info!("Parent process waiting for child PID {}", child);
                self.wait_for_child(child)?;
                Ok(())
            }
            Ok(nix::unistd::ForkResult::Child) => {
                // First child process: close unused pipe ends
                let _ = close(child_to_parent_read);
                let _ = close(parent_to_child_write);

                // Check if we need to set up namespaces
                let has_pid_ns = self.namespace_config.as_ref().map(|c| c.pid_ns).unwrap_or(false);

                // Create namespaces in this child process
                if let Some(ref ns_config) = self.namespace_config {
                    if let Err(e) = self.setup_namespaces_in_child_with_sync(
                        ns_config,
                        child_to_parent_write,
                        parent_to_child_read,
                    ) {
                        error!("Failed to set up namespaces in child: {}", e);
                        std::process::exit(1);
                    }
                }

                // Close sync pipes (no longer needed)
                let _ = close(child_to_parent_write);
                let _ = close(parent_to_child_read);

                // If PID namespace is enabled, we need a second fork.
                // After unshare(CLONE_NEWPID), THIS process is still in the parent PID namespace.
                // Only children forked AFTER unshare will be in the new PID namespace.
                if has_pid_ns {
                    debug!("Second fork to enter PID namespace as PID 1");
                    match unsafe { nix::unistd::fork() } {
                        Ok(nix::unistd::ForkResult::Parent { child: grandchild }) => {
                            // First child: wait for grandchild and propagate exit status
                            debug!(
                                "First child waiting for grandchild PID {} (will be PID 1 in new namespace)",
                                grandchild
                            );
                            match wait::waitpid(grandchild, None) {
                                Ok(wait::WaitStatus::Exited(_, code)) => {
                                    debug!("Grandchild exited with code {}", code);
                                    std::process::exit(code);
                                }
                                Ok(wait::WaitStatus::Signaled(_, signal, _)) => {
                                    debug!("Grandchild terminated by signal {:?}", signal);
                                    // Convert signal to exit code (128 + signal number)
                                    std::process::exit(128 + signal as i32);
                                }
                                Ok(other) => {
                                    warn!("Unexpected grandchild exit status: {:?}", other);
                                    std::process::exit(1);
                                }
                                Err(e) => {
                                    error!("Failed to wait for grandchild: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Ok(nix::unistd::ForkResult::Child) => {
                            // Grandchild: now PID 1 in the new PID namespace!
                            info!("Grandchild is now PID 1 in new PID namespace");

                            // Mount /proc for the new PID namespace
                            // This will now work because we're actually IN the new PID namespace
                            if let Err(e) = self.mount_proc() {
                                warn!("Failed to mount /proc in grandchild (PID 1): {}", e);
                                // Continue execution - some functionality may be limited
                            }

                            // Continue to exec below
                        }
                        Err(e) => {
                            error!("Second fork failed: {}", e);
                            std::process::exit(1);
                        }
                    }
                } else {
                    // No PID namespace, just try to mount /proc (will likely fail without privileges)
                    if let Err(e) = self.mount_proc() {
                        warn!(
                            "Failed to mount /proc (expected without PID namespace): {}",
                            e
                        );
                    }
                }

                // Set working directory if specified
                if let Some(dir) = &self.config.working_dir {
                    if let Err(e) = std::env::set_current_dir(dir) {
                        error!("Failed to set working directory to {}: {}", dir, e);
                        std::process::exit(1);
                    }
                }

                // Set environment variables
                for (key, value) in &self.config.env {
                    std::env::set_var(key, value);
                }

                // Execute the command
                // In the PID namespace case, this is the grandchild (PID 1)
                // Without PID namespace, this is the first child
                // Note: execvp only returns on error (it replaces the process on success)
                match nix::unistd::execvp(&args[0], &args) {
                    Ok(_infallible) => unreachable!("execvp returned Ok"),
                    Err(e) => {
                        error!("Failed to execvp {}: {}", program, e);
                        std::process::exit(1);
                    }
                }
            }
            Err(e) => Err(Error::Execution(format!("Failed to fork: {}", e))),
        }
    }

    /// Write UID/GID mappings for a child process from the parent namespace
    ///
    /// This must be called from the parent process (in the parent user namespace)
    /// after the child has created its user namespace via unshare().
    fn write_child_mappings(
        &self,
        child_pid: Pid,
        config: &NamespaceConfig,
        parent_uid: u32,
        parent_gid: u32,
    ) -> Result<()> {
        let proc_path = format!("/proc/{}", child_pid);

        // First, deny setgroups (required since Linux 3.19 for unprivileged user namespaces)
        let setgroups_path = format!("{}/setgroups", proc_path);
        if let Err(e) = self.write_mapping_file(&setgroups_path, "deny") {
            // Ignore "No such file" for older kernels
            if !format!("{}", e).contains("No such file") {
                warn!("Failed to write setgroups deny: {}", e);
                // Continue anyway - some kernels don't require this
            }
        }

        // Write UID mapping: UID 0 inside maps to parent UID outside
        let uid_map_path = format!("{}/uid_map", proc_path);
        let uid_map = config.uid_map.clone().unwrap_or_else(|| format!("0 {} 1", parent_uid));
        self.write_mapping_file(&uid_map_path, &uid_map)?;

        // Write GID mapping: GID 0 inside maps to parent GID outside
        let gid_map_path = format!("{}/gid_map", proc_path);
        let gid_map = config.gid_map.clone().unwrap_or_else(|| format!("0 {} 1", parent_gid));
        self.write_mapping_file(&gid_map_path, &gid_map)?;

        debug!(
            "Parent wrote uid_map='{}', gid_map='{}' for child {}",
            uid_map, gid_map, child_pid
        );
        Ok(())
    }

    /// Write content to a file (used for /proc/.../uid_map, gid_map, setgroups)
    fn write_mapping_file(&self, path: &str, content: &str) -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|e| Error::Namespace(format!("Failed to open {}: {}", path, e)))?;

        file.write_all(content.as_bytes())
            .map_err(|e| Error::Namespace(format!("Failed to write to {}: {}", path, e)))?;

        debug!("Wrote '{}' to {}", content, path);
        Ok(())
    }

    /// Set up namespaces in the forked child process with parent synchronization
    ///
    /// This is called after forking, so we're in a single-threaded process
    /// and can safely combine CLONE_NEWUSER with CLONE_NEWPID.
    ///
    /// For user namespaces, after calling `unshare(CLONE_NEWUSER)`, the child
    /// signals the parent to write uid_map/gid_map, then waits for confirmation.
    fn setup_namespaces_in_child_with_sync(
        &self,
        config: &NamespaceConfig,
        child_to_parent_write: i32,
        parent_to_child_read: i32,
    ) -> Result<()> {
        info!("Setting up namespaces in forked child process");

        let mut flags = CloneFlags::empty();

        if config.user_ns {
            flags |= CloneFlags::CLONE_NEWUSER;
        }
        if config.mount_ns {
            flags |= CloneFlags::CLONE_NEWNS;
        }
        if config.pid_ns {
            flags |= CloneFlags::CLONE_NEWPID;
        }
        if config.uts_ns {
            flags |= CloneFlags::CLONE_NEWUTS;
        }
        if config.ipc_ns {
            flags |= CloneFlags::CLONE_NEWIPC;
        }

        if !flags.is_empty() {
            unshare(flags).map_err(|e| {
                error!("Failed to unshare namespaces in child: {}", e);
                Error::Namespace(format!("Failed to unshare namespaces: {}", e))
            })?;
            debug!("Successfully created namespaces: {:?}", flags);
        }

        // If user namespace is enabled, synchronize with parent for uid_map/gid_map
        if config.user_ns {
            // Signal parent that unshare() is complete
            debug!("Child signaling parent that unshare() is complete");
            nix_write(child_to_parent_write, b"R")
                .map_err(|e| Error::Namespace(format!("Failed to signal parent: {}", e)))?;

            // Wait for parent to write uid_map/gid_map
            let mut buf = [0u8; 1];
            debug!("Child waiting for parent to complete uid_map/gid_map");
            read(parent_to_child_read, &mut buf)
                .map_err(|e| Error::Namespace(format!("Failed to read parent sync: {}", e)))?;

            if buf[0] != b'O' {
                return Err(Error::Namespace(
                    "Parent failed to set up uid_map/gid_map".to_string(),
                ));
            }

            debug!("Child received confirmation, switching to root in namespace");

            // Now that mappings are set, switch to root inside the namespace
            setresuid(Uid::from_raw(0), Uid::from_raw(0), Uid::from_raw(0))
                .map_err(|e| Error::Namespace(format!("Failed to setresuid to root: {}", e)))?;

            setresgid(Gid::from_raw(0), Gid::from_raw(0), Gid::from_raw(0))
                .map_err(|e| Error::Namespace(format!("Failed to setresgid to root: {}", e)))?;

            debug!("Child is now root in user namespace");
        }

        info!("Namespace setup complete in child process");
        Ok(())
    }

    /// Mount /proc filesystem correctly for PID namespace
    ///
    /// In a new mount namespace, the mount tree inherits the parent's mount propagation.
    /// We need to make the root mount private first, otherwise mount operations will
    /// propagate back to the parent namespace and fail with EPERM.
    fn mount_proc(&self) -> Result<()> {
        info!("Mounting /proc for PID namespace");

        // Make the entire mount tree private to prevent propagation to parent namespace.
        // This is equivalent to: mount --make-rprivate /
        // Without this, mounting in a user namespace fails with EPERM because the kernel
        // refuses to propagate mount events from an unprivileged namespace.
        mount(
            None::<&str>,
            "/",
            None::<&str>,
            MsFlags::MS_REC | MsFlags::MS_PRIVATE,
            None::<&str>,
        )
        .map_err(|e| {
            warn!("Failed to make root mount private: {}", e);
            Error::Execution(format!("Failed to make root mount private: {}", e))
        })?;
        debug!("Made root mount tree private");

        // Unmount any existing /proc mount
        let _ = nix::mount::umount("/proc");

        // Mount new /proc
        mount(
            Some("proc"),
            "/proc",
            Some("proc"),
            MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC | MsFlags::MS_NODEV,
            None::<&str>,
        )
        .map_err(|e| {
            warn!("Failed to mount /proc: {}", e);
            Error::Execution(format!("Failed to mount /proc: {}", e))
        })?;

        debug!("Successfully mounted /proc");
        Ok(())
    }

    /// Wait for child process to complete and handle cleanup
    fn wait_for_child(&self, child_pid: Pid) -> Result<()> {
        info!("Waiting for child process {} to complete", child_pid);

        match wait::waitpid(child_pid, None) {
            Ok(wait::WaitStatus::Exited(pid, code)) => {
                info!("Child process {} exited with code {}", pid, code);
                if code == 0 {
                    Ok(())
                } else {
                    Err(Error::Execution(format!(
                        "Child process exited with code {}",
                        code
                    )))
                }
            }
            Ok(wait::WaitStatus::Signaled(pid, signal, _)) => {
                warn!("Child process {} terminated by signal {:?}", pid, signal);
                Err(Error::Execution(format!(
                    "Child process terminated by signal {:?}",
                    signal
                )))
            }
            Ok(other) => {
                warn!(
                    "Unexpected wait status for child {}: {:?}",
                    child_pid, other
                );
                Err(Error::Execution(format!(
                    "Unexpected child exit status: {:?}",
                    other
                )))
            }
            Err(e) => {
                error!("Failed to wait for child process {}: {}", child_pid, e);
                Err(Error::Execution(format!("Failed to wait for child: {}", e)))
            }
        }
    }

    /// Fork and execute command in child process (for testing)
    pub fn fork_and_exec(&self) -> Result<Pid> {
        match unsafe { fork() } {
            Ok(ForkResult::Parent { child }) => {
                debug!("Forked child process with PID: {}", child);
                Ok(child)
            }
            Ok(ForkResult::Child) => {
                // In child process, execute the command
                if let Err(_e) = self.exec_as_pid1() {
                    // If execution fails, exit with error
                    std::process::exit(1);
                }
                unreachable!();
            }
            Err(e) => Err(Error::Execution(format!("Failed to fork: {}", e))),
        }
    }

    /// Execute command using std::process::Command (for testing without namespace isolation)
    pub async fn exec_command(&self) -> Result<std::process::Stdio> {
        if self.config.command.is_empty() {
            return Err(Error::Execution("No command specified".to_string()));
        }

        let mut cmd = TokioCommand::new(&self.config.command[0]);
        cmd.args(&self.config.command[1..]);

        if let Some(dir) = &self.config.working_dir {
            cmd.current_dir(dir);
        }

        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        let _child = cmd
            .spawn()
            .map_err(|e| Error::Execution(format!("Failed to spawn command: {}", e)))?;

        // For testing purposes, we return the child's stdout
        // In a real implementation, we'd handle the process lifecycle
        Ok(std::process::Stdio::null())
    }

    /// Get the current process configuration
    pub fn config(&self) -> &ProcessConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_manager_creation() {
        let manager = ProcessManager::new();
        assert!(!manager.config().command.is_empty());
    }

    #[test]
    fn test_process_config() {
        let config = ProcessConfig {
            command: vec!["echo".to_string(), "hello".to_string()],
            working_dir: Some("/tmp".to_string()),
            env: vec![("TEST".to_string(), "value".to_string())],
        };
        let manager = ProcessManager::with_config(config.clone());
        assert_eq!(manager.config().command, config.command);
        assert_eq!(manager.config().working_dir, config.working_dir);
    }
}
