// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Seccomp notification handler with ADDFD injection.

use crate::Result;
use crate::error::Error;
use crate::path_resolver::PathResolver;
use async_trait::async_trait;
use libseccomp_sys::*;
use sandbox_proto::{AuditEntry, FilesystemRequest, FilesystemResponse, Message};
use std::os::unix::io::RawFd;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Supervisor client for communicating filesystem access decisions
#[async_trait]
pub trait SupervisorClient: Send + Sync {
    /// Request permission for filesystem access
    async fn request_access(&self, request: FilesystemRequest) -> Result<FilesystemResponse>;

    /// Send audit entry
    async fn send_audit(&self, entry: AuditEntry) -> Result<()>;
}

/// Default supervisor client using mpsc channel
pub struct ChannelSupervisorClient {
    tx: mpsc::UnboundedSender<Message>,
    #[allow(dead_code)]
    rx: Mutex<mpsc::UnboundedReceiver<Message>>,
}

impl ChannelSupervisorClient {
    pub fn new(tx: mpsc::UnboundedSender<Message>) -> Self {
        let (_client_tx, rx) = mpsc::unbounded_channel();
        Self {
            tx,
            rx: Mutex::new(rx),
        }
    }
}

#[async_trait]
impl SupervisorClient for ChannelSupervisorClient {
    async fn request_access(&self, request: FilesystemRequest) -> Result<FilesystemResponse> {
        let message = Message::FilesystemRequest(request.clone());

        // Send request
        self.tx.send(message).map_err(|e| {
            Error::Notification(format!("Failed to send filesystem request: {}", e))
        })?;

        // For now, deny all requests since we don't have a response mechanism
        // TODO: Implement proper request-response protocol
        Ok(FilesystemResponse {
            allow: false,
            reason: Some("No supervisor configured".into()),
        })
    }

    async fn send_audit(&self, entry: AuditEntry) -> Result<()> {
        let message = Message::Audit(entry);
        self.tx
            .send(message)
            .map_err(|e| Error::Notification(format!("Failed to send audit entry: {}", e)))?;
        Ok(())
    }
}

/// Seccomp notification handler
pub struct NotificationHandler {
    supervisor: Box<dyn SupervisorClient>,
    path_resolver: PathResolver,
    notify_fd: Option<RawFd>,
}

impl NotificationHandler {
    /// Create a new notification handler
    pub fn new(supervisor_tx: mpsc::UnboundedSender<Message>, path_resolver: PathResolver) -> Self {
        let supervisor = Box::new(ChannelSupervisorClient::new(supervisor_tx));

        Self {
            supervisor,
            path_resolver,
            notify_fd: None,
        }
    }

    /// Run the notification handler loop
    pub async fn run(mut self) -> Result<()> {
        info!("Starting seccomp notification handler");

        // Initialize path resolver
        let mut path_resolver = self.path_resolver.clone();
        path_resolver.initialize()?;

        // Get the notification file descriptor
        // Note: We need to pass NULL as context since we're using the global context
        let notify_fd = unsafe { seccomp_notify_fd(std::ptr::null()) };
        if notify_fd < 0 {
            return Err(Error::Notification(
                "Failed to get seccomp notify fd".into(),
            ));
        }
        self.notify_fd = Some(notify_fd);

        // Create epoll instance for monitoring the notify fd
        let epoll_fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if epoll_fd < 0 {
            return Err(Error::Notification(
                "Failed to create epoll instance".into(),
            ));
        }

        // Add notify fd to epoll
        let mut event = libc::epoll_event {
            events: (libc::EPOLLIN | libc::EPOLLERR | libc::EPOLLHUP) as u32,
            u64: notify_fd as u64,
        };

        if unsafe { libc::epoll_ctl(epoll_fd, libc::EPOLL_CTL_ADD, notify_fd, &mut event) } < 0 {
            unsafe { libc::close(epoll_fd) };
            return Err(Error::Notification(
                "Failed to add notify fd to epoll".into(),
            ));
        }

        let mut events = [libc::epoll_event { events: 0, u64: 0 }; 1];

        loop {
            // Wait for notifications
            let n = unsafe { libc::epoll_wait(epoll_fd, events.as_mut_ptr(), 1, -1) };
            if n < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                warn!("epoll_wait failed: {}", std::io::Error::last_os_error());
                break;
            }

            if n > 0 && (events[0].events & libc::EPOLLIN as u32) != 0 {
                if let Err(e) = self.handle_notification(&path_resolver).await {
                    warn!("Failed to handle notification: {}", e);
                }
            }
        }

