// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(non_snake_case)] // Allow FFI/macOS symbol-style names for interposed functions
#![allow(clippy::too_many_arguments)] // Many interposed libc symbols exceed clippy's arg threshold
#![allow(unused_doc_comments)] // Macro-generated hooks don't retain doc comments

use libc::{c_int, c_void};
use once_cell::sync::{Lazy, OnceCell};
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, OsStr};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use core_foundation::{base::TCFType, declare_TCFType, impl_TCFType};
use scopeguard::guard;

// SSZ imports
use agentfs_client::{AgentFsClient, AllowlistConfig, ClientConfig, ProcessConfig};
use ssz::{Decode, Encode};

// AgentFS proto imports
use agentfs_proto::messages::{
    AclDeleteDefFileRequest, AclGetFdRequest, AclGetFileRequest, AclSetFdRequest,
    AclSetFileRequest, ChflagsRequest, ClonefileRequest, CopyfileRequest, DirEntry,
    FchflagsRequest, FclonefileatRequest, FcopyfileRequest, FgetxattrRequest, FlistxattrRequest,
    FremovexattrRequest, FsEventBroadcastRequest, FsetxattrRequest, GetattrlistRequest,
    GetattrlistbulkRequest, GetxattrRequest, LchflagsRequest, ListxattrRequest, RemovexattrRequest,
    Request, Response, SetattrlistRequest, SetxattrRequest, StatData, StatfsData,
    SynthesizedKevent, TimespecData, WatchRegisterFSEventsRequest,
};

// Error codes for interpose forwarding failures
#[allow(dead_code)]
const FORWARDING_UNAVAILABLE: u32 = 1;

use std::os::unix::net::UnixStream;

// FSEvents type definitions (CoreFoundation types)
type CFAllocatorRef = *mut libc::c_void;
type CFArrayRef = *mut libc::c_void;
type CFTimeInterval = f64;
type FSEventStreamEventFlags = u32;
type FSEventStreamRef = *mut libc::c_void;

// FSEvents flag constants
#[allow(non_upper_case_globals)]
const kFSEventStreamEventFlagItemCreated: FSEventStreamEventFlags = 0x00000100;
#[allow(non_upper_case_globals)]
const kFSEventStreamEventFlagItemRemoved: FSEventStreamEventFlags = 0x00000200;
#[allow(non_upper_case_globals)]
const kFSEventStreamEventFlagItemModified: FSEventStreamEventFlags = 0x00001000;
#[allow(non_upper_case_globals)]
const kFSEventStreamEventFlagItemRenamed: FSEventStreamEventFlags = 0x00000800;
#[allow(non_upper_case_globals, dead_code)]
const kFSEventStreamEventFlagItemIsFile: FSEventStreamEventFlags = 0x00010000;
#[allow(non_upper_case_globals, dead_code)]
const kFSEventStreamEventFlagItemIsDir: FSEventStreamEventFlags = 0x00020000;
type FSEventStreamCallback = extern "C" fn(
    stream_ref: FSEventStreamRef,
    client_callback_info: *mut libc::c_void,
    num_events: libc::size_t,
    event_paths: *mut libc::c_void, // CFArrayRef
    event_flags: *const FSEventStreamEventFlags,
    event_ids: *const u64,
);
#[repr(C)]
struct FSEventStreamContext {
    version: CFIndex,
    info: *mut libc::c_void,
    retain: Option<extern "C" fn(info: *const libc::c_void) -> *const libc::c_void>,
    release: Option<extern "C" fn(info: *const libc::c_void)>,
    copy_description: Option<extern "C" fn(info: *const libc::c_void) -> CFStringRef>,
}
type CFStringRef = *mut libc::c_void;
type CFMessagePortRef = *mut libc::c_void;
type CFRunLoopRef = *mut libc::c_void;
type CFRunLoopSourceRef = *mut libc::c_void;
type CFDataRef = *mut libc::c_void;
type CFIndex = isize;
type SInt32 = i32;
type Boolean = u8;
#[allow(non_camel_case_types)]
type dev_t = u32; // macOS style typedef

// Wrapper types for thread-safe CF objects
#[allow(dead_code)]
struct CFMessagePortWrapper(CFMessagePortRef);
#[allow(dead_code)]
struct CFRunLoopSourceWrapper(CFRunLoopSourceRef);
#[derive(Hash, Eq, PartialEq)]
struct CFRunLoopSourceKey(CFRunLoopRef, CFStringRef);

// RAII wrapper for CFMessagePort with proper invalidation and release
#[inline]
fn cf_message_port_get_type_id() -> usize {
    unsafe { CFMessagePortGetTypeID() as usize }
}

declare_TCFType!(CFMessagePort, CFMessagePortRef);
impl_TCFType!(CFMessagePort, CFMessagePortRef, cf_message_port_get_type_id);

// RAII wrapper for CF objects with release capability (like auto_ptr.release())
struct OwnedCFRef {
    ptr: Option<*mut libc::c_void>,
}

impl OwnedCFRef {
    fn new(ptr: *mut libc::c_void) -> Self {
        Self { ptr: Some(ptr) }
    }

    // Transfer ownership out (like auto_ptr.release())
    #[allow(dead_code)]
    fn release(mut self) -> *mut libc::c_void {
        self.ptr.take().expect("CF object already released")
    }
}

impl Drop for OwnedCFRef {
    fn drop(&mut self) {
        if let Some(ptr) = self.ptr {
            unsafe { CFRelease(ptr) };
        }
    }
}

unsafe impl Send for OwnedCFRef {}
unsafe impl Sync for OwnedCFRef {}

// RAII wrapper for CFString
struct OwnedCFString(CFStringRef);

impl OwnedCFString {
    fn new(ptr: CFStringRef) -> Self {
        Self(ptr)
    }

    fn as_cf_string_ref(&self) -> CFStringRef {
        self.0
    }
}

impl Drop for OwnedCFString {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

unsafe impl Send for OwnedCFString {}
unsafe impl Sync for OwnedCFString {}

// Outer wrapper that ensures invalidation happens before CFRelease
struct OwnedMessagePort {
    inner: CFMessagePort,
}

impl OwnedMessagePort {
    fn new(port: CFMessagePortRef) -> Self {
        Self {
            inner: unsafe { CFMessagePort::wrap_under_create_rule(port) },
        }
    }
}

// CF objects are thread-safe according to Apple's documentation
unsafe impl Send for OwnedMessagePort {}
unsafe impl Sync for OwnedMessagePort {}

impl Drop for OwnedMessagePort {
    fn drop(&mut self) {
        unsafe { CFMessagePortInvalidate(self.inner.as_concrete_TypeRef()) };
        // inner drops automatically and calls CFRelease
    }
}

// CF objects are thread-safe according to Apple's documentation
unsafe impl Send for CFMessagePortWrapper {}
unsafe impl Sync for CFMessagePortWrapper {}
unsafe impl Send for CFRunLoopSourceWrapper {}
unsafe impl Sync for CFRunLoopSourceWrapper {}
unsafe impl Send for CFRunLoopSourceKey {}
unsafe impl Sync for CFRunLoopSourceKey {}

// CoreFoundation extern declarations for CFMessagePort
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFAllocatorDefault: CFAllocatorRef;

    fn CFMessagePortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMessagePortRef,
        order: CFIndex,
    ) -> CFRunLoopSourceRef;

    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRemoveSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFMessagePortInvalidate(port: CFMessagePortRef);
    fn CFMessagePortGetTypeID() -> u64;
    fn CFRelease(cf: *mut std::ffi::c_void);
    fn CFRetain(cf: *const std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        c_str: *const std::ffi::c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFArrayCreate(
        allocator: CFAllocatorRef,
        values: *const *const libc::c_void,
        num_values: CFIndex,
        callbacks: *const libc::c_void,
    ) -> CFArrayRef;

    fn CFMessagePortCreateLocal(
        allocator: CFAllocatorRef,
        name: CFStringRef,
        callout: CFMessagePortCallBack,
        context: *const CFMessagePortContext,
        should_free_info: *mut Boolean,
    ) -> CFMessagePortRef;

    // CFArray functions for extracting paths
    fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
    fn CFArrayGetValueAtIndex(array: CFArrayRef, idx: CFIndex) -> *const std::ffi::c_void;

    // Property list functions
    fn CFPropertyListCreateWithData(
        allocator: CFAllocatorRef,
        data: CFDataRef,
        options: u64,
        format: *mut u32,
        error: *mut *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;

    // CFDictionary functions
    fn CFDictionaryGetValue(
        dict: *const std::ffi::c_void,
        key: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;

    // CFNumber functions
    fn CFNumberGetValue(
        number: *const std::ffi::c_void,
        the_type: u64,
        value_ptr: *mut std::ffi::c_void,
    ) -> Boolean;

    // CFType functions for type checking
    fn CFGetTypeID(cf: *const std::ffi::c_void) -> u64;
    fn CFDictionaryGetTypeID() -> u64;
    fn CFArrayGetTypeID() -> u64;
    fn CFStringGetTypeID() -> u64;
    fn CFNumberGetTypeID() -> u64;

    // CFString functions for converting to C string
    fn CFStringGetCStringPtr(cf_str: CFStringRef, encoding: u32) -> *const std::ffi::c_char;
    fn CFStringGetLength(cf_str: CFStringRef) -> CFIndex;
    fn CFStringGetCString(
        cf_str: CFStringRef,
        buffer: *mut std::ffi::c_char,
        buffer_size: CFIndex,
        encoding: u32,
    ) -> bool;
}

const KCFSTRINGENCODING_UTF8: u32 = 0x0800_0100;

type CFMessagePortCallBack = extern "C" fn(
    local: CFMessagePortRef,
    msgid: SInt32,
    data: CFDataRef,
    info: *mut std::ffi::c_void,
) -> CFDataRef;

#[repr(C)]
struct CFMessagePortContext {
    version: CFIndex,
    info: *mut std::ffi::c_void,
    retain: Option<extern "C" fn(info: *const std::ffi::c_void) -> *const std::ffi::c_void>,
    release: Option<extern "C" fn(info: *const std::ffi::c_void)>,
    copy_description: Option<extern "C" fn(info: *const std::ffi::c_void) -> CFStringRef>,
}

// kqueue/kevent types and constants
type KeventStruct = libc::kevent;
const EVFILT_VNODE: i16 = -4; // vnode events
const EVFILT_USER: i16 = -5; // user events
const EV_ADD: u16 = 0x0001; // add event to kq (implies enable)
const EV_DELETE: u16 = 0x0002; // delete event from kq
const EV_ENABLE: u16 = 0x0004; // enable event
const EV_CLEAR: u16 = 0x0020; // disable event after reporting
#[allow(dead_code)]
const NOTE_TRIGGER: u32 = 0x01000000; // trigger the event

/// Wrapper for storing original FSEvents callback information
struct FSEventsCallbackInfo {
    original_callback: FSEventStreamCallback,
    original_context: usize, // Store context as usize to avoid Send/Sync issues
    stream_id: u64,
}

/// Global storage for FSEvents callback information, keyed by stream reference
static FSEVENTS_CALLBACKS: Lazy<Mutex<HashMap<usize, FSEventsCallbackInfo>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Counter for generating unique FSEvents stream IDs
static FSEVENTS_STREAM_ID_COUNTER: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

const LOG_PREFIX: &str = "[agentfs-interpose]";
const ENV_ENABLED: &str = "AGENTFS_INTERPOSE_ENABLED";
const ENV_SOCKET: &str = "AGENTFS_INTERPOSE_SOCKET";
const ENV_ALLOWLIST: &str = "AGENTFS_INTERPOSE_ALLOWLIST";
const ENV_LOG_LEVEL: &str = "AGENTFS_INTERPOSE_LOG";
const ENV_FAIL_FAST: &str = "AGENTFS_INTERPOSE_FAIL_FAST";
const ENV_BRANCH_ID: &str = "AGENTFS_BRANCH_ID";
const DEFAULT_BANNER: &str = "AgentFS interpose shim loaded";

/// Per-process directory file descriptor mapping
#[derive(Clone, Debug)]
struct DirfdMapping {
    /// Current working directory for AT_FDCWD resolution
    cwd: PathBuf,
    /// File descriptor to path mappings
    fd_paths: HashMap<RawFd, PathBuf>,
}

impl DirfdMapping {
    fn new() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            fd_paths: HashMap::new(),
        }
    }

    /// Get the path for a directory file descriptor
    fn get_path(&self, dirfd: RawFd) -> Option<&PathBuf> {
        self.fd_paths.get(&dirfd)
    }

    /// Set the path for a directory file descriptor
    fn set_path(&mut self, dirfd: RawFd, path: PathBuf) {
        self.fd_paths.insert(dirfd, path);
    }

    /// Remove a directory file descriptor mapping
    fn remove_path(&mut self, dirfd: RawFd) {
        self.fd_paths.remove(&dirfd);
    }

    /// Update current working directory
    fn set_cwd(&mut self, cwd: PathBuf) {
        self.cwd = cwd;
    }

    /// Duplicate file descriptor mapping
    fn dup_fd(&mut self, old_fd: RawFd, new_fd: RawFd) {
        if let Some(path) = self.fd_paths.get(&old_fd).cloned() {
            self.fd_paths.insert(new_fd, path);
        }
    }
}

/// Generate a random doorbell ident for kqueue
fn generate_doorbell_ident() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Use current time as randomness source (good enough for this use case)
    // Reserve high bits to avoid collision with app's ident space
    let timestamp =
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u32;
    0xAFFE00000000u64 | (timestamp as u64)
}

/// Translate FSEvents paths from overlay to backstore paths
#[allow(dead_code)]
fn translate_fsevent_paths(_paths_to_watch: CFArrayRef) -> Result<Vec<Vec<u8>>, String> {
    // TODO: Implement CoreFoundation array manipulation to extract paths from CFArray
    // For now, we can't extract the paths from the CFArray, but we can at least
    // attempt to send a translation request with empty paths to test the protocol
    // and log that translation would be needed.

    // In a full implementation, we would:
    // 1. Extract CFString objects from the CFArray
    // 2. Convert them to Rust strings
    // 3. Check if they are overlay paths (start with the mount point)
    // 4. Send FSEventsTranslatePaths request to AgentFS
    // 5. Get back translated paths
    // 6. Return them for creating a new CFArray

    // For now, just return empty vec to indicate "no translation needed"
    // but log that we would translate if we could extract the paths
    log_message("FSEvents path translation: CoreFoundation array extraction not implemented yet");
    Ok(Vec::new())
}

/// Replacement FSEvents callback that intercepts and modifies events
extern "C" fn fsevents_replacement_callback(
    stream_ref: FSEventStreamRef,
    _client_callback_info: *mut libc::c_void,
    num_events: libc::size_t,
    event_paths: CFArrayRef,
    event_flags: *const FSEventStreamEventFlags,
    event_ids: *const u64,
) {
    // Get the original callback information
    let stream_key = stream_ref as usize;
    let callbacks = FSEVENTS_CALLBACKS.lock().unwrap();

    if let Some(callback_info) = callbacks.get(&stream_key) {
        log_message(&format!(
            "FSEvents replacement callback: intercepting {} events for stream {:p} (id={})",
            num_events, stream_ref, callback_info.stream_id
        ));

        // Get any queued AgentFS events for this stream
        let queued_broadcasts = drain_fsevents_broadcasts(callback_info.stream_id);

        if !queued_broadcasts.is_empty() {
            log_message(&format!(
                "FSEvents: injecting {} queued AgentFS events for stream {}",
                queued_broadcasts.len(),
                callback_info.stream_id
            ));

            // Create synthetic FSEvents data from queued broadcasts
            let mut synthetic_paths: Vec<OwnedCFString> = Vec::new();
            let mut synthetic_flags = Vec::new();
            let mut synthetic_event_ids = Vec::new();

            for broadcast in &queued_broadcasts {
                // Convert path to CFString
                let path_str = String::from_utf8_lossy(&broadcast.path);
                let cf_path = unsafe {
                    CFStringCreateWithCString(
                        kCFAllocatorDefault,
                        path_str.as_ptr() as *const libc::c_char,
                        0x08000100,
                    )
                };
                synthetic_paths.push(OwnedCFString::new(cf_path));

                // Convert event kind to FSEvents flags
                let flags = match broadcast.event_kind {
                    0 => kFSEventStreamEventFlagItemCreated,  // Created
                    1 => kFSEventStreamEventFlagItemRemoved,  // Removed
                    2 => kFSEventStreamEventFlagItemModified, // Modified
                    3 => kFSEventStreamEventFlagItemRenamed,  // Renamed
                    _ => kFSEventStreamEventFlagItemModified, // Default to modified
                };

                // For renames, we might need to handle both paths
                if broadcast.event_kind == 3 {
                    // Renamed
                    if let Some(aux_path) = &broadcast.aux_path {
                        let aux_path_str = String::from_utf8_lossy(aux_path);
                        let cf_aux_path = unsafe {
                            CFStringCreateWithCString(
                                kCFAllocatorDefault,
                                aux_path_str.as_ptr() as *const libc::c_char,
                                0x08000100,
                            )
                        };
                        synthetic_paths.push(OwnedCFString::new(cf_aux_path));
                        synthetic_flags.push(flags);
                        synthetic_event_ids.push(broadcast.seqno);
                    }
                }

                synthetic_flags.push(flags);
                synthetic_event_ids.push(broadcast.seqno);
            }

            // Create CFArray from synthetic paths
            let cf_paths_array_wrapper = if !synthetic_paths.is_empty() {
                // Extract raw pointers for CFArray creation
                let path_ptrs: Vec<CFStringRef> =
                    synthetic_paths.iter().map(|owned| owned.as_cf_string_ref()).collect();

                let array = unsafe {
                    CFArrayCreate(
                        kCFAllocatorDefault,
                        path_ptrs.as_ptr() as *const *const libc::c_void,
                        path_ptrs.len() as CFIndex,
                        std::ptr::null(),
                    )
                };
                Some(OwnedCFRef::new(array))
            } else {
                None
            };

            // Call original callback with synthetic events
            if !synthetic_paths.is_empty() && cf_paths_array_wrapper.is_some() {
                if let Some(wrapper) = cf_paths_array_wrapper.as_ref() {
                    let cf_paths_array = wrapper.ptr.unwrap() as CFArrayRef;
                    (callback_info.original_callback)(
                        stream_ref,
                        callback_info.original_context as *mut libc::c_void,
                        synthetic_paths.len(),
                        cf_paths_array,
                        synthetic_flags.as_ptr(),
                        synthetic_event_ids.as_ptr(),
                    );
                }
            }

            // CFString objects and CFArray will be cleaned up automatically by RAII
        }

        // TODO: Implement event filtering based on AgentFS overlay state
        // - Check paths against whiteouts
        // - Translate backstore paths to overlay paths

        // Forward the original events to the real callback
        (callback_info.original_callback)(
            stream_ref,
            callback_info.original_context as *mut libc::c_void,
            num_events,
            event_paths,
            event_flags,
            event_ids,
        );
    } else {
        log_message(&format!(
            "FSEvents replacement callback: no callback info found for stream {:p}",
            stream_ref
        ));
    }
}

/// Queue an FSEvents broadcast for injection into the callback
#[allow(dead_code)]
fn queue_fsevents_broadcast(stream_id: u64, broadcast: FsEventBroadcastRequest) {
    let mut queue = FSEVENTS_QUEUE.lock().unwrap();
    queue.entry(stream_id).or_default().push(broadcast);
}

/// Get and clear queued FSEvents broadcasts for a stream
fn drain_fsevents_broadcasts(stream_id: u64) -> Vec<FsEventBroadcastRequest> {
    let mut queue = FSEVENTS_QUEUE.lock().unwrap();
    queue.remove(&stream_id).unwrap_or_default()
}

/// CFMessagePort callback for receiving FSEvents batches from daemon
extern "C" fn fsevents_message_port_callback(
    _local: CFMessagePortRef,
    msgid: SInt32,
    data: CFDataRef,
    _info: *mut c_void,
) -> CFDataRef {
    // Catch panics to prevent unwinds crossing FFI boundaries
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fsevents_message_port_callback_impl(_local, msgid, data, _info)
    })) {
        Ok(result) => result,
        Err(_) => {
            log_message("FSEvents port callback panicked, returning null");
            std::ptr::null_mut()
        }
    }
}

