// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! macOS-specific kqueue and FSEvents helpers for the AgentFS daemon.

use crate::AgentFsDaemon;

// CoreFoundation type aliases used throughout the watch service.
pub type CFAllocatorRef = *mut std::ffi::c_void;
pub type CFStringRef = *mut std::ffi::c_void;
pub type CFDataRef = *mut std::ffi::c_void;
pub type CFIndex = isize;
pub type SInt32 = i32;
pub type CFMutableDictionaryRef = *mut std::ffi::c_void;
pub type CFMutableArrayRef = *mut std::ffi::c_void;
pub type CFNumberRef = *mut std::ffi::c_void;
pub type CFPropertyListRef = *mut std::ffi::c_void;
pub type CFErrorRef = *mut std::ffi::c_void;
pub type CFPropertyListFormat = u32;
pub type CFNumberType = u64;

// CoreFoundation constants mirrored from platform headers.
pub const K_CFPROPERTY_LIST_BINARY_FORMAT_V1_0: CFPropertyListFormat = 200;
pub const K_CFNUMBER_SINT32_TYPE: CFNumberType = 3;
pub const K_CFNUMBER_SINT64_TYPE: CFNumberType = 4;
pub const K_CFNUMBER_INT_TYPE: CFNumberType = 9;
pub const K_CFSTRING_ENCODING_UTF8: u32 = 0x0800_0100;

// kqueue vnode event flags (macOS)
pub const EVFILT_USER: i16 = -5; // user events
pub const NOTE_TRIGGER: u32 = 0x01_00_00_00; // trigger the event
pub const EVFILT_VNODE: i16 = -4; // vnode events
pub const NOTE_DELETE: u32 = 0x0000_0001;
pub const NOTE_WRITE: u32 = 0x0000_0002;
pub const NOTE_EXTEND: u32 = 0x0000_0004;
pub const NOTE_ATTRIB: u32 = 0x0000_0008;
#[cfg_attr(not(test), allow(dead_code))]
pub const NOTE_LINK: u32 = 0x0000_0010;
pub const NOTE_RENAME: u32 = 0x0000_0020;

// FSEvents constants mirrored from platform headers.
pub const K_FSEVENT_STREAM_EVENT_FLAG_ITEM_CREATED: u32 = 0x0000_0100;
pub const K_FSEVENT_STREAM_EVENT_FLAG_ITEM_REMOVED: u32 = 0x0000_0200;
pub const K_FSEVENT_STREAM_EVENT_FLAG_ITEM_MODIFIED: u32 = 0x0000_1000;
pub const K_FSEVENT_STREAM_EVENT_FLAG_ITEM_RENAMED: u32 = 0x0000_0800;
pub const K_FSEVENT_STREAM_EVENT_FLAG_ITEM_IS_FILE: u32 = 0x0001_0000;
pub const K_FSEVENT_STREAM_EVENT_FLAG_ITEM_IS_DIR: u32 = 0x0002_0000;

// SSZ message identifier used for batched FSEvents notifications.
pub const AGENTFS_MSG_FSEVENTS_BATCH: SInt32 = 0x1001;

use core_foundation::{base::TCFType, declare_TCFType, impl_TCFType};

// RAII helpers for CoreFoundation types.
declare_TCFType!(CFNumber, CFNumberRef);
impl_TCFType!(CFNumber, CFNumberRef, CFNumberGetTypeID);

declare_TCFType!(CFMutableArray, CFMutableArrayRef);
impl_TCFType!(CFMutableArray, CFMutableArrayRef, CFArrayGetTypeID);

declare_TCFType!(CFMutableDictionary, CFMutableDictionaryRef);
impl_TCFType!(
    CFMutableDictionary,
    CFMutableDictionaryRef,
    CFDictionaryGetTypeID
);

declare_TCFType!(CFData, CFDataRef);
impl_TCFType!(CFData, CFDataRef, CFDataGetTypeID);

declare_TCFType!(CFError, CFErrorRef);
impl_TCFType!(CFError, CFErrorRef, CFErrorGetTypeID);

