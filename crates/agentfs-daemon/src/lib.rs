// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS Daemon - Watch service and event distribution

use agentfs_core::{EventKind, EventSink, FsCore};
use agentfs_proto::messages::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Daemon for managing AgentFS watch services and event distribution
pub struct AgentFsDaemon {
    core: Arc<FsCore>,
    watch_service: Arc<WatchService>,
}

/// Watch service for managing file system event watchers
pub struct WatchService {
    /// Registered kqueue watches: (pid, kq_fd, watch_id) -> WatchRegistration
    kqueue_watches: Mutex<HashMap<(u32, u32, u64), KqueueWatchRegistration>>,
    /// Registered FSEvents watches: (pid, registration_id) -> FSEventsWatchRegistration
    fsevents_watches: Mutex<HashMap<(u32, u64), FSEventsWatchRegistration>>,
    /// Next registration ID to assign
    next_registration_id: Mutex<u64>,
}

/// Registration information for a kqueue watch
pub struct KqueueWatchRegistration {
    pub registration_id: u64,
    pub pid: u32,
    pub kq_fd: u32,
    pub watch_id: u64,
    pub fd: u32,
    pub fflags: u32,
    pub doorbell_ident: Option<u64>,
}

/// Registration information for an FSEvents watch
pub struct FSEventsWatchRegistration {
    pub registration_id: u64,
    pub pid: u32,
    pub stream_id: u64,
    pub root_paths: Vec<String>,
    pub flags: u32,
    pub latency: u64,
}

impl WatchService {
    pub fn new() -> Self {
        Self {
            kqueue_watches: Mutex::new(HashMap::new()),
            fsevents_watches: Mutex::new(HashMap::new()),
            next_registration_id: Mutex::new(1),
        }
    }

    /// Register a kqueue watch
    pub fn register_kqueue_watch(
        &self,
        pid: u32,
        kq_fd: u32,
        watch_id: u64,
        fd: u32,
        fflags: u32,
    ) -> u64 {
        let mut next_id = self.next_registration_id.lock().unwrap();
        let registration_id = *next_id;
        *next_id += 1;

        let registration = KqueueWatchRegistration {
            registration_id,
            pid,
            kq_fd,
            watch_id,
            fd,
            fflags,
            doorbell_ident: None,
        };

        self.kqueue_watches.lock().unwrap().insert((pid, kq_fd, watch_id), registration);

        registration_id
    }

    /// Register an FSEvents watch
    pub fn register_fsevents_watch(
        &self,
        pid: u32,
        stream_id: u64,
        root_paths: Vec<String>,
        flags: u32,
        latency: u64,
    ) -> u64 {
        let mut next_id = self.next_registration_id.lock().unwrap();
        let registration_id = *next_id;
        *next_id += 1;

        let registration = FSEventsWatchRegistration {
            registration_id,
            pid,
            stream_id,
            root_paths,
            flags,
            latency,
        };

        self.fsevents_watches
            .lock()
            .unwrap()
            .insert((pid, registration_id), registration);

        registration_id
    }

    /// Unregister a watch by registration ID
    pub fn unregister_watch(&self, pid: u32, registration_id: u64) {
        // Remove from both maps (registration_id is unique across both)
        self.kqueue_watches
            .lock()
            .unwrap()
            .retain(|_, reg| reg.registration_id != registration_id || reg.pid != pid);
        self.fsevents_watches.lock().unwrap().remove(&(pid, registration_id));
    }

    /// Set doorbell identifier for a kqueue
    pub fn set_doorbell(&self, pid: u32, kq_fd: u32, doorbell_ident: u64) {
        let mut watches = self.kqueue_watches.lock().unwrap();
        for (_, registration) in watches.iter_mut() {
            if registration.pid == pid && registration.kq_fd == kq_fd {
                registration.doorbell_ident = Some(doorbell_ident);
            }
        }
    }

    /// Get all kqueue watches for a process
    pub fn get_kqueue_watches_for_pid(&self, pid: u32) -> Vec<KqueueWatchRegistration> {
        self.kqueue_watches
            .lock()
            .unwrap()
            .values()
            .filter(|reg| reg.pid == pid)
            .cloned()
            .collect()
    }