/// Implementation of the message port callback with RAII cleanup
fn fsevents_message_port_callback_impl(
    _local: CFMessagePortRef,
    msgid: SInt32,
    data: CFDataRef,
    _info: *mut c_void,
) -> CFDataRef {
    // We only handle our specific message ID
    const AGENTFS_MSG_FSEVENTS_BATCH: SInt32 = 0x1001;
    if msgid != AGENTFS_MSG_FSEVENTS_BATCH {
        log_message(&format!("FSEvents port received unknown msgid: {}", msgid));
        return std::ptr::null_mut();
    }

    if data.is_null() {
        log_message("FSEvents port received null data");
        return std::ptr::null_mut();
    }

    // Deserialize binary property list
    unsafe {
        let mut format_out: u32 = 0;
        let mut error: *mut std::ffi::c_void = std::ptr::null_mut();

        let plist_raw = CFPropertyListCreateWithData(
            kCFAllocatorDefault,
            data,
            0, // options
            &mut format_out,
            &mut error,
        );

        if !error.is_null() {
            // error will be cleaned up by RAII
            let _error = OwnedCFRef::new(error);
            log_message("FSEvents port: failed to deserialize property list");
            return std::ptr::null_mut();
        }

        if plist_raw.is_null() {
            log_message("FSEvents port: property list deserialization returned null");
            return std::ptr::null_mut();
        }

        // Wrap plist in RAII wrapper
        let _plist = OwnedCFRef::new(plist_raw);

        // Validate it's a dictionary
        let dict_type_id = CFDictionaryGetTypeID();
        let plist_type_id = CFGetTypeID(plist_raw);
        if plist_type_id != dict_type_id {
            log_message("FSEvents port: property list is not a dictionary");
            return std::ptr::null_mut();
        }

        // Extract dictionary values - create CFString keys with RAII
        let type_key = core_foundation::string::CFString::new("type");
        let version_key = core_foundation::string::CFString::new("version");
        let stream_id_key = core_foundation::string::CFString::new("stream_id");
        let num_events_key = core_foundation::string::CFString::new("num_events");
        let paths_key = core_foundation::string::CFString::new("paths");
        let flags_key = core_foundation::string::CFString::new("flags");
        let event_ids_key = core_foundation::string::CFString::new("event_ids");

        // Validate type
        let type_value = CFDictionaryGetValue(
            plist_raw,
            type_key.as_concrete_TypeRef() as *const std::ffi::c_void,
        );
        if type_value.is_null() {
            log_message("FSEvents port: missing 'type' field");
            return std::ptr::null_mut();
        }

        // Get version
        let version_value = CFDictionaryGetValue(
            plist_raw,
            version_key.as_concrete_TypeRef() as *const std::ffi::c_void,
        );
        if version_value.is_null() {
            log_message("FSEvents port: missing 'version' field");
            return std::ptr::null_mut();
        }

        let mut version: i32 = 0;
        if CFNumberGetValue(
            version_value,
            9, /* kCFNumberIntType */
            &mut version as *mut i32 as *mut std::ffi::c_void,
        ) == 0
        {
            log_message("FSEvents port: 'version' is not a number");
            return std::ptr::null_mut();
        }

        if version != 1 {
            log_message(&format!("FSEvents port: unsupported version: {}", version));
            return std::ptr::null_mut();
        }

        // Get stream_id
        let stream_id_value = CFDictionaryGetValue(
            plist_raw,
            stream_id_key.as_concrete_TypeRef() as *const std::ffi::c_void,
        );
        if stream_id_value.is_null() {
            log_message("FSEvents port: missing 'stream_id' field");
            return std::ptr::null_mut();
        }

        let mut stream_id: u64 = 0;
        if CFNumberGetValue(
            stream_id_value,
            4, /* kCFNumberSInt64Type */
            &mut stream_id as *mut u64 as *mut std::ffi::c_void,
        ) == 0
        {
            log_message("FSEvents port: 'stream_id' is not a number");
            return std::ptr::null_mut();
        }

        // Get num_events
        let num_events_value = CFDictionaryGetValue(
            plist_raw,
            num_events_key.as_concrete_TypeRef() as *const std::ffi::c_void,
        );
        if num_events_value.is_null() {
            log_message("FSEvents port: missing 'num_events' field");
            return std::ptr::null_mut();
        }

        let mut num_events: i64 = 0;
        if CFNumberGetValue(
            num_events_value,
            10, /* kCFNumberLongType */
            &mut num_events as *mut i64 as *mut std::ffi::c_void,
        ) == 0
        {
            log_message("FSEvents port: 'num_events' is not a number");
            return std::ptr::null_mut();
        }

        // Get paths array
        let paths_value = CFDictionaryGetValue(
            plist_raw,
            paths_key.as_concrete_TypeRef() as *const std::ffi::c_void,
        );
        if paths_value.is_null() {
            log_message("FSEvents port: missing 'paths' field");
            return std::ptr::null_mut();
        }

        let array_type_id = CFArrayGetTypeID();
        let paths_type_id = CFGetTypeID(paths_value);
        if paths_type_id != array_type_id {
            log_message("FSEvents port: 'paths' is not an array");
            return std::ptr::null_mut();
        }

        let paths_array = paths_value as CFArrayRef;
        let paths_count = CFArrayGetCount(paths_array) as usize;
        if paths_count != num_events as usize {
            log_message(&format!(
                "FSEvents port: paths count ({}) != num_events ({})",
                paths_count, num_events
            ));
            return std::ptr::null_mut();
        }

        // Extract paths as retained CFStringRefs (RAII managed)
        let mut paths: Vec<OwnedCFRef> = Vec::new();
        let string_type_id = CFStringGetTypeID();
        for i in 0..paths_count {
            let item = CFArrayGetValueAtIndex(paths_array, i as CFIndex);
            if CFGetTypeID(item) == string_type_id {
                let cf_string = item as CFStringRef;
                // Retain the string since we'll release it after the callback
                CFRetain(cf_string);
                paths.push(OwnedCFRef::new(cf_string));
            } else {
                log_message(&format!("FSEvents port: paths[{}] is not a string", i));
                return std::ptr::null_mut();
            }
        }

        // Get flags array
        let flags_value = CFDictionaryGetValue(
            plist_raw,
            flags_key.as_concrete_TypeRef() as *const std::ffi::c_void,
        );
        if flags_value.is_null() {
            log_message("FSEvents port: missing 'flags' field");
            return std::ptr::null_mut();
        }

        let flags_type_id = CFGetTypeID(flags_value);
        if flags_type_id != array_type_id {
            log_message("FSEvents port: 'flags' is not an array");
            return std::ptr::null_mut();
        }

        let flags_array = flags_value as CFArrayRef;
        let flags_count = CFArrayGetCount(flags_array) as usize;
        if flags_count != num_events as usize {
            log_message(&format!(
                "FSEvents port: flags count ({}) != num_events ({})",
                flags_count, num_events
            ));
            return std::ptr::null_mut();
        }

        // Extract flags
        let mut flags: Vec<u32> = Vec::new();
        let number_type_id = CFNumberGetTypeID();
        for i in 0..flags_count {
            let item = CFArrayGetValueAtIndex(flags_array, i as CFIndex);
            if CFGetTypeID(item) == number_type_id {
                let mut flag: u32 = 0;
                if CFNumberGetValue(
                    item,
                    3, /* kCFNumberSInt32Type */
                    &mut flag as *mut u32 as *mut std::ffi::c_void,
                ) != 0
                {
                    flags.push(flag);
                } else {
                    log_message(&format!(
                        "FSEvents port: flags[{}] is not a valid number",
                        i
                    ));
                    return std::ptr::null_mut();
                }
            } else {
                log_message(&format!("FSEvents port: flags[{}] is not a number", i));
                return std::ptr::null_mut();
            }
        }

        // Get event_ids array
        let event_ids_value = CFDictionaryGetValue(
            plist_raw,
            event_ids_key.as_concrete_TypeRef() as *const std::ffi::c_void,
        );
        if event_ids_value.is_null() {
            log_message("FSEvents port: missing 'event_ids' field");
            return std::ptr::null_mut();
        }

        let event_ids_type_id = CFGetTypeID(event_ids_value);
        if event_ids_type_id != array_type_id {
            log_message("FSEvents port: 'event_ids' is not an array");
            return std::ptr::null_mut();
        }

        let event_ids_array = event_ids_value as CFArrayRef;
        let event_ids_count = CFArrayGetCount(event_ids_array) as usize;
        if event_ids_count != num_events as usize {
            log_message(&format!(
                "FSEvents port: event_ids count ({}) != num_events ({})",
                event_ids_count, num_events
            ));
            return std::ptr::null_mut();
        }

        // Extract event_ids
        let mut event_ids: Vec<u64> = Vec::new();
        for i in 0..event_ids_count {
            let item = CFArrayGetValueAtIndex(event_ids_array, i as CFIndex);
            if CFGetTypeID(item) == number_type_id {
                let mut event_id: u64 = 0;
                if CFNumberGetValue(
                    item,
                    4, /* kCFNumberSInt64Type */
                    &mut event_id as *mut u64 as *mut std::ffi::c_void,
                ) != 0
                {
                    event_ids.push(event_id);
                } else {
                    log_message(&format!(
                        "FSEvents port: event_ids[{}] is not a valid number",
                        i
                    ));
                    return std::ptr::null_mut();
                }
            } else {
                log_message(&format!("FSEvents port: event_ids[{}] is not a number", i));
                return std::ptr::null_mut();
            }
        }

        // plist will be cleaned up automatically by OwnedCFRef
        // CFString keys are now RAII and clean up automatically

        // Create CFArray from paths for the callback
        let cf_paths_array_wrapper = if !paths.is_empty() {
            // Extract raw pointers for CFArray creation
            let path_ptrs: Vec<CFStringRef> =
                paths.iter().map(|owned| owned.ptr.unwrap() as CFStringRef).collect();

            let array = CFArrayCreate(
                kCFAllocatorDefault,
                path_ptrs.as_ptr() as *const *const libc::c_void,
                path_ptrs.len() as CFIndex,
                std::ptr::null(),
            );
            if array.is_null() {
                log_message("Failed to create CFArray for paths");
                return std::ptr::null_mut();
            }
            Some(OwnedCFRef::new(array))
        } else {
            None
        };

        // Find the callback for this stream and deliver events
        let callbacks = FSEVENTS_CALLBACKS.lock().unwrap();
        if let Some(callback_info) = callbacks.values().find(|info| info.stream_id == stream_id) {
            if !paths.is_empty() {
                if let Some(wrapper) = cf_paths_array_wrapper.as_ref() {
                    log_message(&format!(
                        "Delivering {} events to stream {}",
                        paths.len(),
                        stream_id
                    ));

                    // Call the original FSEvents callback
                    if let Some(cf_paths_array) = wrapper.ptr {
                        (callback_info.original_callback)(
                            callback_info.stream_id as FSEventStreamRef,
                            callback_info.original_context as *mut libc::c_void,
                            paths.len(),
                            cf_paths_array,
                            flags.as_ptr(),
                            event_ids.as_ptr(),
                        );
                    } else {
                        log_message(&format!(
                            "Paths array wrapper missing CFArray ptr for stream {}",
                            stream_id
                        ));
                    }
                } else {
                    log_message(&format!(
                        "No CF paths array to deliver {} events for stream {}",
                        paths.len(),
                        stream_id
                    ));
                }
            } else {
                log_message(&format!(
                    "No valid events to deliver for stream {}",
                    stream_id
                ));
            }
        } else {
            log_message(&format!("No callback found for stream {}", stream_id));
        }

        // paths and cf_paths_array_wrapper will clean up automatically
        std::ptr::null_mut()
    }
}

/// Create the per-process CFMessagePort for FSEvents delivery
fn create_fsevents_message_port() -> Result<(), String> {
    let mut port_guard = FSEVENTS_MESSAGE_PORT.lock().unwrap();
    if port_guard.is_some() {
        return Ok(()); // Already created
    }

    let port_name = &*FSEVENTS_PORT_NAME;

    // Create CFString from Rust string with RAII
    let cf_string = core_foundation::string::CFString::new(port_name);

    // Create message port context
    let context = CFMessagePortContext {
        version: 0,
        info: std::ptr::null_mut(),
        retain: None,
        release: None,
        copy_description: None,
    };

    let mut should_free_info = 0u8;

    // Create the local message port
    let port = unsafe {
        CFMessagePortCreateLocal(
            kCFAllocatorDefault,
            cf_string.as_concrete_TypeRef() as CFStringRef,
            fsevents_message_port_callback,
            &context,
            &mut should_free_info,
        )
    };

    if port.is_null() {
        return Err("Failed to create CFMessagePort".to_string());
    }

    // Send port name to daemon via control socket
    send_port_name_to_daemon(port_name)?;

    *port_guard = Some(OwnedMessagePort::new(port));

    log_message(&format!("Created FSEvents message port: {}", port_name));
    Ok(())
}

/// Send the message port name to the daemon
fn send_port_name_to_daemon(port_name: &str) -> Result<(), String> {
    use agentfs_proto::messages::{Response, WatchRegisterFSEventsPortRequest};

    let request = Request::WatchRegisterFSEventsPort((
        vec![], // version
        WatchRegisterFSEventsPortRequest {
            pid: std::process::id(),
            port_name: port_name.as_bytes().to_vec(),
        },
    ));

    // Send via the existing control socket
    send_request(request, |response| match response {
        Response::WatchRegisterFSEventsPort(_) => Some(()),
        _ => None,
    })
    .map_err(|e| format!("Failed to send port registration: {}", e))
}

/// Add message port source to a run loop
fn add_port_to_run_loop(run_loop: CFRunLoopRef, mode: CFStringRef) -> Result<(), String> {
    let port_guard = FSEVENTS_MESSAGE_PORT.lock().unwrap();
    let port = match port_guard.as_ref() {
        Some(p) => p.inner.as_concrete_TypeRef(),
        None => return Err("Message port not created".to_string()),
    };

    let mut sources_guard = FSEVENTS_RUN_LOOP_SOURCES.lock().unwrap();

    // Check if we already have a source for this run loop/mode
    let key = CFRunLoopSourceKey(run_loop, mode);
    if sources_guard.contains_key(&key) {
        return Ok(()); // Already added
    }

    // Create run loop source for the port
    let source = unsafe { CFMessagePortCreateRunLoopSource(kCFAllocatorDefault, port, 0) };

    if source.is_null() {
        return Err("Failed to create CFRunLoopSource for message port".to_string());
    }

    // Ensure we remove the source from run loop if we fail later
    let _cleanup = guard((run_loop, source, mode), |(rl, src, md)| unsafe {
        CFRunLoopRemoveSource(rl, src, md);
        CFRelease(src);
    });

    // Add source to run loop
    unsafe {
        CFRunLoopAddSource(run_loop, source, mode);
    }

    sources_guard.insert(key, CFRunLoopSourceWrapper(source));

    // Source is now owned by the sources_guard, disable cleanup
    std::mem::forget(_cleanup);

    log_message("Added FSEvents message port to run loop");
    Ok(())
}

/// Execute a closure with access to the current process's dirfd mapping
fn with_dirfd_mapping<F, R>(f: F) -> R
where
    F: FnOnce(&mut DirfdMapping) -> R,
{
    let pid = std::process::id();
    let mut global_map = DIRFD_MAPPING.lock().unwrap();

    // Initialize mapping for this process if it doesn't exist
    global_map.entry(pid).or_insert_with(DirfdMapping::new);

    // Get mutable reference to the mapping and execute the closure
    let mapping = global_map.get_mut(&pid).unwrap();
    f(mapping)
}

/// Resolve a directory file descriptor to its path
fn resolve_dirfd(dirfd: RawFd) -> Option<PathBuf> {
    with_dirfd_mapping(|mapping| match dirfd {
        libc::AT_FDCWD => Some(mapping.cwd.clone()),
        fd if fd >= 0 => mapping.get_path(fd).cloned(),
        _ => None,
    })
}

/// Resolve a path by combining dirfd + relative path with symlink and .. handling
fn resolve_path_with_dirfd(dirfd: RawFd, path: &CStr) -> Option<PathBuf> {
    let base_path = resolve_dirfd(dirfd)?;
    let relative_path = Path::new(path.to_str().ok()?);

    // Combine base path with relative path
    let mut resolved_path = base_path.clone();
    resolved_path.push(relative_path);

    // Canonicalize the path to resolve . and .. components
    // Note: This follows symlinks, which is the expected behavior for most *at functions
    match resolved_path.canonicalize() {
        Ok(canonical) => Some(canonical),
        Err(_) => {
            // If canonicalization fails (e.g., path doesn't exist), return the non-canonicalized path
            // This allows operations on non-existent files to work correctly
            Some(resolved_path)
        }
    }
}

/// Helper function to resolve path for *at operations
fn resolve_at_path(dirfd: RawFd, path: &CStr) -> PathBuf {
    resolve_path_with_dirfd(dirfd, path).unwrap_or_else(|| {
        // Fallback: construct path manually if resolution fails
        resolve_dirfd(dirfd)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(path.to_str().unwrap_or(""))
    })
}

static INIT_GUARD: OnceCell<()> = OnceCell::new();
static STREAM: Mutex<Option<Arc<Mutex<UnixStream>>>> = Mutex::new(None);

// Global directory file descriptor mapping keyed by process ID
static DIRFD_MAPPING: std::sync::LazyLock<Mutex<HashMap<u32, DirfdMapping>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Watcher table for EVFILT_VNODE registrations: (kq_fd, fd) -> requested_fflags
static WATCHER_TABLE: std::sync::LazyLock<Mutex<HashMap<(c_int, u32), u32>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

// Track which file descriptors are kqueues (fd -> true)
static KQUEUE_FDS: std::sync::LazyLock<Mutex<HashSet<c_int>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashSet::new()));

// Track doorbell ident per kqueue (kq_fd -> doorbell_ident)
static DOORBELL_IDENTS: std::sync::LazyLock<Mutex<HashMap<c_int, u64>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Queue for FSEvents broadcasts received from daemon: stream_id -> Vec<FsEventBroadcastRequest>
static FSEVENTS_QUEUE: std::sync::LazyLock<Mutex<HashMap<u64, Vec<FsEventBroadcastRequest>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Per-process CFMessagePort for FSEvents delivery
static FSEVENTS_MESSAGE_PORT: Mutex<Option<OwnedMessagePort>> = Mutex::new(None);

/// Run-loop sources for the message port: (run_loop, mode) -> source
static FSEVENTS_RUN_LOOP_SOURCES: std::sync::LazyLock<
    Mutex<HashMap<CFRunLoopSourceKey, CFRunLoopSourceWrapper>>,
> = std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Message port name for this process
static FSEVENTS_PORT_NAME: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp =
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64;
    let pid = std::process::id();
    format!("com.agentfs.ipc.fsevents.{}.{:x}", pid, timestamp)
});

// Directory handles are now managed by FsCore directly

#[cfg(test)]
static ENV_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[cfg(test)]
fn set_env_var(key: &str, value: &str) {
    unsafe { std::env::set_var(key, value) };
}

#[cfg(test)]
fn remove_env_var(key: &str) {
    unsafe { std::env::remove_var(key) };
}

#[ctor::ctor]
fn initialize() {
    if INIT_GUARD.set(()).is_err() {
        return;
    }

    if !is_enabled() {
        return;
    }

    log_message(DEFAULT_BANNER);

    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            log_message(&format!("failed to resolve current executable: {err}"));
            PathBuf::from("<unknown>")
        }
    };

    let allow = AllowDecision::from_env(&exe);
    if !allow.allowed {
        let fail_fast = std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
        if fail_fast {
            log_message(&format!(
                "process '{}' not present in allowlist but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program",
                exe.display()
            ));
            std::process::exit(1);
        } else {
            log_message(&format!(
                "process '{}' not present in allowlist; skipping handshake",
                exe.display()
            ));
            return;
        }
    }

    let socket_env = std::env::var_os(ENV_SOCKET);
    log_message(&format!("{ENV_SOCKET} = {:?}", socket_env));
    let Some(socket_path) = socket_env.map(PathBuf::from) else {
        let fail_fast = std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
        if fail_fast {
            log_message(&format!(
                "{ENV_SOCKET} not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program"
            ));
            std::process::exit(1);
        } else {
            log_message(&format!("{ENV_SOCKET} not set; skipping handshake"));
            return;
        }
    };

    match attempt_handshake(&socket_path, &exe, &allow) {
        Ok(stream) => {
            let stream_arc = Arc::new(Mutex::new(stream));
            let mut stream_guard = STREAM.lock().unwrap();
            if stream_guard.is_none() {
                *stream_guard = Some(Arc::clone(&stream_arc));
                log_message("STREAM set successfully in ctor");
            } else {
                log_message("STREAM already set in ctor");
            }

            stackable_hooks::enable_hooks();
        }
        Err(err) => {
            log_message(&format!("handshake failed: {err}"));
            // Check if we should fail fast
            let fail_fast = std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
            if fail_fast {
                log_message(
                    "AGENTFS_INTERPOSE_FAIL_FAST=1 set, terminating program due to handshake failure",
                );
                std::process::exit(1);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
#[ctor::ctor]
fn initialize() {}

fn is_enabled() -> bool {
    match std::env::var(ENV_ENABLED) {
        Ok(value) => matches!(value.as_str(), "1" | "true" | "TRUE" | "True"),
        Err(std::env::VarError::NotPresent) => true,
        Err(err) => {
            log_message(&format!("unable to read {ENV_ENABLED}: {err}"));
            true
        }
    }
}

fn log_message(message: &str) {
    if std::env::var_os(ENV_LOG_LEVEL)
        .as_deref()
        .is_some_and(|v| matches!(v.to_str(), Some("0" | "false" | "FALSE" | "False")))
    {
        return;
    }
    let pid = std::process::id();
    // Avoid clippy::disallowed_methods (eprintln). Write directly to stderr.
    {
        use std::io::Write;
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "{LOG_PREFIX} [pid={pid}] {message}");
    }
}

#[no_mangle]
pub extern "C" fn agentfs_interpose_force_reconnect() {
    if let Err(err) = try_reconnect_to_daemon() {
        log_message(&format!(
            "agentfs_interpose_force_reconnect failed: {}",
            err
        ));
    }
}

#[derive(Debug)]
struct AllowDecision {
    allowed: bool,
    matched_entry: Option<String>,
    raw_entries: Option<Vec<String>>,
}

impl AllowDecision {
    fn from_env(exe_path: &Path) -> Self {
        let Some(raw) = std::env::var(ENV_ALLOWLIST).ok().filter(|s| !s.trim().is_empty()) else {
            return Self {
                allowed: true,
                matched_entry: None,
                raw_entries: None,
            };
        };

        let entries: Vec<String> =
            raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

        if entries.is_empty() {
            return Self {
                allowed: true,
                matched_entry: None,
                raw_entries: Some(entries),
            };
        }

        let exe_basename =
            exe_path.file_name().and_then(OsStr::to_str).unwrap_or("").to_ascii_lowercase();

        let exe_display = exe_path.display().to_string().to_ascii_lowercase();

        for entry in &entries {
            if entry == "*" {
                return Self {
                    allowed: true,
                    matched_entry: Some(entry.clone()),
                    raw_entries: Some(entries),
                };
            }
            let entry_lower = entry.to_ascii_lowercase();
            if exe_basename == entry_lower || exe_display.contains(&entry_lower) {
                return Self {
                    allowed: true,
                    matched_entry: Some(entry.clone()),
                    raw_entries: Some(entries),
                };
            }
        }

        Self {
            allowed: false,
            matched_entry: None,
            raw_entries: Some(entries),
        }
    }
}

// SSZ encoding/decoding functions for interpose communication
fn encode_ssz(data: &impl Encode) -> Vec<u8> {
    data.as_ssz_bytes()
}

fn decode_ssz<T: Decode>(data: &[u8]) -> Result<T, String> {
    T::from_ssz_bytes(data).map_err(|e| format!("SSZ decode error: {:?}", e))
}

fn read_exact_with_timeout(
    stream: &mut UnixStream,
    buf: &mut [u8],
    timeout: Duration,
) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    use std::os::fd::AsRawFd;
    use std::time::Instant;

    let fd = stream.as_raw_fd();
    let mut offset = 0;
    let deadline = Instant::now() + timeout;

    while offset < buf.len() {
        let now = Instant::now();
        if now >= deadline {
            return Err(Error::new(ErrorKind::TimedOut, "read timeout"));
        }

        let remaining_ms = (deadline - now).as_millis() as libc::c_int;
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };

        let poll_result = unsafe { libc::poll(&mut pfd, 1, remaining_ms) };
        if poll_result < 0 {
            return Err(Error::last_os_error());
        }
        if poll_result == 0 {
            return Err(Error::new(ErrorKind::TimedOut, "read timeout"));
        }

        let bytes_read = stream.read(&mut buf[offset..])?;
        if bytes_read == 0 {
            return Err(Error::new(ErrorKind::UnexpectedEof, "connection closed"));
        }
        offset += bytes_read;
    }

    Ok(())
}

