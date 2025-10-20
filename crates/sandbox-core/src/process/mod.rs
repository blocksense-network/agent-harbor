//! Process execution and lifecycle management for sandboxing.

use nix::mount::{MsFlags, mount};
use nix::sys::wait;
use nix::unistd::{ForkResult, Pid, fork};
use std::ffi::CString;
use tokio::process::Command as TokioCommand;
use tracing::{debug, error, info, warn};

use crate::Result;
use crate::error::Error;

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
        }
    }

    /// Create a process manager with custom configuration
    pub fn with_config(config: ProcessConfig) -> Self {
        Self { config }
    }

    /// Execute the configured command as PID 1 in the sandbox
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

        // Fork to enter the new PID namespace
        match unsafe { nix::unistd::fork() } {
            Ok(nix::unistd::ForkResult::Parent { child }) => {
                // Parent process: wait for child and handle cleanup
                info!("Parent process waiting for child PID {}", child);
                self.wait_for_child(child)?;
                Ok(())
            }
            Ok(nix::unistd::ForkResult::Child) => {
                // Child process: Now in new PID namespace as PID 1

                // Mount /proc for the new PID namespace (may fail due to user namespace limitations)
                if let Err(e) = self.mount_proc() {
                    warn!(
                        "Failed to mount /proc in child process (expected in unprivileged user namespaces): {}",
                        e
                    );
                    // Continue execution - some functionality may be limited but sandbox still provides other isolation
                }

                // Set working directory if specified
                if let Some(dir) = &self.config.working_dir {
                    std::env::set_current_dir(dir).map_err(|e| {
                        Error::Execution(format!(
                            "Failed to set working directory to {}: {}",
                            dir, e
                        ))
                    })?;
                }

                // Set environment variables
                for (key, value) in &self.config.env {
                    std::env::set_var(key, value);
                }

                // Execute the command as PID 1
                // This will replace the current child process
                nix::unistd::execvp(&args[0], &args).map_err(|e| {
                    Error::Execution(format!("Failed to execvp {}: {}", program, e))
                })?;

                // This should never be reached
                unreachable!();
            }
            Err(e) => Err(Error::Execution(format!("Failed to fork: {}", e))),
        }
    }

    /// Mount /proc filesystem correctly for PID namespace
    fn mount_proc(&self) -> Result<()> {
        info!("Mounting /proc for PID namespace");

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