    /// Get all FSEvents watches for a process
    pub fn get_fsevents_watches_for_pid(&self, pid: u32) -> Vec<FSEventsWatchRegistration> {
        self.fsevents_watches
            .lock()
            .unwrap()
            .values()
            .filter(|reg| reg.pid == pid)
            .cloned()
            .collect()
    }
}

impl Clone for KqueueWatchRegistration {
    fn clone(&self) -> Self {
        Self {
            registration_id: self.registration_id,
            pid: self.pid,
            kq_fd: self.kq_fd,
            watch_id: self.watch_id,
            fd: self.fd,
            fflags: self.fflags,
            doorbell_ident: self.doorbell_ident,
        }
    }
}

impl Clone for FSEventsWatchRegistration {
    fn clone(&self) -> Self {
        Self {
            registration_id: self.registration_id,
            pid: self.pid,
            stream_id: self.stream_id,
            root_paths: self.root_paths.clone(),
            flags: self.flags,
            latency: self.latency,
        }
    }
}

impl AgentFsDaemon {
    /// Create a new daemon instance
    pub fn new(core: FsCore) -> Result<Self, Box<dyn std::error::Error>> {
        let core = Arc::new(core);
        let watch_service = Arc::new(WatchService::new());

        Ok(Self {
            core,
            watch_service,
        })
    }

    /// Get reference to the watch service
    pub fn watch_service(&self) -> &Arc<WatchService> {
        &self.watch_service
    }

    /// Subscribe to FsCore events
    pub fn subscribe_events(&self) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: Implement event subscription when FsCore exposes the API
        // For now, this is a placeholder
        Ok(())
    }

    /// Start the daemon event loop (placeholder)
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: Implement the main daemon event loop
        // This would handle incoming connections and process watch registrations
        Ok(())
    }
}

/// Event sink implementation for the daemon
pub struct DaemonEventSink {
    watch_service: Arc<WatchService>,
}

impl DaemonEventSink {
    pub fn new(watch_service: Arc<WatchService>) -> Self {
        Self { watch_service }
    }
}

impl EventSink for DaemonEventSink {
    fn on_event(&self, evt: &EventKind) {
        // TODO: Implement event broadcasting to registered watchers
        // This would:
        // 1. Convert EventKind to appropriate message format
        // 2. Find relevant watchers based on paths/pids
        // 3. Send events to shim via control plane
        tracing::debug!("Received FsCore event: {:?}", evt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watch_service_creation() {
        let service = WatchService::new();
        assert_eq!(service.get_kqueue_watches_for_pid(123).len(), 0);
        assert_eq!(service.get_fsevents_watches_for_pid(123).len(), 0);
    }

    #[test]
    fn test_kqueue_watch_registration() {
        let service = WatchService::new();
        let registration_id = service.register_kqueue_watch(123, 5, 1, 10, 0x123);
        assert_eq!(registration_id, 1);

        let watches = service.get_kqueue_watches_for_pid(123);
        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0].kq_fd, 5);
        assert_eq!(watches[0].fd, 10);
        assert_eq!(watches[0].fflags, 0x123);
    }

    #[test]
    fn test_fsevents_watch_registration() {
        let service = WatchService::new();
        let root_paths = vec!["/tmp/test".to_string()];
        let registration_id =
            service.register_fsevents_watch(456, 2, root_paths.clone(), 0x456, 1000);
        assert_eq!(registration_id, 1);

        let watches = service.get_fsevents_watches_for_pid(456);
        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0].stream_id, 2);
        assert_eq!(watches[0].root_paths, root_paths);
    }

    #[test]
    fn test_watch_unregistration() {
        let service = WatchService::new();
        let reg_id = service.register_kqueue_watch(123, 5, 1, 10, 0x123);
        assert_eq!(service.get_kqueue_watches_for_pid(123).len(), 1);

        service.unregister_watch(123, reg_id);
        assert_eq!(service.get_kqueue_watches_for_pid(123).len(), 0);
    }

    #[test]
    fn test_doorbell_setting() {
        let service = WatchService::new();
        service.register_kqueue_watch(123, 5, 1, 10, 0x123);
        service.set_doorbell(123, 5, 0xABC);

        let watches = service.get_kqueue_watches_for_pid(123);
        assert_eq!(watches[0].doorbell_ident, Some(0xABC));
    }
}