fn write_all_with_timeout(
    stream: &mut UnixStream,
    buf: &[u8],
    timeout: Duration,
) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    use std::os::fd::AsRawFd;
    use std::time::Instant;

    let fd = stream.as_raw_fd();
    let mut offset = 0;
    let deadline = Instant::now() + timeout;

    while offset < buf.len() {
        let now = Instant::now();
        if now >= deadline {
            return Err(Error::new(ErrorKind::TimedOut, "write timeout"));
        }

        let remaining_ms = (deadline - now).as_millis() as libc::c_int;
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLOUT,
            revents: 0,
        };

        let poll_result = unsafe { libc::poll(&mut pfd, 1, remaining_ms) };
        if poll_result < 0 {
            return Err(Error::last_os_error());
        }
        if poll_result == 0 {
            return Err(Error::new(ErrorKind::TimedOut, "write timeout"));
        }

        let bytes_written = stream.write(&buf[offset..])?;
        if bytes_written == 0 {
            return Err(Error::new(
                ErrorKind::WriteZero,
                "failed to write to socket",
            ));
        }
        offset += bytes_written;
    }

    Ok(())
}

fn send_request<F, T>(request: Request, extract_response: F) -> Result<T, String>
where
    F: Fn(Response) -> Option<T>,
{
    let stream_arc = match STREAM.lock().unwrap().as_ref().map(Arc::clone) {
        Some(arc) => arc,
        None => {
            let fail_fast = std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
            if fail_fast {
                log_message(
                    "STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program",
                );
                std::process::exit(1);
            } else {
                log_message("STREAM not set, falling back to original function");
                return Err("not connected to AgentFS control socket".to_string());
            }
        }
    };

    let ssz_bytes = encode_ssz(&request);
    let ssz_len = ssz_bytes.len() as u32;

    // Send request
    {
        let mut stream_guard = stream_arc.lock().unwrap();
        write_all_with_timeout(
            &mut stream_guard,
            &ssz_len.to_le_bytes(),
            Duration::from_millis(50),
        )
        .map_err(|e| format!("send request failed: {}", e))?;
        write_all_with_timeout(&mut stream_guard, &ssz_bytes, Duration::from_millis(200))
            .map_err(|e| format!("send request failed: {}", e))?;
    }

    // Read response
    let mut len_buf = [0u8; 4];
    let mut msg_buf: Vec<u8>;
    {
        let mut stream_guard = stream_arc.lock().unwrap();
        read_exact_with_timeout(&mut stream_guard, &mut len_buf, Duration::from_millis(50))
            .map_err(|e| format!("read response length failed: {}", e))?;

        let msg_len = u32::from_le_bytes(len_buf) as usize;
        msg_buf = vec![0u8; msg_len];

        read_exact_with_timeout(&mut stream_guard, &mut msg_buf, Duration::from_millis(200))
            .map_err(|e| format!("read response failed: {}", e))?;
    }

    // Decode the response
    match decode_ssz::<Response>(&msg_buf) {
        Ok(response) => match extract_response(response) {
            Some(result) => Ok(result),
            None => Err("unexpected response type".to_string()),
        },
        Err(e) => Err(format!("decode response failed: {}", e)),
    }
}

/// Try to reconnect to the daemon and re-register all watches
fn try_reconnect_to_daemon() -> Result<(), String> {
    log_message("Attempting to reconnect to AgentFS daemon");

    // Clear existing stream
    {
        let mut stream_guard = STREAM.lock().unwrap();
        *stream_guard = None;
    }

    // Try to connect using environment variables
    let socket_path_env = std::env::var_os("AGENTFS_INTERPOSE_SOCKET");
    let exe_env = std::env::var_os("AGENTFS_INTERPOSE_EXE");

    match (socket_path_env, exe_env) {
        (Some(socket_path_str), Some(exe_str)) => {
            let socket_path = PathBuf::from(socket_path_str);
            let exe = PathBuf::from(exe_str);

            // Attempt handshake
            let allow = AllowDecision::from_env(&exe); // Assume we were previously allowed
            match attempt_handshake(&socket_path, &exe, &allow) {
                Ok(stream) => {
                    let stream_arc = Arc::new(Mutex::new(stream));
                    {
                        let mut stream_guard = STREAM.lock().unwrap();
                        *stream_guard = Some(Arc::clone(&stream_arc));
                    }

                    log_message("Successfully reconnected to daemon");

                    // Re-register all active watches
                    re_register_all_watches()?;

                    Ok(())
                }
                Err(e) => Err(format!("handshake failed: {}", e)),
            }
        }
        _ => Err(
            "missing AGENTFS_INTERPOSE_SOCKET or AGENTFS_INTERPOSE_EXE environment variables"
                .to_string(),
        ),
    }
}

/// Re-register all active watches with the daemon
fn re_register_all_watches() -> Result<(), String> {
    log_message("Re-registering all active watches");

    let pid = std::process::id();

    // Re-register kqueue watches
    {
        let watcher_table = WATCHER_TABLE.lock().unwrap();
        for ((kq_fd, fd), fflags) in watcher_table.iter() {
            let request = Request::watch_register_kqueue(
                pid,
                *kq_fd as u32,
                0, // watch_id - daemon will assign
                *fd,
                *fflags,
            );

            send_request(request, |_| None)?;
            log_message(&format!(
                "Re-registered watch for fd {} on kqueue {}",
                fd, kq_fd
            ));
        }
    }

    // Re-register doorbell for active kqueues
    {
        let doorbell_idents = DOORBELL_IDENTS.lock().unwrap();
        for (kq_fd, doorbell_ident) in doorbell_idents.iter() {
            let request = Request::watch_doorbell(pid, *kq_fd as u32, *doorbell_ident);
            send_request(request, |_| None)?;
            log_message(&format!("Re-registered doorbell for kqueue {}", kq_fd));
        }
    }

    // Re-register FSEvents streams - this is more complex as we need callback info
    // For now, we'll skip this as it's less critical for basic functionality

    log_message("Finished re-registering watches");
    Ok(())
}

fn attempt_handshake(
    socket_path: &Path,
    exe: &Path,
    allow: &AllowDecision,
) -> Result<UnixStream, String> {
    let socket_display = socket_path.display();
    let pid = std::process::id();
    let ppid = unsafe { libc::getppid() as u32 };
    let uid = unsafe { libc::geteuid() as u32 };
    let gid = unsafe { libc::getegid() as u32 };
    let exe_name = exe.file_name().and_then(OsStr::to_str).unwrap_or("<unknown>");
    let exe_path_owned = exe.to_string_lossy().into_owned();
    let exe_name_owned = exe_name.to_string();

    let process_config = ProcessConfig::new(
        pid,
        ppid,
        uid,
        gid,
        exe_path_owned.clone(),
        exe_name_owned.clone(),
    );

    let mut builder = ClientConfig::builder("agentfs-interpose-shim", env!("CARGO_PKG_VERSION"))
        .feature("handshake")
        .feature("allowlist")
        .process(process_config)
        .read_timeout(Duration::from_secs(2))
        .write_timeout(Duration::from_secs(2));

    if allow.matched_entry.is_some() || allow.raw_entries.is_some() {
        builder = builder.allowlist(AllowlistConfig::new(
            allow.matched_entry.clone(),
            allow.raw_entries.clone(),
        ));
    }

    let config = builder
        .build()
        .map_err(|err| format!("failed to build AgentFS client configuration: {err}"))?;

    let mut client = AgentFsClient::connect(socket_path, &config).map_err(|err| {
        format!(
            "failed to connect to AgentFS control socket '{}': {}",
            socket_display, err
        )
    })?;

    let acknowledgement = String::from_utf8_lossy(client.handshake_ack()).into_owned();
    log_message(&format!(
        "handshake acknowledged by {socket_display}: {acknowledgement}"
    ));

    // Bind the current process to the branch specified in the environment
    if let Some(branch_id_str) = std::env::var_os(ENV_BRANCH_ID) {
        if let Some(branch_id_str) = branch_id_str.to_str() {
            match client.branch_bind(branch_id_str, None) {
                Ok(()) => {
                    log_message(&format!(
                        "bound current process to branch {} via AgentFS control socket: {}",
                        branch_id_str, socket_display
                    ));
                }
                Err(err) => {
                    log_message(&format!(
                        "failed to bind current process to branch {}: {}",
                        branch_id_str, err
                    ));
                    // Don't fail the handshake if branch binding fails
                }
            }
        }
    }

    log_message(&format!(
        "handshake completed with AgentFS control socket: {}",
        socket_display
    ));

    Ok(client.into_stream())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_allows_when_not_set() {
        let _lock = ENV_GUARD.lock().unwrap();
        remove_env_var(ENV_ALLOWLIST);
        let exe = Path::new("/Applications/MyApp.app/Contents/MacOS/MyApp");
        let decision = AllowDecision::from_env(exe);
        assert!(decision.allowed);
        assert!(decision.matched_entry.is_none());
    }

    #[test]
    fn allowlist_matches_basename() {
        let _lock = ENV_GUARD.lock().unwrap();
        set_env_var(ENV_ALLOWLIST, "MyApp,OtherApp");
        let exe = Path::new("/Applications/MyApp.app/Contents/MacOS/MyApp");
        let decision = AllowDecision::from_env(exe);
        assert!(decision.allowed);
        assert_eq!(decision.matched_entry.as_deref(), Some("MyApp"));
    }

    #[test]
    fn allowlist_rejects_non_match() {
        let _lock = ENV_GUARD.lock().unwrap();
        set_env_var(ENV_ALLOWLIST, "OtherApp");
        let exe = Path::new("/Applications/MyApp.app/Contents/MacOS/MyApp");
        let decision = AllowDecision::from_env(exe);
        assert!(!decision.allowed);
        assert!(decision.matched_entry.is_none());
    }

    #[test]

    fn fsevents_translate_paths_stub_implementation() {
        // Test that the translate_fsevent_paths function returns an empty vec
        // indicating that CoreFoundation array manipulation is not yet implemented
        let dummy_cfarray = std::ptr::null_mut();
        let result = translate_fsevent_paths(dummy_cfarray);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]

    fn fsevents_hooks_are_defined() {
        // Test that the FSEvents hooks are compiled and available
        // This is a compile-time test - if the hooks don't exist, this won't compile

        // Note: We can't easily test the actual hook behavior without mocking
        // the CoreFoundation APIs, but we can verify the hook functions are defined
        // by checking that the module compiles successfully with the hooks present.
        // Placeholder removed (clippy::assertions_on_constants). The compile-time presence of hooks is validated by successful compilation of this test.
    }
}

// Interposition implementation for file operations
mod interpose {
    use super::*;
    use libc::{
        CMSG_DATA, CMSG_FIRSTHDR, CMSG_LEN, CMSG_SPACE, SCM_RIGHTS, SOL_SOCKET, c_char, c_int,
        c_uint, c_void, gid_t, iovec, mode_t, msghdr, off_t, size_t, ssize_t, timespec, uid_t,
    };

    // Re-export kqueue types from parent scope for use in this module
    use super::EVFILT_VNODE;

    // ACL types - these may need to be defined manually for macOS
    #[allow(non_camel_case_types)]
    type acl_type_t = u32;
    #[allow(non_camel_case_types)]
    type acl_t = *mut c_void;

    // attrlist types for getattrlist operations
    #[repr(C)]
    #[allow(non_camel_case_types)] // Preserve C struct name required for FFI correctness
    struct attrlist {
        bitmapcount: u16,
        reserved: u16,
        commonattr: u32,
        volattr: u32,
        dirattr: u32,
        fileattr: u32,
        forkattr: u32,
    }

    // copyfile types for copyfile operations
    #[allow(non_camel_case_types)]
    type copyfile_state_t = *mut c_void;
    #[allow(non_camel_case_types)]
    type copyfile_flags_t = u32;

    // Additional type aliases for macOS
    #[allow(non_camel_case_types)]
    type u_long = usize;
    #[allow(non_camel_case_types)]
    type u_int64_t = u64;
    use std::io::Read;
    use std::mem;