extern "C" {
    pub static kCFAllocatorDefault: CFAllocatorRef;
    pub static kCFTypeArrayCallBacks: *const std::ffi::c_void;
    pub static kCFTypeDictionaryKeyCallBacks: *const std::ffi::c_void;
    pub static kCFTypeDictionaryValueCallBacks: *const std::ffi::c_void;

    pub fn CFRelease(cf: *mut std::ffi::c_void);

    // Type ID functions
    fn CFNumberGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFDataGetTypeID() -> usize;
    fn CFErrorGetTypeID() -> usize;

    pub fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        c_str: *const std::ffi::c_char,
        encoding: u32,
    ) -> CFStringRef;

    // Property list functions
    pub fn CFDictionaryCreateMutable(
        allocator: CFAllocatorRef,
        capacity: CFIndex,
        key_call_backs: *const std::ffi::c_void,
        value_call_backs: *const std::ffi::c_void,
    ) -> CFMutableDictionaryRef;
    pub fn CFDictionarySetValue(
        the_dict: CFMutableDictionaryRef,
        key: *const std::ffi::c_void,
        value: *const std::ffi::c_void,
    );
    pub fn CFArrayCreateMutable(
        allocator: CFAllocatorRef,
        capacity: CFIndex,
        call_backs: *const std::ffi::c_void,
    ) -> CFMutableArrayRef;
    pub fn CFArrayAppendValue(the_array: CFMutableArrayRef, value: *const std::ffi::c_void);
    pub fn CFNumberCreate(
        allocator: CFAllocatorRef,
        the_type: CFNumberType,
        value_ptr: *const std::ffi::c_void,
    ) -> CFNumberRef;
    pub fn CFPropertyListCreateData(
        allocator: CFAllocatorRef,
        property_list: CFPropertyListRef,
        format: CFPropertyListFormat,
        options: u32,
        error: *mut CFErrorRef,
    ) -> CFDataRef;
}

use libc::{c_int, kevent as libc_kevent, timespec};

/// RAII wrapper for CoreFoundation references that releases ownership on drop.
pub struct OwnedCFRef {
    ptr: Option<*mut std::ffi::c_void>,
}

impl OwnedCFRef {
    pub fn new(ptr: *mut std::ffi::c_void) -> Self {
        Self { ptr: Some(ptr) }
    }

