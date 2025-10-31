// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS Daemon - Watch service and event distribution

use agentfs_core::{EventKind, EventSink, FsCore};
use agentfs_proto::messages::*;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
use libc::{c_int, kevent as libc_kevent, timespec};

// kqueue types and constants (macOS) - using libc types directly

#[cfg(target_os = "macos")]
const EVFILT_USER: i16 = -5; // user events
#[cfg(target_os = "macos")]
const EV_ADD: u16 = 0x0001; // add event to kq (implies enable)
#[cfg(target_os = "macos")]
const EV_ENABLE: u16 = 0x0004; // enable event
#[cfg(target_os = "macos")]
const EV_CLEAR: u16 = 0x0020; // disable event after reporting
#[cfg(target_os = "macos")]
const NOTE_TRIGGER: u32 = 0x01000000; // trigger the event

// kqueue vnode event flags (macOS)
#[cfg(target_os = "macos")]
const EVFILT_VNODE: i16 = -4; // vnode events
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
    /// Doorbell idents for kqueues: (pid, kq_fd) -> doorbell_ident
    doorbell_idents: Mutex<HashMap<(u32, u32), u64>>,
    /// Next registration ID to assign
    next_registration_id: Mutex<u64>,
    /// Received kqueue file descriptors: (pid, kq_fd) -> actual_fd
    #[cfg(target_os = "macos")]
    kqueue_fds: Mutex<HashMap<(u32, u32), c_int>>,
    /// Pending synthesized events for each kqueue: (pid, kq_fd) -> Vec<SynthesizedKevent>
    pending_events: Mutex<HashMap<(u32, u32), VecDeque<SynthesizedKevent>>>,
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
    pub is_directory: bool,
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
            doorbell_idents: Mutex::new(HashMap::new()),
            next_registration_id: Mutex::new(1),
            #[cfg(target_os = "macos")]
            kqueue_fds: Mutex::new(HashMap::new()),
            pending_events: Mutex::new(HashMap::new()),
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
        is_directory: bool,
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
            is_directory,
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

    /// Store a received kqueue file descriptor
    #[cfg(target_os = "macos")]
    pub fn store_kqueue_fd(&self, pid: u32, kq_fd: u32, actual_fd: c_int) {
        self.kqueue_fds.lock().unwrap().insert((pid, kq_fd), actual_fd);
    }

    /// Set doorbell identifier for a kqueue
    pub fn set_doorbell(&self, pid: u32, kq_fd: u32, doorbell_ident: u64) {
        // Store in efficient lookup map
        self.doorbell_idents.lock().unwrap().insert((pid, kq_fd), doorbell_ident);

        // Also update in registrations for backward compatibility
        let mut watches = self.kqueue_watches.lock().unwrap();
        for (_, registration) in watches.iter_mut() {
            if registration.pid == pid && registration.kq_fd == kq_fd {
                registration.doorbell_ident = Some(doorbell_ident);
            }
        }
    }

    /// Post a doorbell to wake up a kqueue
    #[cfg(target_os = "macos")]
    pub fn post_doorbell(&self, pid: u32, kq_fd: u32, payload_id: u64) -> Result<(), String> {
        let doorbell_idents = self.doorbell_idents.lock().unwrap();
        let fds = self.kqueue_fds.lock().unwrap();

        // Get the doorbell ident for this kqueue
        let doorbell_ident = doorbell_idents.get(&(pid, kq_fd)).copied();

        // Get the actual kqueue FD
        let actual_kq_fd = fds.get(&(pid, kq_fd));

        match (doorbell_ident, actual_kq_fd) {
            (Some(ident), Some(&kq_fd_actual)) => {
                // Post EVFILT_USER NOTE_TRIGGER event to the kqueue
                let mut kev = libc::kevent {
                    ident: ident as usize,
                    filter: EVFILT_USER as i16,
                    flags: 0, // 0 means we're triggering, not adding
                    fflags: NOTE_TRIGGER | ((payload_id & 0xFFFFFF) as u32),
                    data: 0,
                    udata: std::ptr::null_mut(),
                };

                let timeout = timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                };

                unsafe {
                    let result =
                        libc_kevent(kq_fd_actual, &mut kev, 1, std::ptr::null_mut(), 0, &timeout);
                    if result == -1 {
                        Err(format!(
                            "kevent doorbell failed: {}",
                            std::io::Error::last_os_error()
                        ))
                    } else {
                        tracing::debug!(
                            "Posted doorbell ident={:#x}, payload_id={} to kqueue fd={} for pid={}",
                            ident,
                            payload_id,
                            kq_fd,
                            pid
                        );
                        Ok(())
                    }
                }
            }
            _ => {
                tracing::debug!(
                    "Cannot post doorbell: missing ident or FD for pid={}, kq_fd={}",
                    pid,
                    kq_fd
                );
                Ok(()) // Don't fail, just log
            }
        }
    }

    /// Post a doorbell to wake up a kqueue (fallback for non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn post_doorbell(&self, pid: u32, kq_fd: u32, payload_id: u64) -> Result<(), String> {
        // No-op on non-macOS platforms
        tracing::debug!(
            "Doorbell posting not implemented on this platform (pid={}, kq_fd={}, payload={})",
            pid,
            kq_fd,
            payload_id
        );
        Ok(())
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

    /// Get the current doorbell ident for a given (pid, kq_fd)
    pub fn get_doorbell_ident(&self, pid: u32, kq_fd: u32) -> u64 {
        self.doorbell_idents.lock().unwrap().get(&(pid, kq_fd)).copied().unwrap_or(0)
    }

    /// Get the current doorbell ident for a given pid (legacy method, finds first match)
    pub fn get_doorbell_ident_legacy(&self, pid: u32) -> u64 {
        let watches = self.kqueue_watches.lock().unwrap();
        for (_, registration) in watches.iter() {
            if registration.pid == pid {
                return registration.doorbell_ident.unwrap_or(0);
            }
        }
        0 // No doorbell ident found
    }

    /// Find kqueue fd for a given pid (returns the first match)
    pub fn find_kqueue_fd_for_pid(&self, pid: u32) -> Option<u32> {
        let watches = self.kqueue_watches.lock().unwrap();
        for (_, registration) in watches.iter() {
            if registration.pid == pid {
                return Some(registration.kq_fd);
            }
        }
        None
    }

    /// Enqueue a synthesized event for a specific kqueue
    /// Coalesces flags if an event for the same fd already exists
    pub fn enqueue_event(&self, pid: u32, kq_fd: u32, event: SynthesizedKevent) {
        let mut pending = self.pending_events.lock().unwrap();
        let key = (pid, kq_fd);
        let queue = pending.entry(key).or_insert_with(VecDeque::new);

        // Check if there's already an event for this fd - coalesce flags
        if let Some(existing) = queue.iter_mut().find(|e| e.ident == event.ident) {
            existing.fflags |= event.fflags;
        } else {
            queue.push_back(event);
        }
    }

    /// Drain pending events for a specific kqueue (up to max_events)
    pub fn drain_events(&self, pid: u32, kq_fd: u32, max_events: usize) -> Vec<SynthesizedKevent> {
        let mut pending = self.pending_events.lock().unwrap();
        let key = (pid, kq_fd);
        if let Some(queue) = pending.get_mut(&key) {
            let mut events = Vec::new();
            while events.len() < max_events {
                if let Some(event) = queue.pop_front() {
                    events.push(event);
                } else {
                    break;
                }
            }
            events
        } else {
            Vec::new()
        }
    }

    /// Get the count of pending events for a specific kqueue
    pub fn pending_event_count(&self, pid: u32, kq_fd: u32) -> usize {
        let pending = self.pending_events.lock().unwrap();
        let key = (pid, kq_fd);
        pending.get(&key).map(|q| q.len()).unwrap_or(0)
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
            is_directory: self.is_directory,
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
            EventKind::Created { path }
            | EventKind::Removed { path }
            | EventKind::Modified { path } => {
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

    /// Convert EventKind to kqueue vnode flags for a specific path and watcher context
    fn event_to_vnode_flags(
        &self,
        evt: &EventKind,
        watched_path: &str,
        affected_path: &str,
        is_directory_watcher: bool,
    ) -> u32 {
        if is_directory_watcher {
            // Directory watcher: check if affected_path is relevant
            let is_relevant = watched_path == affected_path
                || affected_path.starts_with(&(watched_path.to_string() + "/"));
            if !is_relevant {
                return 0;
            }

            match evt {
                EventKind::Created { .. }
                | EventKind::Removed { .. }
                | EventKind::Renamed { .. } => NOTE_WRITE,
                EventKind::Modified { .. } => NOTE_ATTRIB, // Directory sees child modifications as attribute changes
                _ => 0,
            }
        } else {
            // File watcher
            if watched_path != affected_path {
                return 0;
            }

            match evt {
                EventKind::Created { .. } => NOTE_WRITE,
                EventKind::Removed { .. } => NOTE_DELETE,
                EventKind::Modified { .. } => NOTE_WRITE | NOTE_EXTEND,
                EventKind::Renamed { from, to } => {
                    // For renames, the affected_path might be the source or destination
                    // But since we already checked watched_path == affected_path, it's fine
                    NOTE_RENAME
                }
                _ => 0,
            }
        }
    }

    /// Route event to matching kqueue watchers with coalescing
    fn route_to_kqueue_watchers(&self, evt: &EventKind, affected_paths: &[String]) {
        let watches = self.watch_service.kqueue_watches.lock().unwrap();

        // Collect all events per (pid, kq_fd, fd) to enable coalescing
        let mut coalesced_events: std::collections::HashMap<(u32, u32, u32), u32> =
            std::collections::HashMap::new();

        for affected_path in affected_paths {
            // Find all watches that could be interested in this affected path
            for (_, watch) in watches.iter() {
                // Check if this watch is relevant to the affected path
                let is_relevant = if watch.is_directory {
                    // Directory watchers are interested in:
                    // - Their own path (directory metadata changes)
                    // - Child paths (directory contents changes)
                    watch.path == *affected_path
                        || affected_path.starts_with(&(watch.path.clone() + "/"))
                        || (*affected_path == watch.path)
                } else {
                    // File watchers are only interested in exact path matches
                    watch.path == *affected_path
                };

                if !is_relevant {
                    continue;
                }

                // Calculate the flags for this watch and affected path combination
                let flags =
                    self.event_to_vnode_flags(evt, &watch.path, affected_path, watch.is_directory);

                // Only proceed if the watch is interested in these flags
                if (watch.fflags & flags) != 0 {
                    // Coalesce flags for this (pid, kq_fd, fd) combination
                    let key = (watch.pid, watch.kq_fd, watch.fd);
                    let existing_flags = coalesced_events.get(&key).copied().unwrap_or(0);
                    coalesced_events.insert(key, existing_flags | flags);

                    tracing::debug!(
                        "Coalescing kqueue event: pid={}, kq_fd={}, fd={}, affected_path={}, flags={:#x} (total now {:#x})",
                        watch.pid,
                        watch.kq_fd,
                        watch.fd,
                        affected_path,
                        flags,
                        existing_flags | flags
                    );
                }
            }
        }

        // Now create and enqueue the coalesced events
        for ((pid, kq_fd, fd), flags) in coalesced_events {
            tracing::debug!(
                "Creating coalesced kqueue event: pid={}, kq_fd={}, fd={}, flags={:#x}",
                pid,
                kq_fd,
                fd,
                flags
            );

            // Create synthesized kevent for this watcher
            let synthesized_event = SynthesizedKevent {
                ident: fd as u64,
                filter: EVFILT_VNODE as u16, // Convert to u16 for SSZ
                flags: 0,                    // No EV_ADD/EV_DELETE for synthesized events
                fflags: flags,
                data: 0,  // Usually 0 for vnode events
                udata: 0, // Usually NULL for synthesized events
            };

            // Enqueue the event for this kqueue
            self.watch_service.enqueue_event(pid, kq_fd, synthesized_event);

            // Post doorbell to wake up the shim (only once per kqueue)
            if let Err(e) = self.watch_service.post_doorbell(pid, kq_fd, 1) {
                tracing::error!("Failed to post doorbell for event: {}", e);
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
                    if affected_path.starts_with(root_path)
                        || std::path::Path::new(affected_path).starts_with(root_path)
                    {
                        should_notify = true;
                        break;
                    }
                }
                if should_notify {
                    break;
                }
            }

            if should_notify {
                tracing::debug!(
                    "Routing FSEvents event: pid={}, stream_id={}",
                    watch.pid,
                    watch.stream_id
                );
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
        let registration_id =
            service.register_kqueue_watch(123, 5, 1, 10, "/tmp/test.txt".to_string(), 0x123, false);
        assert_eq!(registration_id, 1);

        let watches = service.get_kqueue_watches_for_pid(123);
        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0].kq_fd, 5);
        assert_eq!(watches[0].fd, 10);
        assert_eq!(watches[0].path, "/tmp/test.txt");
        assert_eq!(watches[0].fflags, 0x123);
        assert_eq!(watches[0].is_directory, false);
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
        let reg_id =
            service.register_kqueue_watch(123, 5, 1, 10, "/tmp/test.txt".to_string(), 0x123, false);
        assert_eq!(service.get_kqueue_watches_for_pid(123).len(), 1);

        service.unregister_watch(123, reg_id);
        assert_eq!(service.get_kqueue_watches_for_pid(123).len(), 0);
    }

    #[test]
    fn test_doorbell_setting() {
        let service = WatchService::new();
        service.register_kqueue_watch(123, 5, 1, 10, "/tmp/test.txt".to_string(), 0x123, false);
        service.set_doorbell(123, 5, 0xABC);

        let watches = service.get_kqueue_watches_for_pid(123);
        assert_eq!(watches[0].doorbell_ident, Some(0xABC));
    }

    #[test]
    fn test_affected_paths_created() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Created {
            path: "/tmp/dir/file.txt".to_string(),
        };
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
            to: "/tmp/new.txt".to_string(),
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

        let event = EventKind::Created {
            path: "/tmp/test.txt".to_string(),
        };
        let flags = sink.event_to_vnode_flags(&event, "/tmp/test.txt", "/tmp/test.txt", false);

        assert_eq!(flags, NOTE_WRITE);
    }

    #[test]
    fn test_event_to_vnode_flags_removed_file() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Removed {
            path: "/tmp/test.txt".to_string(),
        };
        let flags = sink.event_to_vnode_flags(&event, "/tmp/test.txt", "/tmp/test.txt", false);

        assert_eq!(flags, NOTE_DELETE);
    }

    #[test]
    fn test_event_to_vnode_flags_modified_file() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Modified {
            path: "/tmp/test.txt".to_string(),
        };
        let flags = sink.event_to_vnode_flags(&event, "/tmp/test.txt", "/tmp/test.txt", false);

        assert_eq!(flags, NOTE_WRITE | NOTE_EXTEND);
    }

    #[test]
    fn test_event_to_vnode_flags_renamed_source() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Renamed {
            from: "/tmp/old.txt".to_string(),
            to: "/tmp/new.txt".to_string(),
        };
        let flags = sink.event_to_vnode_flags(&event, "/tmp/old.txt", "/tmp/old.txt", false);

        assert_eq!(flags, NOTE_RENAME);
    }

    #[test]
    fn test_event_to_vnode_flags_renamed_destination() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Renamed {
            from: "/tmp/old.txt".to_string(),
            to: "/tmp/new.txt".to_string(),
        };
        let flags = sink.event_to_vnode_flags(&event, "/tmp/new.txt", "/tmp/new.txt", false);

        assert_eq!(flags, NOTE_RENAME);
    }

    #[test]
    fn test_event_to_vnode_flags_renamed_parent() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Renamed {
            from: "/tmp/old.txt".to_string(),
            to: "/tmp/new.txt".to_string(),
        };
        let flags = sink.event_to_vnode_flags(&event, "/tmp", "/tmp", true); // directory watcher

        assert_eq!(flags, NOTE_WRITE);
    }

    #[test]
    fn test_event_to_vnode_flags_directory_watcher_created() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Created {
            path: "/tmp/dir/file.txt".to_string(),
        };

        // Directory watcher on parent directory should get NOTE_WRITE
        let flags = sink.event_to_vnode_flags(&event, "/tmp/dir", "/tmp/dir/file.txt", true);
        assert_eq!(flags, NOTE_WRITE);

        // Directory watcher on unrelated directory should get 0
        let flags = sink.event_to_vnode_flags(&event, "/tmp/other", "/tmp/dir/file.txt", true);
        assert_eq!(flags, 0);
    }

    #[test]
    fn test_event_to_vnode_flags_file_watcher_created() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Created {
            path: "/tmp/file.txt".to_string(),
        };

        // File watcher on the created file should get NOTE_WRITE
        let flags = sink.event_to_vnode_flags(&event, "/tmp/file.txt", "/tmp/file.txt", false);
        assert_eq!(flags, NOTE_WRITE);

        // File watcher on different file should get 0
        let flags = sink.event_to_vnode_flags(&event, "/tmp/other.txt", "/tmp/file.txt", false);
        assert_eq!(flags, 0);
    }

    #[test]
    fn test_event_to_vnode_flags_directory_watcher_removed() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Removed {
            path: "/tmp/dir/file.txt".to_string(),
        };

        // Directory watcher on parent should get NOTE_WRITE
        let flags = sink.event_to_vnode_flags(&event, "/tmp/dir", "/tmp/dir/file.txt", true);
        assert_eq!(flags, NOTE_WRITE);
    }

    #[test]
    fn test_event_to_vnode_flags_file_watcher_removed() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Removed {
            path: "/tmp/file.txt".to_string(),
        };

        // File watcher on the removed file should get NOTE_DELETE
        let flags = sink.event_to_vnode_flags(&event, "/tmp/file.txt", "/tmp/file.txt", false);
        assert_eq!(flags, NOTE_DELETE);
    }

    #[test]
    fn test_event_to_vnode_flags_directory_watcher_modified() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Modified {
            path: "/tmp/dir/file.txt".to_string(),
        };

        // Directory watcher should get NOTE_ATTRIB for child modifications
        let flags = sink.event_to_vnode_flags(&event, "/tmp/dir", "/tmp/dir/file.txt", true);
        assert_eq!(flags, NOTE_ATTRIB);
    }

    #[test]
    fn test_event_to_vnode_flags_file_watcher_modified() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        let event = EventKind::Modified {
            path: "/tmp/file.txt".to_string(),
        };

        // File watcher should get NOTE_WRITE | NOTE_EXTEND
        let flags = sink.event_to_vnode_flags(&event, "/tmp/file.txt", "/tmp/file.txt", false);
        assert_eq!(flags, NOTE_WRITE | NOTE_EXTEND);
    }

    #[test]
    fn test_event_coalescing_created_file() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        // Register a file watch on /tmp/file.txt
        service.register_kqueue_watch(
            123,
            5,
            1,
            10,
            "/tmp/file.txt".to_string(),
            NOTE_WRITE | NOTE_DELETE,
            false,
        );

        // Register a directory watch on /tmp
        service.register_kqueue_watch(123, 5, 2, 11, "/tmp".to_string(), NOTE_WRITE, true);

        let event = EventKind::Created {
            path: "/tmp/file.txt".to_string(),
        };

        sink.on_event(&event);

        // Should have 2 events: one for file watcher, one for directory watcher
        assert_eq!(service.pending_event_count(123, 5), 2);

        let events = service.drain_events(123, 5, 10);
        assert_eq!(events.len(), 2);

        // Find events by fd
        let file_event = events.iter().find(|e| e.ident == 10).unwrap();
        let dir_event = events.iter().find(|e| e.ident == 11).unwrap();

        // File watcher gets NOTE_WRITE
        assert_eq!(file_event.fflags, NOTE_WRITE);
        // Directory watcher gets NOTE_WRITE
        assert_eq!(dir_event.fflags, NOTE_WRITE);
    }

    #[test]
    fn test_event_coalescing_multiple_operations() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        // Register a file watch that wants all events
        service.register_kqueue_watch(
            123,
            5,
            1,
            10,
            "/tmp/file.txt".to_string(),
            NOTE_WRITE | NOTE_DELETE | NOTE_EXTEND | NOTE_ATTRIB | NOTE_LINK | NOTE_RENAME,
            false,
        );

        // Simulate multiple operations on the same file
        let create_event = EventKind::Created {
            path: "/tmp/file.txt".to_string(),
        };
        let modify_event = EventKind::Modified {
            path: "/tmp/file.txt".to_string(),
        };

        sink.on_event(&create_event);
        sink.on_event(&modify_event);

        // Should coalesce into one event with combined flags
        assert_eq!(service.pending_event_count(123, 5), 1);

        let events = service.drain_events(123, 5, 10);
        assert_eq!(events.len(), 1);

        let event = &events[0];
        assert_eq!(event.ident, 10);
        // Should have NOTE_WRITE from create + NOTE_WRITE|NOTE_EXTEND from modify
        assert_eq!(event.fflags, NOTE_WRITE | NOTE_EXTEND);
    }

    #[test]
    fn test_event_coalescing_renamed() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        // Register watches on both source and destination files
        service.register_kqueue_watch(
            123,
            5,
            1,
            10,
            "/tmp/old.txt".to_string(),
            NOTE_RENAME,
            false,
        );
        service.register_kqueue_watch(
            123,
            5,
            2,
            11,
            "/tmp/new.txt".to_string(),
            NOTE_RENAME,
            false,
        );
        // Register directory watch on parent
        service.register_kqueue_watch(123, 5, 3, 12, "/tmp".to_string(), NOTE_WRITE, true);

        let event = EventKind::Renamed {
            from: "/tmp/old.txt".to_string(),
            to: "/tmp/new.txt".to_string(),
        };

        sink.on_event(&event);

        // Should have 3 events: source file, dest file, and parent directory
        assert_eq!(service.pending_event_count(123, 5), 3);

        let events = service.drain_events(123, 5, 10);
        assert_eq!(events.len(), 3);

        // Check each event
        let old_file_event = events.iter().find(|e| e.ident == 10).unwrap();
        let new_file_event = events.iter().find(|e| e.ident == 11).unwrap();
        let dir_event = events.iter().find(|e| e.ident == 12).unwrap();

        assert_eq!(old_file_event.fflags, NOTE_RENAME);
        assert_eq!(new_file_event.fflags, NOTE_RENAME);
        assert_eq!(dir_event.fflags, NOTE_WRITE);
    }

    #[test]
    fn test_directory_watcher_child_events() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        // Register directory watch on /tmp
        service.register_kqueue_watch(123, 5, 1, 10, "/tmp".to_string(), NOTE_WRITE, true);

        let event = EventKind::Created {
            path: "/tmp/subdir/file.txt".to_string(),
        };

        sink.on_event(&event);

        // Directory watcher should get NOTE_WRITE for child creation
        assert_eq!(service.pending_event_count(123, 5), 1);

        let events = service.drain_events(123, 5, 10);
        assert_eq!(events.len(), 1);

        let event = &events[0];
        assert_eq!(event.ident, 10);
        assert_eq!(event.fflags, NOTE_WRITE);
    }

    #[test]
    fn test_no_events_for_uninterested_watchers() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        // Register a file watch that only wants NOTE_DELETE
        service.register_kqueue_watch(
            123,
            5,
            1,
            10,
            "/tmp/file.txt".to_string(),
            NOTE_DELETE,
            false,
        );

        let event = EventKind::Created {
            path: "/tmp/file.txt".to_string(),
        };

        sink.on_event(&event);

        // Watcher is not interested in NOTE_WRITE, so no event should be generated
        assert_eq!(service.pending_event_count(123, 5), 0);
    }

    #[test]
    fn test_no_events_for_unrelated_paths() {
        let service = Arc::new(WatchService::new());
        let sink = WatchServiceEventSink::new(service.clone());

        // Register a file watch on /tmp/file.txt
        service.register_kqueue_watch(
            123,
            5,
            1,
            10,
            "/tmp/file.txt".to_string(),
            NOTE_WRITE,
            false,
        );

        let event = EventKind::Created {
            path: "/tmp/other.txt".to_string(),
        };

        sink.on_event(&event);

        // No events should be generated for unrelated paths
        assert_eq!(service.pending_event_count(123, 5), 0);
    }
}