    /// Generic function to send a request and receive a response
    /// Send dir_open request and receive directory handle
    fn send_dir_open_request(path: &CStr) -> Result<u64, String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            log_message(&format!(
                "STREAM.lock() returned: {:?}",
                stream_guard.as_ref().map(|_| "Some(arc)")
            ));
            match stream_guard.as_ref() {
                Some(arc) => {
                    log_message("STREAM found, sending dir_open request");
                    Arc::clone(arc)
                }
                None => {
                    let fail_fast =
                        std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(
                            "STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program",
                        );
                        std::process::exit(1);
                    } else {
                        log_message("STREAM not set, falling back to original opendir");
                        return Err("not connected to AgentFS control socket".to_string());
                    }
                }
            }
        };

        let path_str = path.to_string_lossy().into_owned();
        let request = Request::dir_open(path_str);

        let ssz_bytes = encode_ssz(&request);
        let ssz_len = ssz_bytes.len() as u32;

        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .write_all(&ssz_len.to_le_bytes())
                .and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send dir_open request: {e}"))?;
        }

        // Read response
        let mut len_buf = [0u8; 4];
        let mut msg_buf: Vec<u8>;
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .read_exact(&mut len_buf)
                .map_err(|e| format!("read response length: {e}"))?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            msg_buf = vec![0u8; msg_len];
            stream_guard
                .read_exact(&mut msg_buf)
                .map_err(|e| format!("read response: {e}"))?;
        }

        // Decode the response
        let dir_handle = match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => match response {
                Response::DirOpen(dir_response) => {
                    let handle = dir_response.handle;
                    log_message(&format!("received dir handle {}", handle));
                    handle
                }
                Response::Error(err) => {
                    return Err(format!(
                        "daemon error: {}",
                        String::from_utf8_lossy(&err.error)
                    ));
                }
                _ => {
                    return Err("unexpected response type".to_string());
                }
            },
            Err(e) => {
                return Err(format!("decode response failed: {:?}", e));
            }
        };

        Ok(dir_handle)
    }

    /// Send dir_read request and receive directory entries
    fn send_dir_read_request(handle: u64) -> Result<Vec<DirEntry>, String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            match stream_guard.as_ref() {
                Some(arc) => Arc::clone(arc),
                None => {
                    let fail_fast =
                        std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(
                            "STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program",
                        );
                        std::process::exit(1);
                    } else {
                        log_message("STREAM not set, falling back to original readdir");
                        return Err("not connected to AgentFS control socket".to_string());
                    }
                }
            }
        };

        let request = Request::dir_read(handle);
        let ssz_bytes = encode_ssz(&request);
        let ssz_len = ssz_bytes.len() as u32;

        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .write_all(&ssz_len.to_le_bytes())
                .and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send dir_read request: {e}"))?;
        }

        // Read response
        let mut len_buf = [0u8; 4];
        let mut msg_buf: Vec<u8>;
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .read_exact(&mut len_buf)
                .map_err(|e| format!("read dir_read response length: {e}"))?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            msg_buf = vec![0u8; msg_len];
            stream_guard
                .read_exact(&mut msg_buf)
                .map_err(|e| format!("read dir_read response: {e}"))?;
        }

        // Decode the response
        let entries = match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => match response {
                Response::DirRead(dir_read_response) => {
                    log_message(&format!(
                        "received {} directory entries",
                        dir_read_response.entries.len()
                    ));
                    dir_read_response.entries
                }
                Response::Error(err) => {
                    return Err(format!(
                        "daemon error: {}",
                        String::from_utf8_lossy(&err.error)
                    ));
                }
                _ => {
                    return Err("unexpected response type".to_string());
                }
            },
            Err(e) => {
                return Err(format!("decode response failed: {:?}", e));
            }
        };

        Ok(entries)
    }

    /// Send dir_close request
    fn send_dir_close_request(handle: u64) -> Result<(), String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            match stream_guard.as_ref() {
                Some(arc) => Arc::clone(arc),
                None => {
                    let fail_fast =
                        std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(
                            "STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program",
                        );
                        std::process::exit(1);
                    } else {
                        log_message("STREAM not set, falling back to original closedir");
                        return Err("not connected to AgentFS control socket".to_string());
                    }
                }
            }
        };

        let request = Request::dir_close(handle);
        let ssz_bytes = encode_ssz(&request);
        let ssz_len = ssz_bytes.len() as u32;

        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .write_all(&ssz_len.to_le_bytes())
                .and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send dir_close request: {e}"))?;
        }

        // Read response
        let mut len_buf = [0u8; 4];
        let mut msg_buf: Vec<u8>;
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .read_exact(&mut len_buf)
                .map_err(|e| format!("read dir_close response length: {e}"))?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            msg_buf = vec![0u8; msg_len];
            stream_guard
                .read_exact(&mut msg_buf)
                .map_err(|e| format!("read dir_close response: {e}"))?;
        }

        // Decode the response
        match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => match response {
                Response::DirClose(_) => {
                    log_message("received dir_close response");
                    Ok(())
                }
                Response::Error(err) => Err(format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                )),
                _ => Err("unexpected response type".to_string()),
            },
            Err(e) => Err(format!("decode response failed: {:?}", e)),
        }
    }

    /// Send readlink request and receive link target
    fn send_readlink_request(path: &CStr) -> Result<String, String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            log_message(&format!(
                "STREAM.lock() returned: {:?}",
                stream_guard.as_ref().map(|_| "Some(arc)")
            ));
            match stream_guard.as_ref() {
                Some(arc) => {
                    log_message("STREAM found, sending readlink request");
                    Arc::clone(arc)
                }
                None => {
                    let fail_fast =
                        std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(
                            "STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program",
                        );
                        std::process::exit(1);
                    } else {
                        log_message("STREAM not set, falling back to original readlink");
                        return Err("not connected to AgentFS control socket".to_string());
                    }
                }
            }
        };

        let path_str = path.to_string_lossy().into_owned();
        let request = Request::readlink(path_str);

        let ssz_bytes = encode_ssz(&request);
        let ssz_len = ssz_bytes.len() as u32;

        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .write_all(&ssz_len.to_le_bytes())
                .and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send readlink request: {e}"))?;
        }

        // Read response
        let mut len_buf = [0u8; 4];
        let mut msg_buf: Vec<u8>;
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .read_exact(&mut len_buf)
                .map_err(|e| format!("read response length: {e}"))?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            msg_buf = vec![0u8; msg_len];
            stream_guard
                .read_exact(&mut msg_buf)
                .map_err(|e| format!("read response: {e}"))?;
        }

        // Decode the response
        let link_target = match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => match response {
                Response::Readlink(readlink_response) => {
                    let target = String::from_utf8_lossy(&readlink_response.target).into_owned();
                    log_message(&format!("received link target '{}'", target));
                    target
                }
                Response::Error(err) => {
                    return Err(format!(
                        "daemon error: {}",
                        String::from_utf8_lossy(&err.error)
                    ));
                }
                _ => {
                    return Err("unexpected response type".to_string());
                }
            },
            Err(e) => {
                return Err(format!("decode response failed: {:?}", e));
            }
        };

        Ok(link_target)
    }

    /// Send fd_open request and receive file descriptor via SCM_RIGHTS
    fn send_fd_open_request(path: &CStr, flags: c_int, mode: mode_t) -> Result<RawFd, String> {
        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            log_message(&format!(
                "STREAM.lock() returned: {:?}",
                stream_guard.as_ref().map(|_| "Some(arc)")
            ));
            match stream_guard.as_ref() {
                Some(arc) => {
                    log_message("STREAM found, sending fd_open request");
                    Arc::clone(arc)
                }
                None => {
                    let fail_fast =
                        std::env::var_os(ENV_FAIL_FAST).map(|s| s == "1").unwrap_or(false);
                    if fail_fast {
                        log_message(
                            "STREAM not set but AGENTFS_INTERPOSE_FAIL_FAST=1; terminating program",
                        );
                        std::process::exit(1);
                    } else {
                        log_message("STREAM not set, falling back to original open");
                        return Err("not connected to AgentFS control socket".to_string());
                    }
                }
            }
        };

        let path_str = path.to_string_lossy().into_owned();
        let request = Request::fd_open(path_str, flags as u32, mode as u32);

        let ssz_bytes = encode_ssz(&request);
        let ssz_len = ssz_bytes.len() as u32;

        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .write_all(&ssz_len.to_le_bytes())
                .and_then(|_| stream_guard.write_all(&ssz_bytes))
                .map_err(|e| format!("send fd_open request: {e}"))?;
        }

        // Read response (for now, simple response with fd number)
        // TODO: Implement proper SCM_RIGHTS
        let mut len_buf = [0u8; 4];
        let mut msg_buf: Vec<u8>;
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .read_exact(&mut len_buf)
                .map_err(|e| format!("read response length: {e}"))?;
            let msg_len = u32::from_le_bytes(len_buf) as usize;
            msg_buf = vec![0u8; msg_len];
            stream_guard
                .read_exact(&mut msg_buf)
                .map_err(|e| format!("read response: {e}"))?;
        }

        // Decode the response
        let fd = match decode_ssz::<Response>(&msg_buf) {
            Ok(response) => match response {
                Response::FdOpen(fd_response) => {
                    let fd = fd_response.fd as RawFd;
                    log_message(&format!("received fd {} from daemon", fd));
                    fd
                }
                Response::Error(err) => {
                    return Err(format!(
                        "daemon error: {}",
                        String::from_utf8_lossy(&err.error)
                    ));
                }
                _ => {
                    return Err("unexpected response type".to_string());
                }
            },
            Err(e) => {
                return Err(format!("decode response failed: {:?}", e));
            }
        };

        if fd < 0 {
            return Err("invalid file descriptor received".to_string());
        }

        // Duplicate the file descriptor to avoid issues with the received fd
        let dup_fd = unsafe { libc::dup(fd) };
        if dup_fd < 0 {
            return Err(format!("dup failed: {}", std::io::Error::last_os_error()));
        }

        Ok(dup_fd as RawFd)
    }

    /// Send file descriptor via SCM_RIGHTS (for testing/debugging)
    #[allow(dead_code)]
    fn send_fd_response(stream: &UnixStream, fd: RawFd) -> Result<(), String> {
        let response = Response::fd_open(fd as u32);

        let ssz_bytes = encode_ssz(&response);
        let ssz_len = ssz_bytes.len() as u32;

        // Send response with file descriptor via SCM_RIGHTS
        let len_bytes = ssz_len.to_le_bytes();
        let mut iov = iovec {
            iov_base: len_bytes.as_ptr() as *mut libc::c_void,
            iov_len: 4,
        };

        let mut msg: msghdr = unsafe { mem::zeroed() };
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;

        let cmsg_space = unsafe { CMSG_SPACE(mem::size_of::<RawFd>() as libc::c_uint) } as usize;
        let mut cmsg_buf = vec![0u8; cmsg_space];
        msg.msg_control = cmsg_buf.as_mut_ptr() as *mut libc::c_void;
        msg.msg_controllen = cmsg_space as libc::c_uint;

        let cmsg = unsafe { CMSG_FIRSTHDR(&msg) };
        if cmsg.is_null() {
            return Err("failed to get control message header".to_string());
        }

        unsafe {
            (*cmsg).cmsg_len = CMSG_LEN(mem::size_of::<RawFd>() as libc::c_uint);
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            *(CMSG_DATA(cmsg) as *mut RawFd) = fd;
        }

        let result = unsafe { libc::sendmsg(stream.as_raw_fd(), &msg, 0) };
        if result < 0 {
            return Err(format!(
                "sendmsg failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        Ok(())
    }

    /// Interposed open function (fd_open + fd tracking)
    stackable_hooks::hook! {
        unsafe fn open(stackable_self, path: *const c_char, flags: c_int, mode: mode_t) -> c_int => my_open {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing open({}, {:#x}, {:#o})", c_path.to_string_lossy(), flags, mode));

            // First try fd_open request
            match send_fd_open_request(c_path, flags, mode) {
                Ok(fd) => {
                    // Track directory fd if needed
                    if (flags & libc::O_DIRECTORY) != 0 {
                        if let Ok(path_str) = c_path.to_str() {
                            with_dirfd_mapping(|mapping| {
                                let path_buf = PathBuf::from(path_str);
                                let canonical_path = path_buf.canonicalize().unwrap_or(path_buf);
                                mapping.set_path(fd, canonical_path);
                                log_message(&format!("tracked directory fd {} -> {}", fd, path_str));
                            });
                        }
                    }
                    log_message(&format!("fd_open succeeded, returning fd {}", fd));
                    fd as c_int
                }
                Err(err) => {
                    log_message(&format!("fd_open failed: {}, falling back to original", err));
                    // Fall back to original function and track if it's a directory
                    let result = stackable_hooks::call_next!(stackable_self, open, path, flags, mode);
                    log_message(&format!(
                        "fallback open({}) returned {}",
                        c_path.to_string_lossy(),
                        result
                    ));
                    if result < 0 {
                        log_message(&format!(
                            "fallback errno after open({}): {}",
                            c_path.to_string_lossy(),
                            std::io::Error::last_os_error()
                        ));
                    }
                    if result >= 0 && (flags & libc::O_DIRECTORY) != 0 {
                        if let Ok(path_str) = c_path.to_str() {
                            with_dirfd_mapping(|mapping| {
                                let path_buf = PathBuf::from(path_str);
                                let canonical_path = path_buf.canonicalize().unwrap_or(path_buf);
                                mapping.set_path(result, canonical_path);
                                log_message(&format!("tracked directory fd {} -> {}", result, path_str));
                            });
                        }
                    }
                    result
                }
            }
        }
    }

    /// Interposed openat function
    stackable_hooks::hook! {
        unsafe fn openat(stackable_self, dirfd: c_int, path: *const c_char, flags: c_int, mode: mode_t) -> c_int => my_openat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing openat({}, {}, {:#x}, {:#o})", dirfd, c_path.to_string_lossy(), flags, mode));

            // For now, fall back to original - openat forwarding needs more complex path resolution
            stackable_hooks::call_next!(stackable_self, openat, dirfd, path, flags, mode)
        }
    }

    /// Interposed creat function
    stackable_hooks::hook! {
        unsafe fn creat(stackable_self, path: *const c_char, mode: mode_t) -> c_int => my_creat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let flags = libc::O_CREAT | libc::O_TRUNC | libc::O_WRONLY;

            log_message(&format!("interposing creat({}, {:#o})", c_path.to_string_lossy(), mode));

            match send_fd_open_request(c_path, flags, mode) {
                Ok(fd) => {
                    log_message(&format!("fd_open succeeded, returning fd {}", fd));
                    fd as c_int
                }
                Err(err) => {
                    log_message(&format!("fd_open failed: {}, falling back to original", err));
                    stackable_hooks::call_next!(stackable_self, creat, path, mode)
                }
            }
        }
    }

    /// Interposed fopen function
    stackable_hooks::hook! {
        unsafe fn fopen(stackable_self, filename: *const c_char, mode: *const c_char) -> *mut libc::FILE => my_fopen {
            log_message("interposing fopen() - not yet implemented, falling back to original");

            // For now, fall back to original
            stackable_hooks::call_next!(stackable_self, fopen, filename, mode)
        }
    }

    /// Interposed freopen function
    stackable_hooks::hook! {
        unsafe fn freopen(stackable_self, filename: *const c_char, mode: *const c_char, stream: *mut libc::FILE) -> *mut libc::FILE => my_freopen {
            log_message("interposing freopen() - not yet implemented, falling back to original");

            // For now, fall back to original
            stackable_hooks::call_next!(stackable_self, freopen, filename, mode, stream)
        }
    }

    /// Interposed opendir function
    stackable_hooks::hook! {
        unsafe fn opendir(stackable_self, dirname: *const c_char) -> *mut libc::DIR => my_opendir {
            if dirname.is_null() {
                return std::ptr::null_mut();
            }

            let c_path = unsafe { CStr::from_ptr(dirname) };
            log_message(&format!("interposing opendir({})", c_path.to_string_lossy()));

            // Try to create directory handle with AgentFS entries
            match send_dir_open_request(c_path) {
                Ok(fscore_handle) => {
                    log_message(&format!("FsCore returned directory handle {}", fscore_handle));
                    // Return the FsCore handle directly as a DIR pointer
                    // This is safe because we're intercepting all directory operations
                    let dir_ptr = fscore_handle as *mut libc::DIR;
                    log_message(&format!("returning FsCore DIR pointer: {:?}", dir_ptr));
                    dir_ptr
                }
                Err(err) => {
                    log_message(&format!("failed to create AgentFS directory handle: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, opendir, dirname)
                }
            }
        }
    }

    /// Interposed fdopendir function
    stackable_hooks::hook! {
        unsafe fn fdopendir(stackable_self, fd: c_int) -> *mut libc::DIR => my_fdopendir {
            log_message(&format!("interposing fdopendir({}) - not yet implemented, falling back to original", fd));

            // For now, fall back to original
            stackable_hooks::call_next!(stackable_self, fdopendir, fd)
        }
    }

    /// Interposed readdir function
    stackable_hooks::hook! {
        unsafe fn readdir(stackable_self, dirp: *mut libc::DIR) -> *mut libc::dirent => my_readdir {
            log_message(&format!("interposing readdir({:?})", dirp));

            // The DIR pointer contains the FsCore handle ID
            let fscore_handle = dirp as u64;

            // Send DirRead request to get the next entry
            match send_dir_read_request(fscore_handle) {
                Ok(entries) => {
                    if entries.is_empty() {
                        log_message("readdir: reached end of directory");
                        return std::ptr::null_mut(); // End of directory
                    }

                    let entry = &entries[0];
                    log_message(&format!("readdir: returning entry {}", String::from_utf8_lossy(&entry.name)));

                    // For now, return null since we don't have proper dirent allocation
                    // In a full implementation, we'd allocate a libc::dirent and fill it
                    std::ptr::null_mut()
                }
                Err(err) => {
                    log_message(&format!("dir_read failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, readdir, dirp)
                }
            }
        }
    }

    /// Interposed closedir function
    stackable_hooks::hook! {
        unsafe fn closedir(stackable_self, dirp: *mut libc::DIR) -> c_int => my_closedir {
            log_message(&format!("interposing closedir({:?})", dirp));

            // The DIR pointer contains the FsCore handle ID
            let fscore_handle = dirp as u64;

            // Send DirClose request to FsCore
            match send_dir_close_request(fscore_handle) {
                Ok(()) => {
                    log_message(&format!("FsCore closedir succeeded for handle {}", fscore_handle));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("dir_close failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, closedir, dirp)
                }
            }
        }
    }

    /// Interposed readlink function
    stackable_hooks::hook! {
        unsafe fn readlink(stackable_self, pathname: *const c_char, buf: *mut c_char, bufsiz: libc::size_t) -> libc::ssize_t => my_readlink {
            if pathname.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(pathname) };
            log_message(&format!("interposing readlink({}, bufsiz={})", c_path.to_string_lossy(), bufsiz));

            match send_readlink_request(c_path) {
                Ok(link_target) => {
                    // Copy the link target to the provided buffer
                    let target_bytes = link_target.as_bytes();
                    let copy_len = std::cmp::min(target_bytes.len(), bufsiz);
                    if !buf.is_null() && copy_len > 0 {
                        unsafe {
                            std::ptr::copy_nonoverlapping(target_bytes.as_ptr() as *const c_char, buf, copy_len);
                        }
                    }
                    log_message(&format!("readlink succeeded, returning {} bytes", copy_len));
                    copy_len as libc::ssize_t
                }
                Err(err) => {
                    log_message(&format!("readlink failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, readlink, pathname, buf, bufsiz)
                }
            }
        }
    }

    /// Interposed readlinkat function
    stackable_hooks::hook! {
        unsafe fn readlinkat(stackable_self, dirfd: c_int, pathname: *const c_char, buf: *mut c_char, bufsiz: libc::size_t) -> libc::ssize_t => my_readlinkat {
            if pathname.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(pathname) };
            log_message(&format!("interposing readlinkat({}, {}, bufsiz={}) - not yet implemented, falling back to original", dirfd, c_path.to_string_lossy(), bufsiz));

            // For now, fall back to original
            stackable_hooks::call_next!(stackable_self, readlinkat, dirfd, pathname, buf, bufsiz)
        }
    }

    // Note: _INODE64 variants are not implemented as they require symbol names with '$'
    // which is not valid in Rust function names. The base functions handle the common case.

    /// Send stat request and receive file status
    fn send_stat_request(path: &CStr) -> Result<StatData, String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::stat(path_str);

        send_request(request, |response| match response {
            Response::Stat(stat_response) => {
                log_message("received stat response");
                Some(stat_response.stat)
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send lstat request and receive file status (without following symlinks)
    fn send_lstat_request(path: &CStr) -> Result<StatData, String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::lstat(path_str);

        send_request(request, |response| match response {
            Response::Lstat(lstat_response) => {
                log_message("received lstat response");
                Some(lstat_response.stat)
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fstat request and receive file status for open file descriptor
    fn send_fstat_request(fd: c_int) -> Result<StatData, String> {
        let request = Request::fstat(fd as u32);

        send_request(request, |response| match response {
            Response::Fstat(fstat_response) => {
                log_message(&format!("received fstat response for fd {}", fd));
                Some(fstat_response.stat)
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fstatat request and receive file status relative to directory fd
    fn send_fstatat_request(dirfd: c_int, path: &CStr, flags: c_int) -> Result<StatData, String> {
        let resolved_path = resolve_at_path(dirfd, path);
        let path_str = resolved_path.to_string_lossy().into_owned();
        let request = Request::fstatat(path_str.clone(), flags as u32);

        send_request(request, |response| match response {
            Response::Fstatat(fstatat_response) => {
                log_message(&format!(
                    "received fstatat response for resolved path {}",
                    path_str
                ));
                Some(fstatat_response.stat)
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send chown request to change file owner and group
    fn send_chown_request(path: &CStr, uid: uid_t, gid: gid_t) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::chown(path_str, uid, gid);

        send_request(request, |response| match response {
            Response::Chown(_) => {
                log_message("received chown response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send lchown request to change file owner and group (without following symlinks)
    fn send_lchown_request(path: &CStr, uid: uid_t, gid: gid_t) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::lchown(path_str, uid, gid);

        send_request(request, |response| match response {
            Response::Lchown(_) => {
                log_message("received lchown response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fchown request to change file owner and group by file descriptor
    fn send_fchown_request(fd: c_int, uid: uid_t, gid: gid_t) -> Result<(), String> {
        let request = Request::fchown(fd as u32, uid, gid);

        send_request(request, |response| match response {
            Response::Fchown(_) => {
                log_message(&format!("received fchown response for fd {}", fd));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fchownat request to change file owner and group relative to directory fd
    fn send_fchownat_request(
        dirfd: c_int,
        path: &CStr,
        uid: uid_t,
        gid: gid_t,
        flags: c_int,
    ) -> Result<(), String> {
        let resolved_path = resolve_at_path(dirfd, path);
        let path_str = resolved_path.to_string_lossy().into_owned();
        let request = Request::fchownat(path_str.clone(), uid, gid, flags as u32);

        send_request(request, |response| match response {
            Response::Fchownat(_) => {
                log_message(&format!(
                    "received fchownat response for resolved path {}",
                    path_str
                ));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send chmod request to change file mode
    fn send_chmod_request(path: &CStr, mode: mode_t) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::chmod(path_str, mode as u32);

        send_request(request, |response| match response {
            Response::Chmod(_) => {
                log_message("received chmod response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fchmod request to change file mode by file descriptor
    fn send_fchmod_request(fd: c_int, mode: mode_t) -> Result<(), String> {
        let request = Request::fchmod(fd as u32, mode as u32);

        send_request(request, |response| match response {
            Response::Fchmod(_) => {
                log_message(&format!("received fchmod response for fd {}", fd));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fchmodat request to change file mode relative to directory fd
    fn send_fchmodat_request(
        dirfd: c_int,
        path: &CStr,
        mode: mode_t,
        flags: c_int,
    ) -> Result<(), String> {
        let resolved_path = resolve_at_path(dirfd, path);
        let path_str = resolved_path.to_string_lossy().into_owned();
        let request = Request::fchmodat(path_str.clone(), mode as u32, flags as u32);

        send_request(request, |response| match response {
            Response::Fchmodat(_) => {
                log_message(&format!(
                    "received fchmodat response for resolved path {}",
                    path_str
                ));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send statfs request to get filesystem statistics
    fn send_statfs_request(path: &CStr) -> Result<StatfsData, String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::statfs(path_str);

        send_request(request, |response| match response {
            Response::Statfs(statfs_response) => {
                log_message("received statfs response");
                Some(statfs_response.statfs)
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send fstatfs request to get filesystem statistics by file descriptor
    fn send_fstatfs_request(fd: c_int) -> Result<StatfsData, String> {
        let request = Request::fstatfs(fd as u32);

        send_request(request, |response| match response {
            Response::Fstatfs(fstatfs_response) => {
                log_message(&format!("received fstatfs response for fd {}", fd));
                Some(fstatfs_response.statfs)
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send truncate request to change file size
    fn send_truncate_request(path: &CStr, length: off_t) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::truncate(path_str, length as u64);

        send_request(request, |response| match response {
            Response::Truncate(_) => {
                log_message("received truncate response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send ftruncate request to change file size by file descriptor
    fn send_ftruncate_request(fd: c_int, length: off_t) -> Result<(), String> {
        let request = Request::ftruncate(fd as u32, length as u64);

        send_request(request, |response| match response {
            Response::Ftruncate(_) => {
                log_message(&format!("received ftruncate response for fd {}", fd));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send utimes request to change file access and modification times
    fn send_utimes_request(path: &CStr, times: Option<&[timespec; 2]>) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let times_data = times.map(|t| {
            (
                TimespecData {
                    tv_sec: t[0].tv_sec as u64,
                    tv_nsec: t[0].tv_nsec as u32,
                },
                TimespecData {
                    tv_sec: t[1].tv_sec as u64,
                    tv_nsec: t[1].tv_nsec as u32,
                },
            )
        });
        let request = Request::utimes(path_str, times_data);

        send_request(request, |response| match response {
            Response::Utimes(_) => {
                log_message("received utimes response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send futimes request to change file access and modification times by file descriptor
    fn send_futimes_request(fd: c_int, times: Option<&[timespec; 2]>) -> Result<(), String> {
        let times_data = times.map(|t| {
            (
                TimespecData {
                    tv_sec: t[0].tv_sec as u64,
                    tv_nsec: t[0].tv_nsec as u32,
                },
                TimespecData {
                    tv_sec: t[1].tv_sec as u64,
                    tv_nsec: t[1].tv_nsec as u32,
                },
            )
        });
        let request = Request::futimes(fd as u32, times_data);

        send_request(request, |response| match response {
            Response::Futimes(_) => {
                log_message(&format!("received futimes response for fd {}", fd));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send utimensat request to change file access and modification times relative to directory fd
    fn send_utimensat_request(
        dirfd: c_int,
        path: &CStr,
        times: Option<&[timespec; 2]>,
        flags: c_int,
    ) -> Result<(), String> {
        let resolved_path = resolve_at_path(dirfd, path);
        let path_str = resolved_path.to_string_lossy().into_owned();
        let times_data = times.map(|t| {
            (
                TimespecData {
                    tv_sec: t[0].tv_sec as u64,
                    tv_nsec: t[0].tv_nsec as u32,
                },
                TimespecData {
                    tv_sec: t[1].tv_sec as u64,
                    tv_nsec: t[1].tv_nsec as u32,
                },
            )
        });
        let request = Request::utimensat(path_str.clone(), times_data, flags as u32);

        send_request(request, |response| match response {
            Response::Utimensat(_) => {
                log_message(&format!(
                    "received utimensat response for resolved path {}",
                    path_str
                ));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send futimens request to change file access and modification times by file descriptor (nanosecond precision)
    fn send_futimens_request(fd: c_int, times: Option<&[timespec; 2]>) -> Result<(), String> {
        let times_data = times.map(|t| {
            (
                TimespecData {
                    tv_sec: t[0].tv_sec as u64,
                    tv_nsec: t[0].tv_nsec as u32,
                },
                TimespecData {
                    tv_sec: t[1].tv_sec as u64,
                    tv_nsec: t[1].tv_nsec as u32,
                },
            )
        });
        let request = Request::futimens(fd as u32, times_data);

        send_request(request, |response| match response {
            Response::Futimens(_) => {
                log_message(&format!("received futimens response for fd {}", fd));
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Interposed stat function
    stackable_hooks::hook! {
        unsafe fn stat(stackable_self, path: *const c_char, buf: *mut libc::stat) -> c_int => my_stat {
            if path.is_null() || buf.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing stat({})", c_path.to_string_lossy()));

            match send_stat_request(c_path) {
                Ok(stat_data) => {
                    // Convert StatData to libc::stat
                    let libc_stat = libc::stat {
                        st_dev: stat_data.st_dev as i32,
                        st_ino: stat_data.st_ino,
                        st_mode: stat_data.st_mode as u16,
                        st_nlink: stat_data.st_nlink as u16,
                        st_uid: stat_data.st_uid,
                        st_gid: stat_data.st_gid,
                        st_rdev: stat_data.st_rdev as i32,
                        st_size: stat_data.st_size as i64,
                        st_blksize: stat_data.st_blksize as i32,
                        st_blocks: stat_data.st_blocks as i64,
                        st_atime: stat_data.st_atime as i64,
                        st_atime_nsec: stat_data.st_atime_nsec as i64,
                        st_mtime: stat_data.st_mtime as i64,
                        st_mtime_nsec: stat_data.st_mtime_nsec as i64,
                        st_ctime: stat_data.st_ctime as i64,
                        st_ctime_nsec: stat_data.st_ctime_nsec as i64,

                        st_birthtime: 0, // Not provided by AgentFS yet

                        st_birthtime_nsec: 0,

                        st_flags: 0,

                        st_gen: 0,

                        st_lspare: 0,

                        st_qspare: [0; 2],
                    };

                    unsafe { *buf = libc_stat };
                    log_message("stat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("stat failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, stat, path, buf)
                }
            }
        }
    }

    /// Interposed lstat function
    stackable_hooks::hook! {
        unsafe fn lstat(stackable_self, path: *const c_char, buf: *mut libc::stat) -> c_int => my_lstat {
            if path.is_null() || buf.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing lstat({})", c_path.to_string_lossy()));

            match send_lstat_request(c_path) {
                Ok(stat_data) => {
                    // Convert StatData to libc::stat
                    let libc_stat = libc::stat {
                        st_dev: stat_data.st_dev as i32,
                        st_ino: stat_data.st_ino,
                        st_mode: stat_data.st_mode as u16,
                        st_nlink: stat_data.st_nlink as u16,
                        st_uid: stat_data.st_uid,
                        st_gid: stat_data.st_gid,
                        st_rdev: stat_data.st_rdev as i32,
                        st_size: stat_data.st_size as i64,
                        st_blksize: stat_data.st_blksize as i32,
                        st_blocks: stat_data.st_blocks as i64,
                        st_atime: stat_data.st_atime as i64,
                        st_atime_nsec: stat_data.st_atime_nsec as i64,
                        st_mtime: stat_data.st_mtime as i64,
                        st_mtime_nsec: stat_data.st_mtime_nsec as i64,
                        st_ctime: stat_data.st_ctime as i64,
                        st_ctime_nsec: stat_data.st_ctime_nsec as i64,

                        st_birthtime: 0, // Not provided by AgentFS yet

                        st_birthtime_nsec: 0,

                        st_flags: 0,

                        st_gen: 0,

                        st_lspare: 0,

                        st_qspare: [0; 2],
                    };

                    unsafe { *buf = libc_stat };
                    log_message("lstat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("lstat failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, lstat, path, buf)
                }
            }
        }
    }

    /// Interposed fstat function
    stackable_hooks::hook! {
        unsafe fn fstat(stackable_self, fd: c_int, buf: *mut libc::stat) -> c_int => my_fstat {
            if buf.is_null() {
                return -1;
            }

            log_message(&format!("interposing fstat({})", fd));

            match send_fstat_request(fd) {
                Ok(stat_data) => {
                    // Convert StatData to libc::stat
                    let libc_stat = libc::stat {
                        st_dev: stat_data.st_dev as i32,
                        st_ino: stat_data.st_ino,
                        st_mode: stat_data.st_mode as u16,
                        st_nlink: stat_data.st_nlink as u16,
                        st_uid: stat_data.st_uid,
                        st_gid: stat_data.st_gid,
                        st_rdev: stat_data.st_rdev as i32,
                        st_size: stat_data.st_size as i64,
                        st_blksize: stat_data.st_blksize as i32,
                        st_blocks: stat_data.st_blocks as i64,
                        st_atime: stat_data.st_atime as i64,
                        st_atime_nsec: stat_data.st_atime_nsec as i64,
                        st_mtime: stat_data.st_mtime as i64,
                        st_mtime_nsec: stat_data.st_mtime_nsec as i64,
                        st_ctime: stat_data.st_ctime as i64,
                        st_ctime_nsec: stat_data.st_ctime_nsec as i64,

                        st_birthtime: 0, // Not provided by AgentFS yet

                        st_birthtime_nsec: 0,

                        st_flags: 0,

                        st_gen: 0,

                        st_lspare: 0,

                        st_qspare: [0; 2],
                    };

                    unsafe { *buf = libc_stat };
                    log_message(&format!("fstat succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fstat failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, fstat, fd, buf)
                }
            }
        }
    }

    /// Interposed fstatat function
    stackable_hooks::hook! {
        unsafe fn fstatat(stackable_self, dirfd: c_int, path: *const c_char, buf: *mut libc::stat, flags: c_int) -> c_int => my_fstatat {
            if path.is_null() || buf.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing fstatat({}, {}, {:#x})", dirfd, c_path.to_string_lossy(), flags));

            match send_fstatat_request(dirfd, c_path, flags) {
                Ok(stat_data) => {
                    // Convert StatData to libc::stat
                    let libc_stat = libc::stat {
                        st_dev: stat_data.st_dev as i32,
                        st_ino: stat_data.st_ino,
                        st_mode: stat_data.st_mode as u16,
                        st_nlink: stat_data.st_nlink as u16,
                        st_uid: stat_data.st_uid,
                        st_gid: stat_data.st_gid,
                        st_rdev: stat_data.st_rdev as i32,
                        st_size: stat_data.st_size as i64,
                        st_blksize: stat_data.st_blksize as i32,
                        st_blocks: stat_data.st_blocks as i64,
                        st_atime: stat_data.st_atime as i64,
                        st_atime_nsec: stat_data.st_atime_nsec as i64,
                        st_mtime: stat_data.st_mtime as i64,
                        st_mtime_nsec: stat_data.st_mtime_nsec as i64,
                        st_ctime: stat_data.st_ctime as i64,
                        st_ctime_nsec: stat_data.st_ctime_nsec as i64,

                        st_birthtime: 0, // Not provided by AgentFS yet

                        st_birthtime_nsec: 0,

                        st_flags: 0,

                        st_gen: 0,

                        st_lspare: 0,

                        st_qspare: [0; 2],
                    };

                    unsafe { *buf = libc_stat };
                    log_message("fstatat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fstatat failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, fstatat, dirfd, path, buf, flags)
                }
            }
        }
    }

    /// Interposed statfs function
    stackable_hooks::hook! {
        unsafe fn statfs(stackable_self, path: *const c_char, buf: *mut libc::statfs) -> c_int => my_statfs {
            if path.is_null() || buf.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing statfs({})", c_path.to_string_lossy()));

            match send_statfs_request(c_path) {
                Ok(statfs_data) => {
                    // Convert StatfsData to libc::statfs
                    let libc_statfs = libc::statfs {
                        f_bsize: statfs_data.f_bsize,
                        f_iosize: 0, // Not provided by AgentFS
                        f_blocks: statfs_data.f_blocks,
                        f_bfree: statfs_data.f_bfree,
                        f_bavail: statfs_data.f_bavail,
                        f_files: statfs_data.f_files,
                        f_ffree: statfs_data.f_ffree,
                        f_fsid: unsafe { std::mem::zeroed() }, // Not provided by AgentFS
                        f_owner: 0, // Not provided by AgentFS
                        f_type: 0, // Not provided by AgentFS
                        f_flags: statfs_data.f_flag as u32,
                        f_fssubtype: 0, // Not provided by AgentFS
                        f_fstypename: [0; 16], // Not provided by AgentFS
                        f_mntonname: [0; 1024], // Not provided by AgentFS
                        f_mntfromname: [0; 1024], // Not provided by AgentFS
                        f_flags_ext: 0, // Not provided by AgentFS
                        f_reserved: [0; 7], // Not provided by AgentFS
                    };

                    unsafe { *buf = libc_statfs };
                    log_message("statfs succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("statfs failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, statfs, path, buf)
                }
            }
        }
    }

    /// Interposed fstatfs function
    stackable_hooks::hook! {
        unsafe fn fstatfs(stackable_self, fd: c_int, buf: *mut libc::statfs) -> c_int => my_fstatfs {
            if buf.is_null() {
                return -1;
            }

            log_message(&format!("interposing fstatfs({})", fd));

            match send_fstatfs_request(fd) {
                Ok(statfs_data) => {
                    // Convert StatfsData to libc::statfs
                    let libc_statfs = libc::statfs {
                        f_bsize: statfs_data.f_bsize,
                        f_iosize: 0, // Not provided by AgentFS
                        f_blocks: statfs_data.f_blocks,
                        f_bfree: statfs_data.f_bfree,
                        f_bavail: statfs_data.f_bavail,
                        f_files: statfs_data.f_files,
                        f_ffree: statfs_data.f_ffree,
                        f_fsid: unsafe { std::mem::zeroed() }, // Not provided by AgentFS
                        f_owner: 0, // Not provided by AgentFS
                        f_type: 0, // Not provided by AgentFS
                        f_flags: statfs_data.f_flag as u32,
                        f_fssubtype: 0, // Not provided by AgentFS
                        f_fstypename: [0; 16], // Not provided by AgentFS
                        f_mntonname: [0; 1024], // Not provided by AgentFS
                        f_mntfromname: [0; 1024], // Not provided by AgentFS
                        f_flags_ext: 0, // Not provided by AgentFS
                        f_reserved: [0; 7], // Not provided by AgentFS
                    };

                    unsafe { *buf = libc_statfs };
                    log_message(&format!("fstatfs succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fstatfs failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, fstatfs, fd, buf)
                }
            }
        }
    }

    /// Interposed truncate function
    stackable_hooks::hook! {
        unsafe fn truncate(stackable_self, path: *const c_char, length: off_t) -> c_int => my_truncate {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing truncate({}, {})", c_path.to_string_lossy(), length));

            match send_truncate_request(c_path, length) {
                Ok(()) => {
                    log_message("truncate succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("truncate failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, truncate, path, length)
                }
            }
        }
    }

    /// Interposed ftruncate function
    stackable_hooks::hook! {
        unsafe fn ftruncate(stackable_self, fd: c_int, length: off_t) -> c_int => my_ftruncate {
            log_message(&format!("interposing ftruncate({}, {})", fd, length));

            match send_ftruncate_request(fd, length) {
                Ok(()) => {
                    log_message(&format!("ftruncate succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("ftruncate failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, ftruncate, fd, length)
                }
            }
        }
    }

    /// Interposed utimes function
    stackable_hooks::hook! {
        unsafe fn utimes(stackable_self, path: *const c_char, times: *const timespec) -> c_int => my_utimes {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let times_opt = if times.is_null() {
                // Use current time if times is NULL
                None
            } else {
                Some(unsafe { &std::ptr::read(times as *const [timespec; 2]) })
            };
            log_message(&format!("interposing utimes({}, times={:?})", c_path.to_string_lossy(), times_opt.as_ref().map(|t| (t[0].tv_sec, t[1].tv_sec))));

            match send_utimes_request(c_path, times_opt) {
                Ok(()) => {
                    log_message("utimes succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("utimes failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, utimes, path, times)
                }
            }
        }
    }

    /// Interposed futimes function
    stackable_hooks::hook! {
        unsafe fn futimes(stackable_self, fd: c_int, times: *const timespec) -> c_int => my_futimes {
            let times_opt = if times.is_null() {
                // Use current time if times is NULL
                None
            } else {
                Some(unsafe { &std::ptr::read(times as *const [timespec; 2]) })
            };
            log_message(&format!("interposing futimes({}, times={:?})", fd, times_opt.as_ref().map(|t| (t[0].tv_sec, t[1].tv_sec))));

            match send_futimes_request(fd, times_opt) {
                Ok(()) => {
                    log_message(&format!("futimes succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("futimes failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, futimes, fd, times)
                }
            }
        }
    }

    /// Interposed utimensat function
    stackable_hooks::hook! {
        unsafe fn utimensat(stackable_self, dirfd: c_int, path: *const c_char, times: *const timespec, flags: c_int) -> c_int => my_utimensat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let times_opt = if times.is_null() {
                // Use current time if times is NULL
                None
            } else {
                Some(unsafe { &std::ptr::read(times as *const [timespec; 2]) })
            };
            log_message(&format!("interposing utimensat({}, {}, times={:?}, flags={:#x})", dirfd, c_path.to_string_lossy(), times_opt.as_ref().map(|t| (t[0].tv_sec, t[1].tv_sec)), flags));

            match send_utimensat_request(dirfd, c_path, times_opt, flags) {
                Ok(()) => {
                    log_message("utimensat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("utimensat failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, utimensat, dirfd, path, times, flags)
                }
            }
        }
    }

    /// Interposed futimens function
    stackable_hooks::hook! {
        unsafe fn futimens(stackable_self, fd: c_int, times: *const timespec) -> c_int => my_futimens {
            let times_opt = if times.is_null() {
                // Use current time if times is NULL
                None
            } else {
                Some(unsafe { &std::ptr::read(times as *const [timespec; 2]) })
            };
            log_message(&format!("interposing futimens({}, times={:?})", fd, times_opt.as_ref().map(|t| (t[0].tv_sec, t[1].tv_sec))));

            match send_futimens_request(fd, times_opt) {
                Ok(()) => {
                    log_message(&format!("futimens succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("futimens failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, futimens, fd, times)
                }
            }
        }
    }

    /// Interposed chown function
    stackable_hooks::hook! {
        unsafe fn chown(stackable_self, path: *const c_char, uid: uid_t, gid: gid_t) -> c_int => my_chown {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing chown({}, {}, {})", c_path.to_string_lossy(), uid, gid));

            match send_chown_request(c_path, uid, gid) {
                Ok(()) => {
                    log_message("chown succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("chown failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, chown, path, uid, gid)
                }
            }
        }
    }

    /// Interposed lchown function
    stackable_hooks::hook! {
        unsafe fn lchown(stackable_self, path: *const c_char, uid: uid_t, gid: gid_t) -> c_int => my_lchown {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing lchown({}, {}, {})", c_path.to_string_lossy(), uid, gid));

            match send_lchown_request(c_path, uid, gid) {
                Ok(()) => {
                    log_message("lchown succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("lchown failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, lchown, path, uid, gid)
                }
            }
        }
    }

    /// Interposed fchown function
    stackable_hooks::hook! {
        unsafe fn fchown(stackable_self, fd: c_int, uid: uid_t, gid: gid_t) -> c_int => my_fchown {
            log_message(&format!("interposing fchown({}, {}, {})", fd, uid, gid));

            match send_fchown_request(fd, uid, gid) {
                Ok(()) => {
                    log_message(&format!("fchown succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fchown failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, fchown, fd, uid, gid)
                }
            }
        }
    }

    /// Interposed fchownat function
    stackable_hooks::hook! {
        unsafe fn fchownat(stackable_self, dirfd: c_int, path: *const c_char, uid: uid_t, gid: gid_t, flags: c_int) -> c_int => my_fchownat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing fchownat({}, {}, {}, {}, {:#x})", dirfd, c_path.to_string_lossy(), uid, gid, flags));

            match send_fchownat_request(dirfd, c_path, uid, gid, flags) {
                Ok(()) => {
                    log_message("fchownat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fchownat failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, fchownat, dirfd, path, uid, gid, flags)
                }
            }
        }
    }

    /// Interposed chmod function
    stackable_hooks::hook! {
        unsafe fn chmod(stackable_self, path: *const c_char, mode: mode_t) -> c_int => my_chmod {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing chmod({}, {:#o})", c_path.to_string_lossy(), mode));

            match send_chmod_request(c_path, mode) {
                Ok(()) => {
                    log_message("chmod succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("chmod failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, chmod, path, mode)
                }
            }
        }
    }

    /// Interposed fchmod function
    stackable_hooks::hook! {
        unsafe fn fchmod(stackable_self, fd: c_int, mode: mode_t) -> c_int => my_fchmod {
            log_message(&format!("interposing fchmod({}, {:#o})", fd, mode));

            match send_fchmod_request(fd, mode) {
                Ok(()) => {
                    log_message(&format!("fchmod succeeded for fd {}", fd));
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fchmod failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, fchmod, fd, mode)
                }
            }
        }
    }

    /// Interposed fchmodat function
    stackable_hooks::hook! {
        unsafe fn fchmodat(stackable_self, dirfd: c_int, path: *const c_char, mode: mode_t, flags: c_int) -> c_int => my_fchmodat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing fchmodat({}, {}, {:#o}, {:#x})", dirfd, c_path.to_string_lossy(), mode, flags));

            match send_fchmodat_request(dirfd, c_path, mode, flags) {
                Ok(()) => {
                    log_message("fchmodat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("fchmodat failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, fchmodat, dirfd, path, mode, flags)
                }
            }
        }
    }

    fn send_rename_request(old_path: &CStr, new_path: &CStr) -> Result<(), String> {
        let old_path_str = old_path.to_string_lossy().into_owned();
        let new_path_str = new_path.to_string_lossy().into_owned();
        let request = Request::rename(old_path_str, new_path_str);

        send_request(request, |response| match response {
            Response::Rename(_) => {
                log_message("received rename response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send renameat request
    fn send_renameat_request(
        old_dirfd: c_int,
        old_path: &CStr,
        new_dirfd: c_int,
        new_path: &CStr,
    ) -> Result<(), String> {
        let old_resolved_path = resolve_at_path(old_dirfd, old_path);
        let new_resolved_path = resolve_at_path(new_dirfd, new_path);
        let old_path_str = old_resolved_path.to_string_lossy().into_owned();
        let new_path_str = new_resolved_path.to_string_lossy().into_owned();
        let request = Request::renameat(old_path_str, new_path_str);

        send_request(request, |response| match response {
            Response::Renameat(_) => {
                log_message("received renameat response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send renameatx_np request (macOS-specific)
    fn send_renameatx_np_request(
        old_dirfd: c_int,
        old_path: &CStr,
        new_dirfd: c_int,
        new_path: &CStr,
        flags: c_int,
    ) -> Result<(), String> {
        let old_path_str = old_path.to_string_lossy().into_owned();
        let new_path_str = new_path.to_string_lossy().into_owned();
        let request = Request::renameatx_np(
            old_dirfd as u32,
            old_path_str,
            new_dirfd as u32,
            new_path_str,
            flags as u32,
        );

        send_request(request, |response| match response {
            Response::RenameatxNp(_) => {
                log_message("received renameatx_np response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send link request
    fn send_link_request(old_path: &CStr, new_path: &CStr) -> Result<(), String> {
        let old_path_str = old_path.to_string_lossy().into_owned();
        let new_path_str = new_path.to_string_lossy().into_owned();
        let request = Request::link(old_path_str, new_path_str);

        send_request(request, |response| match response {
            Response::Link(_) => {
                log_message("received link response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send linkat request
    fn send_linkat_request(
        old_dirfd: c_int,
        old_path: &CStr,
        new_dirfd: c_int,
        new_path: &CStr,
        flags: c_int,
    ) -> Result<(), String> {
        let old_resolved_path = resolve_at_path(old_dirfd, old_path);
        let new_resolved_path = resolve_at_path(new_dirfd, new_path);
        let old_path_str = old_resolved_path.to_string_lossy().into_owned();
        let new_path_str = new_resolved_path.to_string_lossy().into_owned();
        let request = Request::linkat(old_path_str, new_path_str, flags as u32);

        send_request(request, |response| match response {
            Response::Linkat(_) => {
                log_message("received linkat response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send symlink request
    fn send_symlink_request(target: &CStr, linkpath: &CStr) -> Result<(), String> {
        let target_str = target.to_string_lossy().into_owned();
        let linkpath_str = linkpath.to_string_lossy().into_owned();
        let request = Request::symlink(target_str, linkpath_str);

        send_request(request, |response| match response {
            Response::Symlink(_) => {
                log_message("received symlink response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send symlinkat request
    fn send_symlinkat_request(
        target: &CStr,
        new_dirfd: c_int,
        linkpath: &CStr,
    ) -> Result<(), String> {
        let target_str = target.to_string_lossy().into_owned();
        let resolved_linkpath = resolve_at_path(new_dirfd, linkpath);
        let linkpath_str = resolved_linkpath.to_string_lossy().into_owned();
        let request = Request::symlinkat(target_str, linkpath_str);

        send_request(request, |response| match response {
            Response::Symlinkat(_) => {
                log_message("received symlinkat response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send unlink request
    fn send_unlink_request(path: &CStr) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::unlink(path_str);

        send_request(request, |response| match response {
            Response::Unlink(_) => {
                log_message("received unlink response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send unlinkat request
    fn send_unlinkat_request(dirfd: c_int, path: &CStr, flags: c_int) -> Result<(), String> {
        let resolved_path = resolve_at_path(dirfd, path);
        let path_str = resolved_path.to_string_lossy().into_owned();
        let request = Request::unlinkat(path_str, flags as u32);

        send_request(request, |response| match response {
            Response::Unlinkat(_) => {
                log_message("received unlinkat response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send remove request (alias for unlink)
    fn send_remove_request(path: &CStr) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::remove(path_str);

        send_request(request, |response| match response {
            Response::Remove(_) => {
                log_message("received remove response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send mkdir request
    fn send_mkdir_request(path: &CStr, mode: mode_t) -> Result<(), String> {
        let path_str = path.to_string_lossy().into_owned();
        let request = Request::mkdir(path_str, mode as u32);

        send_request(request, |response| match response {
            Response::Mkdir(_) => {
                log_message("received mkdir response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Send mkdirat request
    fn send_mkdirat_request(dirfd: c_int, path: &CStr, mode: mode_t) -> Result<(), String> {
        let resolved_path = resolve_at_path(dirfd, path);
        let path_str = resolved_path.to_string_lossy().into_owned();
        let request = Request::mkdirat(path_str, mode as u32);

        send_request(request, |response| match response {
            Response::Mkdirat(_) => {
                log_message("received mkdirat response");
                Some(())
            }
            Response::Error(err) => {
                log_message(&format!(
                    "daemon error: {}",
                    String::from_utf8_lossy(&err.error)
                ));
                None
            }
            _ => None,
        })
    }

    /// Interposed rename function
    stackable_hooks::hook! {
        unsafe fn rename(stackable_self, old_path: *const c_char, new_path: *const c_char) -> c_int => my_rename {
            if old_path.is_null() || new_path.is_null() {
                return -1;
            }

            let c_old_path = unsafe { CStr::from_ptr(old_path) };
            let c_new_path = unsafe { CStr::from_ptr(new_path) };
            log_message(&format!("interposing rename({}, {})", c_old_path.to_string_lossy(), c_new_path.to_string_lossy()));

            match send_rename_request(c_old_path, c_new_path) {
                Ok(()) => {
                    log_message("rename succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("rename failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, rename, old_path, new_path)
                }
            }
        }
    }

    /// Interposed renameat function
    stackable_hooks::hook! {
        unsafe fn renameat(stackable_self, old_dirfd: c_int, old_path: *const c_char, new_dirfd: c_int, new_path: *const c_char) -> c_int => my_renameat {
            if old_path.is_null() || new_path.is_null() {
                return -1;
            }

            let c_old_path = unsafe { CStr::from_ptr(old_path) };
            let c_new_path = unsafe { CStr::from_ptr(new_path) };
            log_message(&format!("interposing renameat({}, {}, {}, {})", old_dirfd, c_old_path.to_string_lossy(), new_dirfd, c_new_path.to_string_lossy()));

            match send_renameat_request(old_dirfd, c_old_path, new_dirfd, c_new_path) {
                Ok(()) => {
                    log_message("renameat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("renameat failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, renameat, old_dirfd, old_path, new_dirfd, new_path)
                }
            }
        }
    }

    /// Interposed renameatx_np function (macOS-specific)
    stackable_hooks::hook! {
        unsafe fn renameatx_np(stackable_self, old_dirfd: c_int, old_path: *const c_char, new_dirfd: c_int, new_path: *const c_char, flags: libc::c_uint) -> c_int => my_renameatx_np {
            if old_path.is_null() || new_path.is_null() {
                return -1;
            }

            let c_old_path = unsafe { CStr::from_ptr(old_path) };
            let c_new_path = unsafe { CStr::from_ptr(new_path) };
            log_message(&format!("interposing renameatx_np({}, {}, {}, {}, {:#x})", old_dirfd, c_old_path.to_string_lossy(), new_dirfd, c_new_path.to_string_lossy(), flags));

            match send_renameatx_np_request(old_dirfd, c_old_path, new_dirfd, c_new_path, flags as c_int) {
                Ok(()) => {
                    log_message("renameatx_np succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("renameatx_np failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, renameatx_np, old_dirfd, old_path, new_dirfd, new_path, flags)
                }
            }
        }
    }

    /// Interposed link function
    stackable_hooks::hook! {
        unsafe fn link(stackable_self, old_path: *const c_char, new_path: *const c_char) -> c_int => my_link {
            if old_path.is_null() || new_path.is_null() {
                return -1;
            }

            let c_old_path = unsafe { CStr::from_ptr(old_path) };
            let c_new_path = unsafe { CStr::from_ptr(new_path) };
            log_message(&format!("interposing link({}, {})", c_old_path.to_string_lossy(), c_new_path.to_string_lossy()));

            match send_link_request(c_old_path, c_new_path) {
                Ok(()) => {
                    log_message("link succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("link failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, link, old_path, new_path)
                }
            }
        }
    }

    /// Interposed linkat function
    stackable_hooks::hook! {
        unsafe fn linkat(stackable_self, old_dirfd: c_int, old_path: *const c_char, new_dirfd: c_int, new_path: *const c_char, flags: c_int) -> c_int => my_linkat {
            if old_path.is_null() || new_path.is_null() {
                return -1;
            }

            let c_old_path = unsafe { CStr::from_ptr(old_path) };
            let c_new_path = unsafe { CStr::from_ptr(new_path) };
            log_message(&format!("interposing linkat({}, {}, {}, {}, {:#x})", old_dirfd, c_old_path.to_string_lossy(), new_dirfd, c_new_path.to_string_lossy(), flags));

            match send_linkat_request(old_dirfd, c_old_path, new_dirfd, c_new_path, flags) {
                Ok(()) => {
                    log_message("linkat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("linkat failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, linkat, old_dirfd, old_path, new_dirfd, new_path, flags)
                }
            }
        }
    }

    /// Interposed symlink function
    stackable_hooks::hook! {
        unsafe fn symlink(stackable_self, target: *const c_char, linkpath: *const c_char) -> c_int => my_symlink {
            if target.is_null() || linkpath.is_null() {
                return -1;
            }

            let c_target = unsafe { CStr::from_ptr(target) };
            let c_linkpath = unsafe { CStr::from_ptr(linkpath) };
            log_message(&format!("interposing symlink({}, {})", c_target.to_string_lossy(), c_linkpath.to_string_lossy()));

            match send_symlink_request(c_target, c_linkpath) {
                Ok(()) => {
                    log_message("symlink succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("symlink failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, symlink, target, linkpath)
                }
            }
        }
    }

    /// Interposed symlinkat function
    stackable_hooks::hook! {
        unsafe fn symlinkat(stackable_self, target: *const c_char, new_dirfd: c_int, linkpath: *const c_char) -> c_int => my_symlinkat {
            if target.is_null() || linkpath.is_null() {
                return -1;
            }

            let c_target = unsafe { CStr::from_ptr(target) };
            let c_linkpath = unsafe { CStr::from_ptr(linkpath) };
            log_message(&format!("interposing symlinkat({}, {}, {})", c_target.to_string_lossy(), new_dirfd, c_linkpath.to_string_lossy()));

            match send_symlinkat_request(c_target, new_dirfd, c_linkpath) {
                Ok(()) => {
                    log_message("symlinkat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("symlinkat failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, symlinkat, target, new_dirfd, linkpath)
                }
            }
        }
    }

    /// Interposed unlink function
    stackable_hooks::hook! {
        unsafe fn unlink(stackable_self, path: *const c_char) -> c_int => my_unlink {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing unlink({})", c_path.to_string_lossy()));

            match send_unlink_request(c_path) {
                Ok(()) => {
                    log_message("unlink succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("unlink failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, unlink, path)
                }
            }
        }
    }

    /// Interposed unlinkat function
    stackable_hooks::hook! {
        unsafe fn unlinkat(stackable_self, dirfd: c_int, path: *const c_char, flags: c_int) -> c_int => my_unlinkat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing unlinkat({}, {}, {:#x})", dirfd, c_path.to_string_lossy(), flags));

            match send_unlinkat_request(dirfd, c_path, flags) {
                Ok(()) => {
                    log_message("unlinkat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("unlinkat failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, unlinkat, dirfd, path, flags)
                }
            }
        }
    }

    /// Interposed remove function (alias for unlink)
    stackable_hooks::hook! {
        unsafe fn remove(stackable_self, path: *const c_char) -> c_int => my_remove {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing remove({})", c_path.to_string_lossy()));

            match send_remove_request(c_path) {
                Ok(()) => {
                    log_message("remove succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("remove failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, remove, path)
                }
            }
        }
    }

    /// Interposed mkdir function
    stackable_hooks::hook! {
        unsafe fn mkdir(stackable_self, path: *const c_char, mode: mode_t) -> c_int => my_mkdir {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing mkdir({}, {:#o})", c_path.to_string_lossy(), mode));

            match send_mkdir_request(c_path, mode) {
                Ok(()) => {
                    log_message("mkdir succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("mkdir failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, mkdir, path, mode)
                }
            }
        }
    }

    /// Interposed mkdirat function
    stackable_hooks::hook! {
        unsafe fn mkdirat(stackable_self, dirfd: c_int, path: *const c_char, mode: mode_t) -> c_int => my_mkdirat {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            log_message(&format!("interposing mkdirat({}, {}, {:#o})", dirfd, c_path.to_string_lossy(), mode));

            match send_mkdirat_request(dirfd, c_path, mode) {
                Ok(()) => {
                    log_message("mkdirat succeeded");
                    0 // Success
                }
                Err(err) => {
                    log_message(&format!("mkdirat failed: {}, falling back to original", err));
                    // Fall back to original function
                    stackable_hooks::call_next!(stackable_self, mkdirat, dirfd, path, mode)
                }
            }
        }
    }

    /// Interposed close function (for fd tracking)
    stackable_hooks::hook! {
        unsafe fn close(stackable_self, fd: c_int) -> c_int => my_close_fd_tracking {
            // Check if this fd is a kqueue or being watched by kqueue and notify daemon to unregister
            {
                let is_kqueue = KQUEUE_FDS.lock().unwrap().contains(&fd);

                if is_kqueue {
                    // This is a kqueue being closed - clean up all its watches
                                let pid = std::process::id();
                    let request = Request::watch_unregister_kqueue(pid, fd as u32);

                    if let Err(e) = send_request(request, |_| None::<()>) {
                        log_message(&format!("failed to send kqueue unregister for fd {}: {}", fd, e));
                    } else {
                        log_message(&format!("sent kqueue unregister for kqueue fd {}", fd));
                    }

                    // Remove from kqueue tracking
                    KQUEUE_FDS.lock().unwrap().remove(&fd);

                    // Remove doorbell ident for this kqueue
                    DOORBELL_IDENTS.lock().unwrap().remove(&fd);

                    // Clear watcher table entries for this kqueue
                    WATCHER_TABLE.lock().unwrap().retain(|(kq_fd, _), _| *kq_fd != fd);
                } else {
                    // Check if this fd is being watched by kqueue
                    let mut watcher_table = WATCHER_TABLE.lock().unwrap();
                    // Find all kqueues watching this fd
                    let watching_kqueues: Vec<_> = watcher_table.keys()
                        .filter(|(_, watched_fd)| *watched_fd == fd as u32)
                        .map(|(kq_fd, _)| *kq_fd)
                        .collect();

                    // Remove all watcher table entries for this fd
                    watcher_table.retain(|(_, watched_fd), _| *watched_fd != fd as u32);

                    for kq_fd in watching_kqueues {
                        // Send unregister message to daemon
                        let pid = std::process::id();
                        let request = Request::watch_unregister_fd(pid, fd as u32);

                        if let Err(e) = send_request(request, |_| None::<()>) {
                            log_message(&format!("failed to send watch unregister for fd {}: {}", fd, e));
                        } else {
                            log_message(&format!("sent watch unregister for fd {} on kqueue {}", fd, kq_fd));
                        }
                    }
                }
            }

            // Remove fd mapping before closing
            with_dirfd_mapping(|mapping| {
                mapping.remove_path(fd);
                log_message(&format!("removed fd {} from tracking", fd));
            });

            stackable_hooks::call_next!(stackable_self, close, fd)
        }
    }

    /// Interposed dup function (for fd tracking)
    stackable_hooks::hook! {
        unsafe fn dup(stackable_self, oldfd: c_int) -> c_int => my_dup_fd_tracking {
            let result = stackable_hooks::call_next!(stackable_self, dup, oldfd);
            if result >= 0 {
                // Duplicate the fd mapping
                with_dirfd_mapping(|mapping| {
                    mapping.dup_fd(oldfd, result);
                    log_message(&format!("duplicated fd {} -> {}", oldfd, result));
                });
            }
            result
        }
    }

    /// Interposed dup2 function (for fd tracking)
    stackable_hooks::hook! {
        unsafe fn dup2(stackable_self, oldfd: c_int, newfd: c_int) -> c_int => my_dup2_fd_tracking {
            let result = stackable_hooks::call_next!(stackable_self, dup2, oldfd, newfd);
            if result >= 0 {
                // Duplicate the fd mapping
                with_dirfd_mapping(|mapping| {
                    mapping.dup_fd(oldfd, newfd);
                    log_message(&format!("duplicated fd {} -> {} with dup2", oldfd, newfd));
                });
            }
            result
        }
    }

    /// Interposed dup3 function (for fd tracking)
    #[cfg(target_os = "linux")]
    stackable_hooks::hook! {
        unsafe fn dup3(stackable_self, oldfd: c_int, newfd: c_int, flags: c_int) -> c_int => my_dup3_fd_tracking {
            let result = stackable_hooks::call_next!(stackable_self, dup3, oldfd, newfd, flags);
            if result >= 0 {
                // Duplicate the fd mapping
                with_dirfd_mapping(|mapping| {
                    mapping.dup_fd(oldfd, newfd);
                    log_message(&format!("duplicated fd {} -> {} with dup3", oldfd, newfd));
                });
            }
            result
        }
    }

    /// Interposed chdir function (for cwd tracking)
    stackable_hooks::hook! {
        unsafe fn chdir(stackable_self, path: *const c_char) -> c_int => my_chdir_fd_tracking {
            if path.is_null() {
                return stackable_hooks::call_next!(stackable_self, chdir, path);
            }

            let result = stackable_hooks::call_next!(stackable_self, chdir, path);
            if result == 0 {
                // Update current working directory
                let c_path = unsafe { CStr::from_ptr(path) };
                if let Ok(path_str) = c_path.to_str() {
                    with_dirfd_mapping(|mapping| {
                        let path_buf = PathBuf::from(path_str);
                        let canonical_path = path_buf.canonicalize().unwrap_or(path_buf);
                        mapping.set_cwd(canonical_path.clone());
                        log_message(&format!("updated cwd to {}", canonical_path.display()));
                    });
                }
            }
            result
        }
    }

    /// Interposed fchdir function (for cwd tracking)
    stackable_hooks::hook! {
        unsafe fn fchdir(stackable_self, fd: c_int) -> c_int => my_fchdir_fd_tracking {
            let result = stackable_hooks::call_next!(stackable_self, fchdir, fd);
            if result == 0 {
                // Update current working directory from fd
                with_dirfd_mapping(|mapping| {
                    if let Some(path) = mapping.get_path(fd).cloned() {
                        mapping.set_cwd(path.clone());
                        log_message(&format!("updated cwd to {} via fd {}", path.display(), fd));
                    }
                });
            }
            result
        }
    }

    /// Interposed getxattr function
    stackable_hooks::hook! {
        unsafe fn getxattr(stackable_self, path: *const c_char, name: *const c_char, value: *mut c_void, size: size_t) -> ssize_t => my_getxattr {
            if path.is_null() || name.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let c_name = unsafe { CStr::from_ptr(name) };

            log_message(&format!("interposing getxattr({}, {}, {}, {})",
                c_path.to_string_lossy(), c_name.to_string_lossy(), value as usize, size));

            let request = GetxattrRequest {
                path: c_path.to_bytes().to_vec(),
                name: c_name.to_bytes().to_vec(),
            };

            match send_request(Request::Getxattr((b"1".to_vec(), request)), |response| match response {
                Response::Getxattr(resp) => Some(resp),
                _ => None,
            }) {
                Ok(response) => {
                    let value_len = response.value.len();
                    if size == 0 {
                        // Return required buffer size
                        return value_len as ssize_t;
                    } else if value.is_null() {
                        return -1;
                    } else if value_len > size {
                        // Buffer too small
                        unsafe { *libc::__error() = libc::ERANGE };
                        return -1;
                    } else {
                        // Copy value to buffer
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                response.value.as_ptr(),
                                value as *mut u8,
                                value_len
                            );
                        }
                        return value_len as ssize_t;
                    }
                }
                Err(_) => {
                    // Fallback to original function
                    return stackable_hooks::call_next!(stackable_self, getxattr, path, name, value, size);
                }
            }
        }
    }

    /// Interposed lgetxattr function
    #[cfg(target_os = "linux")]
    stackable_hooks::hook! {
        unsafe fn lgetxattr(stackable_self, path: *const c_char, name: *const c_char, value: *mut c_void, size: size_t, position: u32, options: c_int) -> ssize_t => my_lgetxattr {
            if path.is_null() || name.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let c_name = unsafe { CStr::from_ptr(name) };

            log_message(&format!("interposing lgetxattr({}, {}, {}, {}, {}, {})",
                c_path.to_string_lossy(), c_name.to_string_lossy(), value as usize, size, position, options));

            let request = LgetxattrRequest {
                path: c_path.to_bytes().to_vec(),
                name: c_name.to_bytes().to_vec(),
            };

            match send_request(Request::Lgetxattr((b"1".to_vec(), request)), |response| match response {
                Response::Lgetxattr(resp) => Some(resp),
                _ => None,
            }) {
                Ok(response) => {
                    let value_len = response.value.len();
                    if size == 0 {
                        return value_len as ssize_t;
                    } else if value.is_null() {
                        return -1;
                    } else if value_len > size {
                        unsafe { *libc::__error() = libc::ERANGE };
                        return -1;
                    } else {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                response.value.as_ptr(),
                                value as *mut u8,
                                value_len
                            );
                        }
                        return value_len as ssize_t;
                    }
                }
                _ => {
                    return stackable_hooks::call_next!(stackable_self, lgetxattr, path, name, value, size, position, options);
                }
            }
        }
    }

    /// Interposed fgetxattr function
    stackable_hooks::hook! {
        unsafe fn fgetxattr(stackable_self, fd: c_int, name: *const c_char, value: *mut c_void, size: size_t, position: u32, options: c_int) -> ssize_t => my_fgetxattr {
            if name.is_null() {
                return -1;
            }

            let c_name = unsafe { CStr::from_ptr(name) };

            log_message(&format!("interposing fgetxattr({}, {}, {}, {}, {}, {})",
                fd, c_name.to_string_lossy(), value as usize, size, position, options));

            let request = FgetxattrRequest {
                handle_id: fd as u64, // For now, assume fd maps directly to handle_id
                name: c_name.to_bytes().to_vec(),
            };

            match send_request(Request::Fgetxattr((b"1".to_vec(), request)), |response| match response {
                Response::Fgetxattr(resp) => Some(resp),
                _ => None,
            }) {
                Ok(response) => {
                    let value_len = response.value.len();
                    if size == 0 {
                        return value_len as ssize_t;
                    } else if value.is_null() {
                        return -1;
                    } else if value_len > size {
                        unsafe { *libc::__error() = libc::ERANGE };
                        return -1;
                    } else {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                response.value.as_ptr(),
                                value as *mut u8,
                                value_len
                            );
                        }
                        return value_len as ssize_t;
                    }
                }
                _ => {
                    return stackable_hooks::call_next!(stackable_self, fgetxattr, fd, name, value, size, position, options);
                }
            }
        }
    }

    /// Interposed setxattr function
    stackable_hooks::hook! {
        unsafe fn setxattr(stackable_self, path: *const c_char, name: *const c_char, value: *const c_void, size: size_t, position: u32, options: c_int) -> c_int => my_setxattr {
            if path.is_null() || name.is_null() || value.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let c_name = unsafe { CStr::from_ptr(name) };
            let value_slice = unsafe { std::slice::from_raw_parts(value as *const u8, size) };

            log_message(&format!("interposing setxattr({}, {}, value[{}])",
                c_path.to_string_lossy(), c_name.to_string_lossy(), size));

            let request = SetxattrRequest {
                path: c_path.to_bytes().to_vec(),
                name: c_name.to_bytes().to_vec(),
                value: value_slice.to_vec(),
                flags: options as u32,
            };

            match send_request(Request::Setxattr((b"1".to_vec(), request)), |response| match response {
                Response::Setxattr(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                _ => {
                    return stackable_hooks::call_next!(stackable_self, setxattr, path, name, value, size, position, options);
                }
            }
        }
    }

    /// Interposed lsetxattr function
    #[cfg(target_os = "linux")]
    stackable_hooks::hook! {
        unsafe fn lsetxattr(stackable_self, path: *const c_char, name: *const c_char, value: *const c_void, size: size_t, position: u32, options: c_int) -> c_int => my_lsetxattr {
            if path.is_null() || name.is_null() || value.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let c_name = unsafe { CStr::from_ptr(name) };
            let value_slice = unsafe { std::slice::from_raw_parts(value as *const u8, size) };

            log_message(&format!("interposing lsetxattr({}, {}, value[{}])",
                c_path.to_string_lossy(), c_name.to_string_lossy(), size));

            let request = LsetxattrRequest {
                path: c_path.to_bytes().to_vec(),
                name: c_name.to_bytes().to_vec(),
                value: value_slice.to_vec(),
                flags: options as u32,
            };

            match send_request(Request::Lsetxattr((b"1".to_vec(), request)), |response| match response {
                Response::Lsetxattr(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                _ => {
                    return stackable_hooks::call_next!(stackable_self, lsetxattr, path, name, value, size, position, options);
                }
            }
        }
    }

    /// Interposed fsetxattr function
    stackable_hooks::hook! {
        unsafe fn fsetxattr(stackable_self, fd: c_int, name: *const c_char, value: *const c_void, size: size_t, position: u32, options: c_int) -> c_int => my_fsetxattr {
            if name.is_null() || value.is_null() {
                return -1;
            }

            let c_name = unsafe { CStr::from_ptr(name) };
            let value_slice = unsafe { std::slice::from_raw_parts(value as *const u8, size) };

            log_message(&format!("interposing fsetxattr({}, {}, value[{}])",
                fd, c_name.to_string_lossy(), size));

            let request = FsetxattrRequest {
                handle_id: fd as u64, // For now, assume fd maps directly to handle_id
                name: c_name.to_bytes().to_vec(),
                value: value_slice.to_vec(),
                flags: options as u32,
            };

            match send_request(Request::Fsetxattr((b"1".to_vec(), request)), |response| match response {
                Response::Fsetxattr(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                _ => {
                    return stackable_hooks::call_next!(stackable_self, fsetxattr, fd, name, value, size, position, options);
                }
            }
        }
    }

    /// Interposed listxattr function
    stackable_hooks::hook! {
        unsafe fn listxattr(stackable_self, path: *const c_char, namebuf: *mut c_char, size: size_t, options: c_int) -> ssize_t => my_listxattr {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing listxattr({}, {}, {}, {})",
                c_path.to_string_lossy(), namebuf as usize, size, options));

            let request = ListxattrRequest {
                path: c_path.to_bytes().to_vec(),
            };

            match send_request(Request::Listxattr((b"1".to_vec(), request)), |response| match response {
                Response::Listxattr(resp) => Some(resp),
                _ => None,
            }) {
                Ok(response) => {
                    let names_data: Vec<u8> = response.names.into_iter().flatten().collect();
                    let names_len = names_data.len();
                    if size == 0 {
                        return names_len as ssize_t;
                    } else if namebuf.is_null() {
                        return -1;
                    } else if names_len > size {
                        unsafe { *libc::__error() = libc::ERANGE };
                        return -1;
                    } else {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                names_data.as_ptr(),
                                namebuf as *mut u8,
                                names_len
                            );
                        }
                        return names_len as ssize_t;
                    }
                }
                _ => {
                    return stackable_hooks::call_next!(stackable_self, listxattr, path, namebuf, size, options);
                }
            }
        }
    }

    /// Interposed llistxattr function
    #[cfg(target_os = "linux")]
    stackable_hooks::hook! {
        unsafe fn llistxattr(stackable_self, path: *const c_char, namebuf: *mut c_char, size: size_t, options: c_int) -> ssize_t => my_llistxattr {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing llistxattr({}, {}, {}, {})",
                c_path.to_string_lossy(), namebuf as usize, size, options));

            let request = LlistxattrRequest {
                path: c_path.to_bytes().to_vec(),
            };

            match send_request(Request::Llistxattr((b"1".to_vec(), request)), |response| match response {
                Response::Llistxattr(resp) => Some(resp),
                _ => None,
            }) {
                Ok(response) => {
                    let names_data: Vec<u8> = response.names.into_iter().flatten().collect();
                    let names_len = names_data.len();
                    if size == 0 {
                        return names_len as ssize_t;
                    } else if namebuf.is_null() {
                        return -1;
                    } else if names_len > size {
                        unsafe { *libc::__error() = libc::ERANGE };
                        return -1;
                    } else {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                names_data.as_ptr(),
                                namebuf as *mut u8,
                                names_len
                            );
                        }
                        return names_len as ssize_t;
                    }
                }
                _ => {
                    return stackable_hooks::call_next!(stackable_self, llistxattr, path, namebuf, size, options);
                }
            }
        }
    }

    /// Interposed flistxattr function
    stackable_hooks::hook! {
        unsafe fn flistxattr(stackable_self, fd: c_int, namebuf: *mut c_char, size: size_t, options: c_int) -> ssize_t => my_flistxattr {
            log_message(&format!("interposing flistxattr({}, {}, {}, {})",
                fd, namebuf as usize, size, options));

            let request = FlistxattrRequest {
                handle_id: fd as u64, // For now, assume fd maps directly to handle_id
            };

            match send_request(Request::Flistxattr((b"1".to_vec(), request)), |response| match response {
                Response::Flistxattr(resp) => Some(resp),
                _ => None,
            }) {
                Ok(response) => {
                    let names_data: Vec<u8> = response.names.into_iter().flatten().collect();
                    let names_len = names_data.len();
                    if size == 0 {
                        return names_len as ssize_t;
                    } else if namebuf.is_null() {
                        return -1;
                    } else if names_len > size {
                        unsafe { *libc::__error() = libc::ERANGE };
                        return -1;
                    } else {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                names_data.as_ptr(),
                                namebuf as *mut u8,
                                names_len
                            );
                        }
                        return names_len as ssize_t;
                    }
                }
                _ => {
                    return stackable_hooks::call_next!(stackable_self, flistxattr, fd, namebuf, size, options);
                }
            }
        }
    }

    /// Interposed removexattr function
    stackable_hooks::hook! {
        unsafe fn removexattr(stackable_self, path: *const c_char, name: *const c_char, options: c_int) -> c_int => my_removexattr {
            if path.is_null() || name.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let c_name = unsafe { CStr::from_ptr(name) };

            log_message(&format!("interposing removexattr({}, {}, {})",
                c_path.to_string_lossy(), c_name.to_string_lossy(), options));

            let request = RemovexattrRequest {
                path: c_path.to_bytes().to_vec(),
                name: c_name.to_bytes().to_vec(),
            };

            match send_request(Request::Removexattr((b"1".to_vec(), request)), |response| match response {
                Response::Removexattr(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                _ => {
                    return stackable_hooks::call_next!(stackable_self, removexattr, path, name, options);
                }
            }
        }
    }

    /// Interposed lremovexattr function
    #[cfg(target_os = "linux")]
    stackable_hooks::hook! {
        unsafe fn lremovexattr(stackable_self, path: *const c_char, name: *const c_char, options: c_int) -> c_int => my_lremovexattr {
            if path.is_null() || name.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };
            let c_name = unsafe { CStr::from_ptr(name) };

            log_message(&format!("interposing lremovexattr({}, {}, {})",
                c_path.to_string_lossy(), c_name.to_string_lossy(), options));

            let request = LremovexattrRequest {
                path: c_path.to_bytes().to_vec(),
                name: c_name.to_bytes().to_vec(),
            };

            match send_request(Request::Lremovexattr((b"1".to_vec(), request)), |response| match response {
                Response::Lremovexattr(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                _ => {
                    return stackable_hooks::call_next!(stackable_self, lremovexattr, path, name, options);
                }
            }
        }
    }

    /// Interposed fremovexattr function
    stackable_hooks::hook! {
        unsafe fn fremovexattr(stackable_self, fd: c_int, name: *const c_char, options: c_int) -> c_int => my_fremovexattr {
            if name.is_null() {
                return -1;
            }

            let c_name = unsafe { CStr::from_ptr(name) };

            log_message(&format!("interposing fremovexattr({}, {}, {})",
                fd, c_name.to_string_lossy(), options));

            let request = FremovexattrRequest {
                handle_id: fd as u64, // For now, assume fd maps directly to handle_id
                name: c_name.to_bytes().to_vec(),
            };

            match send_request(Request::Fremovexattr((b"1".to_vec(), request)), |response| match response {
                Response::Fremovexattr(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, fremovexattr, fd, name, options);
                }
            }
        }
    }

    /// Interposed acl_get_file function
    stackable_hooks::hook! {
        unsafe fn acl_get_file(stackable_self, path: *const c_char, acl_type: acl_type_t) -> acl_t => my_acl_get_file {
            if path.is_null() {
                return std::ptr::null_mut();
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing acl_get_file({}, {})",
                c_path.to_string_lossy(), acl_type));

            let request = AclGetFileRequest {
                path: c_path.to_bytes().to_vec(),
                acl_type,
            };

            match send_request(Request::AclGetFile((b"1".to_vec(), request)), |response| match response {
                Response::AclGetFile(resp) => Some(resp.acl_data),
                _ => None,
            }) {
                Ok(acl_data) => {
                    // Convert binary ACL data back to acl_t
                    if !acl_data.is_empty() {
                        1 as acl_t
                    } else {
                        std::ptr::null_mut()
                    }
                }
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, acl_get_file, path, acl_type);
                }
            }
        }
    }

    /// Interposed acl_set_file function
    stackable_hooks::hook! {
        unsafe fn acl_set_file(stackable_self, path: *const c_char, acl_type: acl_type_t, acl: acl_t) -> c_int => my_acl_set_file {
            if path.is_null() || acl.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing acl_set_file({}, {}, {:p})",
                c_path.to_string_lossy(), acl_type, acl));

            // Convert acl_t to binary data
            let acl_data = if acl == 1 as acl_t {
                vec![1] // Dummy data
            } else {
                Vec::new()
            };

            let request = AclSetFileRequest {
                path: c_path.to_bytes().to_vec(),
                acl_type,
                acl_data,
            };

            match send_request(Request::AclSetFile((b"1".to_vec(), request)), |response| match response {
                Response::AclSetFile(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, acl_set_file, path, acl_type, acl);
                }
            }
        }
    }

    /// Interposed acl_get_fd function
    stackable_hooks::hook! {
        unsafe fn acl_get_fd(stackable_self, fd: c_int, acl_type: acl_type_t) -> acl_t => my_acl_get_fd {
            log_message(&format!("interposing acl_get_fd({}, {})", fd, acl_type));

            let request = AclGetFdRequest {
                handle_id: fd as u64,
                acl_type,
            };

            match send_request(Request::AclGetFd((b"1".to_vec(), request)), |response| match response {
                Response::AclGetFd(resp) => Some(resp.acl_data),
                _ => None,
            }) {
                Ok(acl_data) => {
                    // Convert binary ACL data back to acl_t
                    if !acl_data.is_empty() {
                        1 as acl_t
                    } else {
                        std::ptr::null_mut()
                    }
                }
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, acl_get_fd, fd, acl_type);
                }
            }
        }
    }

    /// Interposed acl_set_fd function
    stackable_hooks::hook! {
        unsafe fn acl_set_fd(stackable_self, fd: c_int, acl_type: acl_type_t, acl: acl_t) -> c_int => my_acl_set_fd {
            if acl.is_null() {
                return -1;
            }

            log_message(&format!("interposing acl_set_fd({}, {}, {:p})", fd, acl_type, acl));

            // Convert acl_t to binary data
            let acl_data = if acl == 1 as acl_t {
                vec![1] // Dummy data
            } else {
                Vec::new()
            };

            let request = AclSetFdRequest {
                handle_id: fd as u64,
                acl_type,
                acl_data,
            };

            match send_request(Request::AclSetFd((b"1".to_vec(), request)), |response| match response {
                Response::AclSetFd(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, acl_set_fd, fd, acl_type, acl);
                }
            }
        }
    }

    /// Interposed acl_delete_def_file function
    stackable_hooks::hook! {
        unsafe fn acl_delete_def_file(stackable_self, path: *const c_char) -> c_int => my_acl_delete_def_file {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing acl_delete_def_file({})",
                c_path.to_string_lossy()));

            let request = AclDeleteDefFileRequest {
                path: c_path.to_bytes().to_vec(),
            };

            match send_request(Request::AclDeleteDefFile((b"1".to_vec(), request)), |response| match response {
                Response::AclDeleteDefFile(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, acl_delete_def_file, path);
                }
            }
        }
    }

    /// Interposed chflags function
    stackable_hooks::hook! {
        unsafe fn chflags(stackable_self, path: *const c_char, flags: libc::c_uint) -> c_int => my_chflags {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing chflags({}, {:#x})",
                c_path.to_string_lossy(), flags));

            let request = ChflagsRequest {
                path: c_path.to_bytes().to_vec(),
                flags,
            };

            match send_request(Request::Chflags((b"1".to_vec(), request)), |response| match response {
                Response::Chflags(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, chflags, path, flags);
                }
            }
        }
    }

    /// Interposed lchflags function
    stackable_hooks::hook! {
        unsafe fn lchflags(stackable_self, path: *const c_char, flags: libc::c_uint) -> c_int => my_lchflags {
            if path.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing lchflags({}, {:#x})",
                c_path.to_string_lossy(), flags));

            let request = LchflagsRequest {
                path: c_path.to_bytes().to_vec(),
                flags,
            };

            match send_request(Request::Lchflags((b"1".to_vec(), request)), |response| match response {
                Response::Lchflags(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, lchflags, path, flags);
                }
            }
        }
    }

    /// Interposed fchflags function
    stackable_hooks::hook! {
        unsafe fn fchflags(stackable_self, fd: c_int, flags: libc::c_uint) -> c_int => my_fchflags {
            log_message(&format!("interposing fchflags({}, {:#x})", fd, flags));

            let request = FchflagsRequest {
                handle_id: fd as u64,
                flags,
            };

            match send_request(Request::Fchflags((b"1".to_vec(), request)), |response| match response {
                Response::Fchflags(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, fchflags, fd, flags);
                }
            }
        }
    }

    /// Interposed getattrlist function
    stackable_hooks::hook! {
        unsafe fn getattrlist(stackable_self, path: *const c_char, attr_list: *mut attrlist, attr_buf: *mut c_void, attr_buf_size: size_t, options: u_long) -> c_int => my_getattrlist {
            if path.is_null() || attr_list.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing getattrlist({}, attr_list={:p}, attr_buf={:p}, size={}, options={:#x})",
                c_path.to_string_lossy(), attr_list, attr_buf, attr_buf_size, options));

            // For now, serialize the attrlist structure as binary data
            let attr_list_data = unsafe {
                std::slice::from_raw_parts(attr_list as *const u8, std::mem::size_of::<attrlist>())
            }.to_vec();

            let request = GetattrlistRequest {
                path: c_path.to_bytes().to_vec(),
                attr_list: attr_list_data,
                options: options as u32,
            };

            match send_request(Request::Getattrlist((b"1".to_vec(), request)), |response| match response {
                Response::Getattrlist(resp) => Some(resp.attr_data),
                _ => None,
            }) {
                Ok(attr_data) => {
                    // Copy the result data back to the caller's buffer
                    if !attr_buf.is_null() && attr_buf_size > 0 && attr_data.len() <= attr_buf_size {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                attr_data.as_ptr(),
                                attr_buf as *mut u8,
                                attr_data.len()
                            );
                        }
                        attr_data.len() as c_int
                    } else if attr_buf_size == 0 {
                        // Size query - return the required size
                        attr_data.len() as c_int
                    } else {
                        -1 // Buffer too small or invalid
                    }
                }
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, getattrlist, path, attr_list, attr_buf, attr_buf_size, options);
                }
            }
        }
    }

    /// Interposed setattrlist function
    stackable_hooks::hook! {
        unsafe fn setattrlist(stackable_self, path: *const c_char, attr_list: *mut attrlist, attr_buf: *mut c_void, attr_buf_size: size_t, options: u_long) -> c_int => my_setattrlist {
            if path.is_null() || attr_list.is_null() {
                return -1;
            }

            let c_path = unsafe { CStr::from_ptr(path) };

            log_message(&format!("interposing setattrlist({}, attr_list={:p}, attr_buf={:p}, size={}, options={:#x})",
                c_path.to_string_lossy(), attr_list, attr_buf, attr_buf_size, options));

            // Serialize the attrlist structure
            let attr_list_data = unsafe {
                std::slice::from_raw_parts(attr_list as *const u8, std::mem::size_of::<attrlist>())
            }.to_vec();

            // Serialize the attribute buffer data
            let attr_data = if !attr_buf.is_null() && attr_buf_size > 0 {
                unsafe {
                    std::slice::from_raw_parts(attr_buf as *const u8, attr_buf_size)
                }.to_vec()
            } else {
                Vec::new()
            };

            let request = SetattrlistRequest {
                path: c_path.to_bytes().to_vec(),
                attr_list: attr_list_data,
                attr_data,
                options: options as u32,
            };

            match send_request(Request::Setattrlist((b"1".to_vec(), request)), |response| match response {
                Response::Setattrlist(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, setattrlist, path, attr_list, attr_buf, attr_buf_size, options);
                }
            }
        }
    }

    /// Interposed getattrlistbulk function
    stackable_hooks::hook! {
        unsafe fn getattrlistbulk(stackable_self, dirfd: c_int, attr_list: *mut attrlist, attr_buf: *mut c_void, attr_buf_size: size_t, options: u_int64_t) -> c_int => my_getattrlistbulk {
            if attr_list.is_null() {
                return -1;
            }

            log_message(&format!("interposing getattrlistbulk({}, attr_list={:p}, attr_buf={:p}, size={}, options={:#x})",
                dirfd, attr_list, attr_buf, attr_buf_size, options));

            // Serialize the attrlist structure
            let attr_list_data = unsafe {
                std::slice::from_raw_parts(attr_list as *const u8, std::mem::size_of::<attrlist>())
            }.to_vec();

            let request = GetattrlistbulkRequest {
                fd: dirfd as u32,
                attr_list: attr_list_data,
                options: options as u32,
            };

            match send_request(Request::Getattrlistbulk((b"1".to_vec(), request)), |response| match response {
                Response::Getattrlistbulk(resp) => Some(resp.entries),
                _ => None,
            }) {
                Ok(entries) => {
                    // For bulk operations, we need to pack multiple entries into the buffer
                    // This is a simplified implementation
                    if entries.is_empty() {
                        0 // No more entries
                    } else {
                        // Copy first entry as an example
                        if let Some(first_entry) = entries.first() {
                            if !attr_buf.is_null() && attr_buf_size >= first_entry.len() {
                                unsafe {
                                    std::ptr::copy_nonoverlapping(
                                        first_entry.as_ptr(),
                                        attr_buf as *mut u8,
                                        first_entry.len()
                                    );
                                }
                                1 // One entry returned
                            } else {
                                -1 // Buffer too small
                            }
                        } else {
                            0
                        }
                    }
                }
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, getattrlistbulk, dirfd, attr_list, attr_buf, attr_buf_size, options);
                }
            }
        }
    }

    /// Interposed copyfile function
    stackable_hooks::hook! {
        unsafe fn copyfile(stackable_self, from: *const c_char, to: *const c_char, state: copyfile_state_t, flags: copyfile_flags_t) -> c_int => my_copyfile {
            if from.is_null() || to.is_null() {
                return -1;
            }

            let c_from = unsafe { CStr::from_ptr(from) };
            let c_to = unsafe { CStr::from_ptr(to) };

            log_message(&format!("interposing copyfile({}, {}, state={:p}, flags={:#x})",
                c_from.to_string_lossy(), c_to.to_string_lossy(), state, flags));

            // Serialize copyfile state (simplified - real implementation would need to handle copyfile_state_t)
            let state_data = if !state.is_null() {
                vec![1] // Placeholder for state data
            } else {
                Vec::new()
            };

            let request = CopyfileRequest {
                src_path: c_from.to_bytes().to_vec(),
                dst_path: c_to.to_bytes().to_vec(),
                state: state_data,
                flags,
            };

            match send_request(Request::Copyfile((b"1".to_vec(), request)), |response| match response {
                Response::Copyfile(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, copyfile, from, to, state, flags);
                }
            }
        }
    }

    /// Interposed fcopyfile function
    stackable_hooks::hook! {
        unsafe fn fcopyfile(stackable_self, from_fd: c_int, to_fd: c_int, state: copyfile_state_t, flags: copyfile_flags_t) -> c_int => my_fcopyfile {
            log_message(&format!("interposing fcopyfile({}, {}, state={:p}, flags={:#x})",
                from_fd, to_fd, state, flags));

            // Serialize copyfile state (simplified)
            let state_data = if !state.is_null() {
                vec![1] // Placeholder for state data
            } else {
                Vec::new()
            };

            let request = FcopyfileRequest {
                src_fd: from_fd as u32,
                dst_fd: to_fd as u32,
                state: state_data,
                flags,
            };

            match send_request(Request::Fcopyfile((b"1".to_vec(), request)), |response| match response {
                Response::Fcopyfile(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, fcopyfile, from_fd, to_fd, state, flags);
                }
            }
        }
    }

    /// Interposed clonefile function
    stackable_hooks::hook! {
        unsafe fn clonefile(stackable_self, from: *const c_char, to: *const c_char, flags: c_int) -> c_int => my_clonefile {
            if from.is_null() || to.is_null() {
                return -1;
            }

            let c_from = unsafe { CStr::from_ptr(from) };
            let c_to = unsafe { CStr::from_ptr(to) };

            log_message(&format!("interposing clonefile({}, {}, flags={})",
                c_from.to_string_lossy(), c_to.to_string_lossy(), flags));

            let request = ClonefileRequest {
                src_path: c_from.to_bytes().to_vec(),
                dst_path: c_to.to_bytes().to_vec(),
                flags: flags as u32,
            };

            match send_request(Request::Clonefile((b"1".to_vec(), request)), |response| match response {
                Response::Clonefile(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, clonefile, from, to, flags);
                }
            }
        }
    }

    /// Interposed fclonefileat function
    stackable_hooks::hook! {
        unsafe fn fclonefileat(stackable_self, from_fd: c_int, to_fd: c_int, to: *const c_char, flags: c_int) -> c_int => my_fclonefileat {
            if to.is_null() {
                return -1;
            }

            let c_to = unsafe { CStr::from_ptr(to) };

            log_message(&format!("interposing fclonefileat({}, {}, {}, flags={})",
                from_fd, to_fd, c_to.to_string_lossy(), flags));

            let request = FclonefileatRequest {
                src_dirfd: from_fd as u32,
                src_path: Vec::new(), // Empty path for fd-based operation
                dst_dirfd: to_fd as u32,
                dst_path: c_to.to_bytes().to_vec(),
                flags: flags as u32,
            };

            match send_request(Request::Fclonefileat((b"1".to_vec(), request)), |response| match response {
                Response::Fclonefileat(_) => Some(()),
                _ => None,
            }) {
                Ok(_) => 0,
                Err(_) => {
                    return stackable_hooks::call_next!(stackable_self, fclonefileat, from_fd, to_fd, to, flags);
                }
            }
        }
    }

    /// Interposed FSEventStreamCreate function - translate overlay paths to backstore paths
    stackable_hooks::hook! {
        unsafe fn FSEventStreamCreate(stackable_self, allocator: CFAllocatorRef, callback: FSEventStreamCallback, context: *const FSEventStreamContext, paths_to_watch: CFArrayRef, since_when: FSEventStreamEventFlags, latency: CFTimeInterval, flags: FSEventStreamEventFlags) -> FSEventStreamRef => my_fsevent_stream_create {
            log_message(&format!("interposing FSEventStreamCreate(since_when={}, latency={}, flags={:#x})",
                since_when, latency, flags));

            // Generate a unique stream ID
            let stream_id = FSEVENTS_STREAM_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            // Store the original callback information
            let callback_info = FSEventsCallbackInfo {
                original_callback: callback,
                original_context: context as usize,
                stream_id,
            };

            // For in-memory AgentFS, we don't need path translation since there are no physical paths
            // Just create the stream with original paths and register it with the daemon
            let stream_ref = stackable_hooks::call_next!(stackable_self, FSEventStreamCreate, allocator, fsevents_replacement_callback, context, paths_to_watch, since_when, latency, flags);

            // Store the callback information keyed by stream reference
            if !stream_ref.is_null() {
                let mut callbacks = FSEVENTS_CALLBACKS.lock().unwrap();
                callbacks.insert(stream_ref as usize, callback_info);
                log_message(&format!("FSEvents: stored callback info for stream {:p}", stream_ref));

                // Register the FSEvents stream with the daemon
                // Extract the root paths from the CFArray
                let mut root_paths = Vec::new();
                if !paths_to_watch.is_null() {
                    // Get the count of paths in the array
                    let count = unsafe { CFArrayGetCount(paths_to_watch) };
                    for i in 0..count {
                        // Get each path string from the array
                        let cf_path = unsafe { CFArrayGetValueAtIndex(paths_to_watch, i) as CFStringRef };
                        if !cf_path.is_null() {
                            // Convert CFString to Rust String
                            let path_str = unsafe {
                                let c_str = CFStringGetCStringPtr(cf_path, KCFSTRINGENCODING_UTF8);
                                if !c_str.is_null() {
                                    std::ffi::CStr::from_ptr(c_str).to_string_lossy().into_owned()
                                } else {
                                    // Fallback: try to get the string as bytes
                                    let length = CFStringGetLength(cf_path);
                                    let mut buffer = vec![0u8; (length * 4) as usize]; // UTF-8 worst case
                                    let success = CFStringGetCString(cf_path, buffer.as_mut_ptr() as *mut c_char, buffer.len() as CFIndex, KCFSTRINGENCODING_UTF8);
                                    if success {
                                        let c_str = std::ffi::CStr::from_ptr(buffer.as_ptr() as *const c_char);
                                        c_str.to_string_lossy().into_owned()
                                    } else {
                                        log_message(&format!("Failed to convert CFString to Rust string for path at index {}", i));
                                        continue;
                                    }
                                }
                            };
                            root_paths.push(path_str.into_bytes());
                        }
                    }
                }

                let register_req = WatchRegisterFSEventsRequest {
                    pid: std::process::id(),
                    stream_id,
                    root_paths,
                    flags,
                    latency: (latency * 1_000_000_000.0) as u64, // Convert seconds to nanoseconds
                };

                match send_request(Request::WatchRegisterFSEvents((b"1".to_vec(), register_req)), |response| match response {
                    Response::WatchRegisterFSEvents(_) => Some(()),
                    _ => None,
                }) {
                    Ok(_) => log_message(&format!("FSEvents: registered stream {} with daemon", stream_id)),
                    Err(e) => log_message(&format!("FSEvents: failed to register stream {} with daemon: {}", stream_id, e)),
                }
            }

            stream_ref
        }
    }

    /// Interposed FSEventStreamCreateRelativeToDevice function
    stackable_hooks::hook! {
        unsafe fn FSEventStreamCreateRelativeToDevice(stackable_self, allocator: CFAllocatorRef, callback: FSEventStreamCallback, context: *const FSEventStreamContext, device_to_watch: dev_t, paths_to_watch_relative_to_device: CFArrayRef, since_when: FSEventStreamEventFlags, latency: CFTimeInterval, flags: FSEventStreamEventFlags) -> FSEventStreamRef => my_fsevent_stream_create_relative_to_device {
            log_message(&format!("interposing FSEventStreamCreateRelativeToDevice(device={}, since_when={}, latency={}, flags={:#x})",
                device_to_watch, since_when, latency, flags));

            // Generate a unique stream ID
            let stream_id = FSEVENTS_STREAM_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            // Store the original callback information
            let callback_info = FSEventsCallbackInfo {
                original_callback: callback,
                original_context: context as usize,
                stream_id,
            };

            // Device-relative paths don't need translation as they're already relative to a device
            // Just log and pass through with wrapped callback
            log_message("FSEventStreamCreateRelativeToDevice: device-relative paths, no translation needed");
            let stream_ref = stackable_hooks::call_next!(stackable_self, FSEventStreamCreateRelativeToDevice, allocator, fsevents_replacement_callback, context, device_to_watch, paths_to_watch_relative_to_device, since_when, latency, flags);

            // Store the callback information keyed by stream reference
            if !stream_ref.is_null() {
                let mut callbacks = FSEVENTS_CALLBACKS.lock().unwrap();
                callbacks.insert(stream_ref as usize, callback_info);
                log_message(&format!("FSEvents: stored callback info for device-relative stream {:p}", stream_ref));

                // Register the FSEvents stream with the daemon
                let register_req = WatchRegisterFSEventsRequest {
                    pid: std::process::id(),
                    stream_id,
                    root_paths: vec![], // Device-relative, no paths to register
                    flags,
                    latency: (latency * 1_000_000_000.0) as u64, // Convert seconds to nanoseconds
                };

                match send_request(Request::WatchRegisterFSEvents((b"1".to_vec(), register_req)), |response| match response {
                    Response::WatchRegisterFSEvents(_) => Some(()),
                    _ => None,
                }) {
                    Ok(_) => log_message(&format!("FSEvents: registered device-relative stream {} with daemon", stream_id)),
                    Err(e) => log_message(&format!("FSEvents: failed to register device-relative stream {} with daemon: {}", stream_id, e)),
                }
            }

            stream_ref
        }
    }

    /// Interposed FSEventStreamScheduleWithRunLoop function
    stackable_hooks::hook! {
        unsafe fn FSEventStreamScheduleWithRunLoop(stackable_self, stream_ref: FSEventStreamRef, run_loop: CFRunLoopRef, run_loop_mode: CFStringRef) -> () => my_fsevent_stream_schedule_with_run_loop {
            log_message(&format!("interposing FSEventStreamScheduleWithRunLoop for stream {:p}", stream_ref));

            // Create the message port if this is the first scheduling
            if let Err(e) = create_fsevents_message_port() {
                log_message(&format!("Failed to create FSEvents message port: {}", e));
            }

            // Add the message port source to this run loop/mode
            if let Err(e) = add_port_to_run_loop(run_loop, run_loop_mode) {
                log_message(&format!("Failed to add message port to run loop: {}", e));
            }

            // Call the original function
            stackable_hooks::call_next!(stackable_self, FSEventStreamScheduleWithRunLoop, stream_ref, run_loop, run_loop_mode);
        }
    }

    /// Interposed FSEventStreamStop function - clean up callback information
    stackable_hooks::hook! {
        unsafe fn FSEventStreamStop(stackable_self, stream_ref: FSEventStreamRef) -> () => my_fsevent_stream_stop {
            log_message(&format!("interposing FSEventStreamStop for stream {:p}", stream_ref));

            // Call the original function first
            stackable_hooks::call_next!(stackable_self, FSEventStreamStop, stream_ref);

            // Clean up our stored callback information
            let mut callbacks = FSEVENTS_CALLBACKS.lock().unwrap();
            callbacks.remove(&(stream_ref as usize));
            log_message(&format!("FSEvents: cleaned up callback info for stopped stream {:p}", stream_ref));
        }
    }

    /// Interposed FSEventStreamInvalidate function - clean up callback information
    stackable_hooks::hook! {
        unsafe fn FSEventStreamInvalidate(stackable_self, stream_ref: FSEventStreamRef) -> () => my_fsevent_stream_invalidate {
            log_message(&format!("interposing FSEventStreamInvalidate for stream {:p}", stream_ref));

            // Call the original function first
            stackable_hooks::call_next!(stackable_self, FSEventStreamInvalidate, stream_ref);

            // Clean up our stored callback information
            let mut callbacks = FSEVENTS_CALLBACKS.lock().unwrap();
            callbacks.remove(&(stream_ref as usize));
            log_message(&format!("FSEvents: cleaned up callback info for invalidated stream {:p}", stream_ref));
        }
    }

    /// Interposed kevent function - filter and inject vnode events based on AgentFS state
    stackable_hooks::hook! {
        unsafe fn kevent(stackable_self, kq: c_int, changelist: *const KeventStruct, nchanges: c_int, eventlist: *mut KeventStruct, nevents: c_int, timeout: *const libc::timespec) -> c_int => my_kevent {
            log_message(&format!("interposing kevent(kq={}, nchanges={}, nevents={})",
                kq, nchanges, nevents));

            // Collision hygiene: Check changelist for EVFILT_USER events that might collide with our doorbell
            if nchanges > 0 && !changelist.is_null() {
                let doorbell_idents = DOORBELL_IDENTS.lock().unwrap();
                let current_doorbell_ident = doorbell_idents.get(&kq).copied().unwrap_or(0);
                if current_doorbell_ident != 0 {
                    for i in 0..nchanges {
                        let change = &*changelist.offset(i as isize);
                        if change.filter == EVFILT_USER && (change.flags & EV_ADD) != 0 {
                            let requested_ident = change.ident;
                            if requested_ident >= 0xAFFE00000000usize && requested_ident as u64 == current_doorbell_ident {
                                log_message(&format!("Detected doorbell collision: app trying to register ident {:#x} which conflicts with our doorbell", requested_ident));

                                // Delete the current doorbell
                                let delete_kev = libc::kevent {
                                    ident: current_doorbell_ident as usize,
                                    filter: EVFILT_USER,
                                    flags: EV_DELETE,
                                    fflags: 0,
                                    data: 0,
                                    udata: std::ptr::null_mut(),
                                };
                                let delete_result = stackable_hooks::call_next!(stackable_self, kevent, kq, &delete_kev as *const _, 1, std::ptr::null_mut(), 0, std::ptr::null());
                                if delete_result == -1 {
                                    log_message(&format!("Failed to delete old doorbell ident {:#x}: {}", current_doorbell_ident, std::io::Error::last_os_error()));
                                }

                                // Allocate new doorbell ident
                                let new_doorbell_ident = generate_doorbell_ident();

                                // Register new doorbell
                                let add_kev = libc::kevent {
                                    ident: new_doorbell_ident as usize,
                                    filter: EVFILT_USER,
                                    flags: EV_ADD | EV_ENABLE | EV_CLEAR,
                                    fflags: 0,
                                    data: 0,
                                    udata: std::ptr::null_mut(),
                                };
                                let add_result = stackable_hooks::call_next!(stackable_self, kevent, kq, &add_kev as *const _, 1, std::ptr::null_mut(), 0, std::ptr::null());
                                if add_result == -1 {
                                    log_message(&format!("Failed to register new doorbell ident {:#x}: {}", new_doorbell_ident, std::io::Error::last_os_error()));
                                } else {
                                    // Update the doorbell ident for this kqueue
                                    drop(doorbell_idents);
                                    DOORBELL_IDENTS.lock().unwrap().insert(kq, new_doorbell_ident);

                                    // Notify daemon about the ident change
                                    let pid = std::process::id();
                                    let update_request = Request::update_doorbell_ident(pid, current_doorbell_ident, new_doorbell_ident);
                                    if let Err(e) = send_request(update_request, |_| Some(())) {
                                        log_message(&format!("Failed to notify daemon about doorbell ident change: {}", e));
                                    } else {
                                        log_message(&format!("Notified daemon: doorbell ident changed from {:#x} to {:#x}", current_doorbell_ident, new_doorbell_ident));
                                    }
                                }
                                break; // Only handle one collision per call
                            }
                        }
                    }
                }
            }

            // Update watcher table based on changelist (track EVFILT_VNODE registrations)
            if nchanges > 0 && !changelist.is_null() {
                let mut watcher_table = WATCHER_TABLE.lock().unwrap();
                for i in 0..nchanges {
                    let change = &*changelist.offset(i as isize);
                    if change.filter == EVFILT_VNODE && (change.flags & (EV_ADD | EV_DELETE)) != 0 {
                        let key = (kq, change.ident as u32);
                        let ident = change.ident;
                        let flags = change.flags;
                        let fflags = change.fflags;
                        if (flags & EV_DELETE) != 0 {
                            watcher_table.remove(&key);
                            log_message(&format!("Removed vnode watcher: kq={}, fd={}", kq, ident));
                        } else if (flags & EV_ADD) != 0 {
                            watcher_table.insert(key, fflags);
                            log_message(&format!("Added vnode watcher: kq={}, fd={}, fflags={:#x}", kq, ident, fflags));
                        }
                    }
                }
            }

            // Call the real kevent system call to get pending events
            let result = stackable_hooks::call_next!(stackable_self, kevent, kq, changelist, nchanges, eventlist, nevents, timeout);

            if result > 0 && !eventlist.is_null() {
                log_message(&format!("kevent returned {} events, checking for doorbell", result));

                // Check if we received our doorbell event
                let mut has_doorbell = false;
                let doorbell_idents = DOORBELL_IDENTS.lock().unwrap();
                let current_doorbell_ident = doorbell_idents.get(&kq).copied().unwrap_or(0);

                for i in 0..result {
                    let event = &*eventlist.offset(i as isize);
                    if event.filter == EVFILT_USER && event.ident == current_doorbell_ident as usize {
                        has_doorbell = true;
                        log_message("Received doorbell event, requesting pending events");
                        break;
                    }
                }

                // If we got a doorbell, request pending events from daemon
                let mut synthesized_events = Vec::new();
                if has_doorbell {
                    synthesized_events = request_pending_events(kq);
                }

                // Inject synthesized events into the result
                if !synthesized_events.is_empty() {
                    log_message(&format!("Injecting {} synthesized events", synthesized_events.len()));

                    // For now, append to the end if there's space
                    // In a full implementation, we'd need to merge properly
                    let available_space = nevents as usize - result as usize;
                    let events_to_inject = synthesized_events.len().min(available_space);

                    for (i, synthesized) in synthesized_events.iter().enumerate().take(events_to_inject) {
                        let target_idx = (result as usize + i) as isize;

                        // Convert SynthesizedKevent to libc::kevent
                        let injected_event = libc::kevent {
                            ident: synthesized.ident as usize, // Convert u64 to usize for libc
                            filter: synthesized.filter as i16, // Convert u16 back to i16 for libc
                            flags: synthesized.flags,
                            fflags: synthesized.fflags,
                            data: synthesized.data as isize, // Convert u64 to isize for libc
                            udata: synthesized.udata as *mut libc::c_void, // Convert u64 to pointer
                        };

                        // Copy to eventlist
                        *eventlist.offset(target_idx) = injected_event;
                    }

                    log_message(&format!("Injected {} events, new total: {}", events_to_inject, result as usize + events_to_inject));
                    return (result as usize + events_to_inject) as c_int;
                }
            }

            result
        }
    }

    /// Interposed kqueue function - setup doorbell channel to daemon

    stackable_hooks::hook! {
        unsafe fn kqueue(stackable_self) -> c_int => my_kqueue {
            log_message("interposing kqueue() - setting up doorbell channel");

            // Call the real kqueue system call first
            let kq_fd = stackable_hooks::call_next!(stackable_self, kqueue);

            if kq_fd == -1 {
                log_message("kqueue() failed");
                return kq_fd;
            }

            // Generate a unique doorbell ident for this kqueue
            let doorbell_ident = generate_doorbell_ident();
            log_message(&format!("kqueue doorbell: fd={}, ident={:#x}", kq_fd, doorbell_ident));

            // Register the doorbell EVFILT_USER event on this kqueue
            let kev = libc::kevent {
                ident: doorbell_ident as usize,
                filter: EVFILT_USER,
                flags: EV_ADD | EV_ENABLE | EV_CLEAR,
                fflags: 0,
                data: 0,
                udata: std::ptr::null_mut(),
            };

            let result = stackable_hooks::call_next!(stackable_self, kevent, kq_fd, &kev as *const _, 1, std::ptr::null_mut(), 0, std::ptr::null());
            if result == -1 {
                log_message(&format!("failed to register doorbell on kqueue fd {}", kq_fd));
                // Continue anyway - the kqueue is still valid
            } else {
                // Store the doorbell ident for this kqueue
                DOORBELL_IDENTS.lock().unwrap().insert(kq_fd, doorbell_ident);
                log_message(&format!("registered doorbell ident {:#x} for kqueue fd {}", doorbell_ident, kq_fd));
            }

            // Track this fd as a kqueue
            KQUEUE_FDS.lock().unwrap().insert(kq_fd);

            // Send WatchDoorbell message to daemon with kqueue FD via SCM_RIGHTS
            let pid = std::process::id();
            let request = Request::watch_doorbell(pid, kq_fd as u32, doorbell_ident);

            // Send the request with FD passing
            if let Err(e) = send_request_with_fd(request, kq_fd) {
                log_message(&format!("failed to send WatchDoorbell to daemon: {}", e));
                // Continue anyway - the app can still use kqueue normally
            } else {
                log_message(&format!("sent WatchDoorbell to daemon: pid={}, kq_fd={}, doorbell_ident={:#x}",
                    pid, kq_fd, doorbell_ident));
            }

            kq_fd
        }
    }

    /// Send a request to daemon with FD passing via SCM_RIGHTS
    fn send_request_with_fd(request: Request, fd_to_send: c_int) -> Result<(), String> {
        use std::os::unix::io::AsRawFd;

        let stream_arc = {
            let stream_guard = STREAM.lock().unwrap();
            match stream_guard.as_ref() {
                Some(arc) => Arc::clone(arc),
                None => return Err("not connected to AgentFS control socket".to_string()),
            }
        };

        let ssz_bytes = encode_ssz(&request);
        let ssz_len = ssz_bytes.len() as u32;

        // Prepare message with ancillary data for FD passing
        let len_bytes = ssz_len.to_le_bytes();
        let mut iov = iovec {
            iov_base: len_bytes.as_ptr() as *mut c_void,
            iov_len: 4,
        };

        let mut msg = msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: std::ptr::null_mut(),
            msg_controllen: 0,
            msg_flags: 0,
        };

        unsafe {
            // Allocate space for ancillary data
            let cmsg_space = CMSG_SPACE(std::mem::size_of::<c_int>() as c_uint) as usize;
            let mut cmsg_buf = vec![0u8; cmsg_space];
            msg.msg_control = cmsg_buf.as_mut_ptr() as *mut c_void;
            msg.msg_controllen = cmsg_buf.len() as u32;

            let cmsg = CMSG_FIRSTHDR(&msg);
            if !cmsg.is_null() {
                (*cmsg).cmsg_level = SOL_SOCKET;
                (*cmsg).cmsg_type = SCM_RIGHTS;
                (*cmsg).cmsg_len = CMSG_LEN(std::mem::size_of::<c_int>() as c_uint);
                *(CMSG_DATA(cmsg) as *mut c_int) = fd_to_send;
                msg.msg_controllen = (*cmsg).cmsg_len;
            }

            // Send the message with ancillary data
            let send_result = {
                let stream_guard = stream_arc.lock().unwrap();
                let stream_fd = stream_guard.as_raw_fd();
                libc::sendmsg(stream_fd, &msg as *const _, 0)
            };

            if send_result == -1 {
                return Err(format!(
                    "sendmsg failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }

        // Now send the SSZ payload
        {
            let mut stream_guard = stream_arc.lock().unwrap();
            stream_guard
                .write_all(&ssz_bytes)
                .map_err(|e| format!("write SSZ payload: {}", e))?;
        }

        Ok(())
    }

    /// Request pending synthesized events from the daemon for a specific kqueue
    fn request_pending_events(kq_fd: c_int) -> Vec<SynthesizedKevent> {
        let pid = std::process::id();

        // Create the request
        let request = Request::watch_drain_events(pid, kq_fd as u32, 64); // Request up to 64 events

        // Send request and get response
        match send_request(request, |response| match response {
            Response::WatchDrainEvents(resp) => Some(resp.events),
            _ => None,
        }) {
            Ok(events) => {
                log_message(&format!(
                    "Received {} pending events from daemon",
                    events.len()
                ));
                events
            }
            Err(e) => {
                log_message(&format!("Failed to request pending events: {}", e));
                Vec::new()
            }
        }
    }

    /// Interposed kevent64 function (64-bit variant)
    stackable_hooks::hook! {
        unsafe fn kevent64(stackable_self, kq: c_int, changelist: *const KeventStruct, nchanges: c_int, eventlist: *mut KeventStruct, nevents: c_int, flags: c_uint, timeout: *const libc::timespec) -> c_int => my_kevent64 {
            log_message(&format!("interposing kevent64(kq={}, nchanges={}, nevents={}, flags={:#x})",
                kq, nchanges, nevents, flags));

            // Call the real kevent64 system call
            let result = stackable_hooks::call_next!(stackable_self, kevent64, kq, changelist, nchanges, eventlist, nevents, flags, timeout);

            if result > 0 && !eventlist.is_null() {
                log_message(&format!("kevent64 returned {} events, filtering EVFILT_VNODE events", result));

                // Same filtering logic as kevent (TODO: implement when ready)
                let mut vnode_events = 0;
                for i in 0..result {
                    let event = &*eventlist.offset(i as isize);
                    if event.filter == EVFILT_VNODE {
                        vnode_events += 1;
                    }
                }

                log_message(&format!("kevent64: found {} EVFILT_VNODE events (filtering/injection not yet implemented)",
                    vnode_events));
            }

            result
        }
    }
}
