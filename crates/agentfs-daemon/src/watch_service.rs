// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS Daemon - Watch service and event distribution

use agentfs_core::{EventKind, EventSink, FsCore};
use agentfs_proto::messages::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// kqueue vnode event flags (macOS)
const NOTE_DELETE: u32 = 0x00000001;
const NOTE_WRITE: u32 = 0x00000002;
const NOTE_EXTEND: u32 = 0x00000004;
const NOTE_ATTRIB: u32 = 0x00000008;
const NOTE_LINK: u32 = 0x00000010;
const NOTE_RENAME: u32 = 0x00000020;
const NOTE_REVOKE: u32 = 0x00000040;


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
    pub path: String,
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
        path: String,
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
            path,
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
            path: self.path.clone(),
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


/// Event sink implementation for the watch service daemon
pub struct WatchServiceEventSink {
    watch_service: Arc<WatchService>,
}


impl WatchServiceEventSink {
    pub fn new(watch_service: Arc<WatchService>) -> Self {
        Self { watch_service }
    }
}

impl EventSink for WatchServiceEventSink {
    fn on_event(&self, evt: &EventKind) {
        tracing::debug!("Received FsCore event: {:?}", evt);

        // Find all affected paths for this event
        let affected_paths = self.get_affected_paths(evt);

        // Route to kqueue watchers
        self.route_to_kqueue_watchers(evt, &affected_paths);

        // Route to FSEvents watchers
        self.route_to_fsevents_watchers(evt, &affected_paths);
    }
}

impl WatchServiceEventSink {
    /// Get all paths affected by this event (including parent directories for directory events)
    fn get_affected_paths(&self, evt: &EventKind) -> Vec<String> {
        match evt {
            EventKind::Created { path } | EventKind::Removed { path } | EventKind::Modified { path } => {
                // For file events, also notify parent directory watchers
                let mut paths = vec![path.clone()];
                if let Some(parent) = std::path::Path::new(path).parent() {
                    if let Some(parent_str) = parent.to_str() {
                        paths.push(parent_str.to_string());
                    }
                }
                paths
            }
            EventKind::Renamed { from, to } => {
                // For renames, notify both source and destination paths and their parents
                let mut paths = vec![from.clone(), to.clone()];
                for path in [from, to] {
                    if let Some(parent) = std::path::Path::new(path).parent() {
                        if let Some(parent_str) = parent.to_str() {
                            paths.push(parent_str.to_string());
                        }
                    }
                }
                paths
            }
            EventKind::BranchCreated { .. } | EventKind::SnapshotCreated { .. } => {
                // Branch/snapshot events don't affect filesystem paths
                vec![]
            }
        }
    }

    /// Convert EventKind to kqueue vnode flags for a specific path
    fn event_to_vnode_flags(&self, evt: &EventKind, path: &str, is_directory: bool) -> u32 {
        match evt {
            EventKind::Created { .. } => {
                if is_directory {
                    NOTE_WRITE // Directory contents changed
                } else {
                    NOTE_WRITE // File created
                }
            }
            EventKind::Removed { .. } => {
                if is_directory {
                    NOTE_WRITE // Directory contents changed
                } else {
                    NOTE_DELETE // File deleted
                }
            }
            EventKind::Modified { .. } => {
                if is_directory {
                    NOTE_ATTRIB // Directory metadata changed
                } else {
                    NOTE_WRITE | NOTE_EXTEND // File content/size changed
                }
            }
            EventKind::Renamed { from, to } => {
                // For renames, determine if this path is the source or destination
                if path == from {
                    NOTE_RENAME // File moved away
                } else if path == to {
                    NOTE_RENAME // File moved to
                } else {
                    // Parent directory of source or destination
                    NOTE_WRITE // Directory contents changed
                }
            }
            _ => 0,
        }
    }

    /// Route event to matching kqueue watchers
    fn route_to_kqueue_watchers(&self, evt: &EventKind, affected_paths: &[String]) {
        let watches = self.watch_service.kqueue_watches.lock().unwrap();

        for path in affected_paths {
            // Find all watches for this path
            let matching_watches: Vec<&KqueueWatchRegistration> = watches.values()
                .filter(|reg| &reg.path == path)
                .collect();

            for watch in matching_watches {
                let flags = self.event_to_vnode_flags(evt, path, false); // Assume files for now

                // Only send if the watch is interested in these flags
                if (watch.fflags & flags) != 0 {
                    tracing::debug!("Routing kqueue event: pid={}, fd={}, flags={:#x}",
                                  watch.pid, watch.fd, flags);
                    // TODO: Send FsEventBroadcast message to shim
                    // This would use the control plane to notify the shim
                }
            }
        }
    }