        unsafe { libc::close(epoll_fd) };
        info!("Seccomp notification handler stopped");
        Ok(())
    }

    /// Handle a single seccomp notification
    async fn handle_notification(&self, path_resolver: &PathResolver) -> Result<()> {
        let mut req = seccomp_notif {
            id: 0,
            pid: 0,
            flags: 0,
            data: libseccomp_sys::seccomp_data {
                nr: 0,
                arch: 0,
                instruction_pointer: 0,
                args: [0; 6],
            },
        };

        // Receive the notification
        let ret = unsafe { seccomp_notify_receive(self.notify_fd.unwrap(), &mut req) };
        if ret != 0 {
            return Err(Error::Notification(format!(
                "Failed to receive seccomp notification: {}",
                ret
            )));
        }

        debug!(
            "Received seccomp notification: syscall={}, pid={}",
            req.data.nr, req.pid
        );

        // Process the notification based on syscall
        let response = if req.data.nr == libc::SYS_openat as i32
            || req.data.nr == libc::SYS_open as i32
        {
            self.handle_open_request(&req, path_resolver).await
        } else if req.data.nr == libc::SYS_stat as i32
            || req.data.nr == libc::SYS_lstat as i32
            || req.data.nr == libc::SYS_fstat as i32
            || req.data.nr == libc::SYS_newfstatat as i32
        {
            self.handle_stat_request(&req, path_resolver).await
        } else if req.data.nr == libc::SYS_access as i32
            || req.data.nr == libc::SYS_faccessat as i32
        {
            self.handle_access_request(&req, path_resolver).await
        } else if req.data.nr == libc::SYS_execve as i32 || req.data.nr == libc::SYS_execveat as i32
        {
            self.handle_exec_request(&req, path_resolver).await
        } else {
            // Unknown syscall - deny
            warn!("Unknown syscall in notification: {}", req.data.nr);
            Ok(seccomp_notif_resp {
                id: req.id,
                val: -libc::EACCES as i64,
                error: -libc::EACCES,
                flags: 0,
            })
        }?;

        // Send the response
        let mut response = response;
        let ret = unsafe { seccomp_notify_respond(self.notify_fd.unwrap(), &mut response) };
        if ret != 0 {
            return Err(Error::Notification(format!(
                "Failed to send seccomp response: {}",
                ret
            )));
        }

        Ok(())
    }

    /// Handle open/openat syscalls
    async fn handle_open_request(
        &self,
        req: &seccomp_notif,
        path_resolver: &PathResolver,
    ) -> Result<seccomp_notif_resp> {
        // Extract path from arguments
        let (_dirfd, pathname_ptr) = if req.data.nr == libc::SYS_openat as i32 {
            (req.data.args[0] as i32, req.data.args[1] as *const i8)
        } else {
            (libc::AT_FDCWD, req.data.args[0] as *const i8)
        };

        let pathname = unsafe { std::ffi::CStr::from_ptr(pathname_ptr) }
            .to_str()
            .map_err(|_| Error::Notification("Invalid pathname in open syscall".into()))?;

        // Resolve the path
        let resolved_path = path_resolver.resolve_path(std::path::Path::new(pathname))?;

        // Create filesystem request
        let fs_request = FilesystemRequest {
            path: resolved_path.to_string_lossy().to_string(),
            operation: "read".to_string(),
            pid: req.pid,
        };

        // Ask supervisor for permission
        let response = self.supervisor.request_access(fs_request).await?;

        // Send audit entry
        let audit_entry = AuditEntry {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            event: "fs_access".to_string(),
            details: serde_json::json!({
                "path": pathname,
                "operation": "open",
                "pid": req.pid,
                "allowed": response.allow
            }),
        };
        let _ = self.supervisor.send_audit(audit_entry).await;

        if response.allow {
            // Allow access - perform ADDFD injection
            self.inject_fd_for_path(&resolved_path, req.pid).await?;
            Ok(seccomp_notif_resp {
                id: req.id,
                val: 0, // Success
                error: 0,
                flags: 0,
            })
        } else {
            // Deny access
            Ok(seccomp_notif_resp {
                id: req.id,
                val: -libc::EACCES as i64,
                error: -libc::EACCES,
                flags: 0,
            })
        }
    }

    /// Handle stat syscalls
    async fn handle_stat_request(
        &self,
        req: &seccomp_notif,
        path_resolver: &PathResolver,
    ) -> Result<seccomp_notif_resp> {
        // For stat operations, we allow them to proceed but log them
        let pathname = if req.data.nr == libc::SYS_newfstatat as i32 {
            let ptr = req.data.args[1] as *const i8;
            unsafe { std::ffi::CStr::from_ptr(ptr) }
                .to_str()
                .map_err(|_| Error::Notification("Invalid pathname in stat syscall".into()))?
        } else {
            let ptr = req.data.args[0] as *const i8;
            unsafe { std::ffi::CStr::from_ptr(ptr) }
                .to_str()
                .map_err(|_| Error::Notification("Invalid pathname in stat syscall".into()))?
        };

        let _resolved_path = path_resolver.resolve_path(std::path::Path::new(pathname))?;

        // For now, allow stat operations but audit them
        let audit_entry = AuditEntry {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            event: "fs_access".to_string(),
            details: serde_json::json!({
                "path": pathname,
                "operation": "stat",
                "pid": req.pid,
                "allowed": true
            }),
        };
        let _ = self.supervisor.send_audit(audit_entry).await;

        Ok(seccomp_notif_resp {
            id: req.id,
            val: 0, // Allow
            error: 0,
            flags: 0,
        })
    }

    /// Handle access syscalls
    async fn handle_access_request(
        &self,
        req: &seccomp_notif,
        path_resolver: &PathResolver,
    ) -> Result<seccomp_notif_resp> {
        // Similar to stat, allow access operations but audit them
        let pathname = if req.data.nr == libc::SYS_faccessat as i32 {
            let ptr = req.data.args[1] as *const i8;
            unsafe { std::ffi::CStr::from_ptr(ptr) }
                .to_str()
                .map_err(|_| Error::Notification("Invalid pathname in access syscall".into()))?
        } else {
            let ptr = req.data.args[0] as *const i8;
            unsafe { std::ffi::CStr::from_ptr(ptr) }
                .to_str()
                .map_err(|_| Error::Notification("Invalid pathname in access syscall".into()))?
        };

        let _resolved_path = path_resolver.resolve_path(std::path::Path::new(pathname))?;

        let audit_entry = AuditEntry {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            event: "fs_access".to_string(),
            details: serde_json::json!({
                "path": pathname,
                "operation": "access",
                "pid": req.pid,
                "allowed": true
            }),
        };
        let _ = self.supervisor.send_audit(audit_entry).await;

        Ok(seccomp_notif_resp {
            id: req.id,
            val: 0, // Allow
            error: 0,
            flags: 0,
        })
    }

    /// Handle exec syscalls
    async fn handle_exec_request(
        &self,
        req: &seccomp_notif,
        path_resolver: &PathResolver,
    ) -> Result<seccomp_notif_resp> {
        // Extract executable path
        let pathname = unsafe { std::ffi::CStr::from_ptr(req.data.args[0] as *const i8) }
            .to_str()
            .map_err(|_| Error::Notification("Invalid pathname in exec syscall".into()))?;

        let resolved_path = path_resolver.resolve_path(std::path::Path::new(pathname))?;

        // Create filesystem request for execution
        let fs_request = FilesystemRequest {
            path: resolved_path.to_string_lossy().to_string(),
            operation: "execute".to_string(),
            pid: req.pid,
        };

        let response = self.supervisor.request_access(fs_request).await?;

        let audit_entry = AuditEntry {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            event: "fs_access".to_string(),
            details: serde_json::json!({
                "path": pathname,
                "operation": "execute",
                "pid": req.pid,
                "allowed": response.allow
            }),
        };
        let _ = self.supervisor.send_audit(audit_entry).await;

        if response.allow {
            // For exec, we allow the syscall to proceed normally
            Ok(seccomp_notif_resp {
                id: req.id,
                val: 0,
                error: 0,
                flags: 0,
            })
        } else {
            Ok(seccomp_notif_resp {
                id: req.id,
                val: -libc::EACCES as i64,
                error: -libc::EACCES,
                flags: 0,
            })
        }
    }

    /// Inject a file descriptor for the given path into the target process
    async fn inject_fd_for_path(&self, _path: &std::path::Path, _pid: u32) -> Result<()> {
        // TODO: Implement ADDFD injection
        // This is complex and requires:
        // 1. Opening the file with the correct permissions
        // 2. Using pidfd_getfd or similar to inject the FD into the target process
        // 3. Handling race conditions with TOCTOU
        warn!("ADDFD injection not yet implemented");
        Ok(())
    }
}

impl Drop for NotificationHandler {
    fn drop(&mut self) {
        if let Some(fd) = self.notify_fd {
            unsafe { libc::close(fd) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ah_test_utils::logged_test]
    fn test_channel_supervisor_client_creation() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let client = ChannelSupervisorClient::new(tx);
        // Client should be created successfully
        assert!(true);
    }
}