    /// Transfers ownership to the caller without releasing the underlying object.
    pub fn release(mut self) -> *mut std::ffi::c_void {
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

/// Metadata required to emit a batched FSEvents notification.
pub struct FseventsSendJob {
    pub pid: u32,
    pub registration_id: u64,
    pub stream_id: u64,
    pub root: String,
    pub paths: Vec<String>,
    pub flags: Vec<u32>,
    pub start_event_id: u64,
    pub reserved_next_event_id: u64,
}

/// Triggers an EVFILT_USER event on the provided kqueue descriptor.
pub fn trigger_user_event(
    kqueue_fd: c_int,
    doorbell_ident: u64,
    payload_id: u64,
) -> Result<(), String> {
    let kev = libc::kevent {
        ident: doorbell_ident as usize,
        filter: EVFILT_USER,
        flags: 0, // trigger existing registration
        fflags: NOTE_TRIGGER | ((payload_id & 0x00FF_FFFF) as u32),
        data: 0,
        udata: std::ptr::null_mut(),
    };

    let timeout = timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };

    let result = unsafe { libc_kevent(kqueue_fd, &kev, 1, std::ptr::null_mut(), 0, &timeout) };
    if result == -1 {
        Err(format!(
            "kevent doorbell failed: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

/// Serialises an FSEvents batch to a binary property list and forwards it to the shim.
pub fn send_fsevents_batch_via_port(
    daemon: &AgentFsDaemon,
    job: &FseventsSendJob,
) -> Result<(), String> {
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;

    let num_events = job.paths.len();
    if num_events == 0 {
        return Ok(());
    }
    if num_events != job.flags.len() {
        return Err(format!(
            "FSEvents batch requires parallel arrays but got {} paths and {} flags",
            num_events,
            job.flags.len()
        ));
    }
    if job.reserved_next_event_id <= job.start_event_id {
        return Err(format!(
            "Invalid event id reservation for pid {} stream {} (start={}, reserved_next={})",
            job.pid, job.stream_id, job.start_event_id, job.reserved_next_event_id
        ));
    }

    let latest_event_id = job.reserved_next_event_id - 1;
    let mut event_ids: Vec<u64> = Vec::with_capacity(num_events);
    for offset in 0..num_events {
        let event_id = job.start_event_id.checked_add(offset as u64).ok_or_else(|| {
            format!(
                "FSEvents event id overflow for pid {} stream {} (start={}, offset={})",
                job.pid, job.stream_id, job.start_event_id, offset
            )
        })?;
        event_ids.push(event_id);
    }

    if let Some(&computed_latest) = event_ids.last() {
        if computed_latest != latest_event_id {
            return Err(format!(
                "Latest event id mismatch for pid {} stream {} (expected {}, computed {})",
                job.pid, job.stream_id, latest_event_id, computed_latest
            ));
        }
    }

    unsafe {
        // Create CFArray for paths (CFString)
        let paths_array_raw = CFArrayCreateMutable(
            kCFAllocatorDefault,
            num_events as CFIndex,
            kCFTypeArrayCallBacks,
        );
        if paths_array_raw.is_null() {
            return Err("Failed to create paths CFArray".to_string());
        }
        let paths_array = OwnedCFRef::new(paths_array_raw);

        for path in &job.paths {
            let cf_string = CFString::new(path);
            CFArrayAppendValue(paths_array_raw, cf_string.as_CFTypeRef());
        }

        // Create CFArray for flags (CFNumber)
        let flags_array_raw = CFArrayCreateMutable(
            kCFAllocatorDefault,
            num_events as CFIndex,
            kCFTypeArrayCallBacks,
        );
        if flags_array_raw.is_null() {
            return Err("Failed to create flags CFArray".to_string());
        }
        let flags_array = OwnedCFRef::new(flags_array_raw);

        for &flag in &job.flags {
            let flag_number = CFNumber::from(flag as i32);
            CFArrayAppendValue(flags_array_raw, flag_number.as_CFTypeRef());
        }

        // Create CFArray for eventIds (CFNumber)
        let event_ids_array_raw = CFArrayCreateMutable(
            kCFAllocatorDefault,
            num_events as CFIndex,
            kCFTypeArrayCallBacks,
        );
        if event_ids_array_raw.is_null() {
            return Err("Failed to create eventIds CFArray".to_string());
        }
        let event_ids_array = OwnedCFRef::new(event_ids_array_raw);

        for event_id in &event_ids {
            let id_number = CFNumber::from(*event_id as i64);
            CFArrayAppendValue(event_ids_array_raw, id_number.as_CFTypeRef());
        }

        // Build dictionary payload
        let dict_raw = CFDictionaryCreateMutable(
            kCFAllocatorDefault,
            0,
            kCFTypeDictionaryKeyCallBacks,
            kCFTypeDictionaryValueCallBacks,
        );
        if dict_raw.is_null() {
            return Err("Failed to create CFDictionary".to_string());
        }
        let dict = OwnedCFRef::new(dict_raw);

        let paths_key = CFString::new("paths");
        CFDictionarySetValue(dict_raw, paths_key.as_CFTypeRef(), paths_array.release());

        let flags_key = CFString::new("flags");
        CFDictionarySetValue(dict_raw, flags_key.as_CFTypeRef(), flags_array.release());

        let ids_key = CFString::new("eventIds");
        CFDictionarySetValue(dict_raw, ids_key.as_CFTypeRef(), event_ids_array.release());

        let stream_key = CFString::new("streamId");
        let stream_value = CFNumber::from(job.stream_id as i64);
        CFDictionarySetValue(
            dict_raw,
            stream_key.as_CFTypeRef(),
            stream_value.as_CFTypeRef(),
        );

        let start_key = CFString::new("startEventId");
        let start_value = CFNumber::from(job.start_event_id as i64);
        CFDictionarySetValue(
            dict_raw,
            start_key.as_CFTypeRef(),
            start_value.as_CFTypeRef(),
        );

        let next_key = CFString::new("reservedNextEventId");
        let next_value = CFNumber::from(job.reserved_next_event_id as i64);
        CFDictionarySetValue(dict_raw, next_key.as_CFTypeRef(), next_value.as_CFTypeRef());

        let root_key = CFString::new("root");
        let root_str = CFString::new(&job.root);
        CFDictionarySetValue(dict_raw, root_key.as_CFTypeRef(), root_str.as_CFTypeRef());

        // Serialise dictionary to binary property list
        let mut error: CFErrorRef = std::ptr::null_mut();
        let cf_data_raw = CFPropertyListCreateData(
            kCFAllocatorDefault,
            dict.release() as CFPropertyListRef,
            K_CFPROPERTY_LIST_BINARY_FORMAT_V1_0,
            0,
            &mut error,
        );

        if !error.is_null() {
            let _error = OwnedCFRef::new(error);
            return Err("Failed to serialize property list".to_string());
        }

        if cf_data_raw.is_null() {
            return Err("Failed to create CFData from property list".to_string());
        }

        let cf_data = OwnedCFRef::new(cf_data_raw);
        let result = daemon.send_fsevents_batch(job.pid, AGENTFS_MSG_FSEVENTS_BATCH, cf_data_raw);
        drop(cf_data); // ensure property list is released after send attempt
        result
    }
}