    /// Route event to matching FSEvents watchers
    fn route_to_fsevents_watchers(&self, evt: &EventKind, affected_paths: &[String]) {
        let watches = self.watch_service.fsevents_watches.lock().unwrap();

        // Check each FSEvents stream to see if any affected path is under its root
        for watch in watches.values() {
            let mut should_notify = false;
            for root_path in &watch.root_paths {
                for affected_path in affected_paths {
                    if affected_path.starts_with(root_path) ||
                       std::path::Path::new(affected_path).starts_with(root_path) {
                        should_notify = true;
                        break;
                    }
                }
                if should_notify {
                    break;
                }
            }

            if should_notify {
                tracing::debug!("Routing FSEvents event: pid={}, stream_id={}",
                              watch.pid, watch.stream_id);
                // TODO: Send FsEventBroadcast message to shim
                // This would use the control plane to notify the shim
            }
        }
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
        let registration_id = service.register_kqueue_watch(123, 5, 1, 10, "/tmp/test.txt".to_string(), 0x123);
        assert_eq!(registration_id, 1);

        let watches = service.get_kqueue_watches_for_pid(123);
        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0].kq_fd, 5);
        assert_eq!(watches[0].fd, 10);
        assert_eq!(watches[0].path, "/tmp/test.txt");
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
        let reg_id = service.register_kqueue_watch(123, 5, 1, 10, "/tmp/test.txt".to_string(), 0x123);
        assert_eq!(service.get_kqueue_watches_for_pid(123).len(), 1);

        service.unregister_watch(123, reg_id);
        assert_eq!(service.get_kqueue_watches_for_pid(123).len(), 0);
    }

    #[test]
    fn test_doorbell_setting() {
        let service = WatchService::new();
        service.register_kqueue_watch(123, 5, 1, 10, "/tmp/test.txt".to_string(), 0x123);
        service.set_doorbell(123, 5, 0xABC);

        let watches = service.get_kqueue_watches_for_pid(123);
        assert_eq!(watches[0].doorbell_ident, Some(0xABC));
    }

    #[test]
    fn test_affected_paths_created() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Created { path: "/tmp/dir/file.txt".to_string() };
        let paths = sink.get_affected_paths(&event);

        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&"/tmp/dir/file.txt".to_string()));
        assert!(paths.contains(&"/tmp/dir".to_string()));
    }

    #[test]
    fn test_affected_paths_renamed() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Renamed {
            from: "/tmp/dir/old.txt".to_string(),
            to: "/tmp/new.txt".to_string()
        };
        let paths = sink.get_affected_paths(&event);

        assert_eq!(paths.len(), 4);
        assert!(paths.contains(&"/tmp/dir/old.txt".to_string())); // from
        assert!(paths.contains(&"/tmp/new.txt".to_string())); // to
        assert!(paths.contains(&"/tmp/dir".to_string())); // from parent
        assert!(paths.contains(&"/tmp".to_string())); // to parent
    }

    #[test]
    fn test_event_to_vnode_flags_created_file() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Created { path: "/tmp/test.txt".to_string() };
        let flags = sink.event_to_vnode_flags(&event, "/tmp/test.txt", false);

        assert_eq!(flags, NOTE_WRITE);
    }

    #[test]
    fn test_event_to_vnode_flags_removed_file() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Removed { path: "/tmp/test.txt".to_string() };
        let flags = sink.event_to_vnode_flags(&event, "/tmp/test.txt", false);

        assert_eq!(flags, NOTE_DELETE);
    }

    #[test]
    fn test_event_to_vnode_flags_modified_file() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Modified { path: "/tmp/test.txt".to_string() };
        let flags = sink.event_to_vnode_flags(&event, "/tmp/test.txt", false);

        assert_eq!(flags, NOTE_WRITE | NOTE_EXTEND);
    }

    #[test]
    fn test_event_to_vnode_flags_renamed_source() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Renamed {
            from: "/tmp/old.txt".to_string(),
            to: "/tmp/new.txt".to_string()
        };
        let flags = sink.event_to_vnode_flags(&event, "/tmp/old.txt", false);

        assert_eq!(flags, NOTE_RENAME);
    }

    #[test]
    fn test_event_to_vnode_flags_renamed_destination() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Renamed {
            from: "/tmp/old.txt".to_string(),
            to: "/tmp/new.txt".to_string()
        };
        let flags = sink.event_to_vnode_flags(&event, "/tmp/new.txt", false);

        assert_eq!(flags, NOTE_RENAME);
    }

    #[test]
    fn test_event_to_vnode_flags_renamed_parent() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Renamed {
            from: "/tmp/old.txt".to_string(),
            to: "/tmp/new.txt".to_string()
        };
        let flags = sink.event_to_vnode_flags(&event, "/tmp", false);

        assert_eq!(flags, NOTE_WRITE);
    }
}
