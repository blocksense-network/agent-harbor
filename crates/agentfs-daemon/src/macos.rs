// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! macOS-specific functionality for the AgentFS daemon.
//!
//! This module contains all macOS-specific code including kqueue operations
//! and other platform-specific filesystem watching functionality.

pub mod kqueue {
    // TODO: Move macOS-specific kqueue functionality here from watch_service.rs

    // CoreFoundation types
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

    // CoreFoundation constants
    pub const K_CFPROPERTY_LIST_BINARY_FORMAT_V1_0: CFPropertyListFormat = 200;
    pub const K_CFNUMBER_SINT32_TYPE: CFNumberType = 3;
    pub const K_CFNUMBER_SINT64_TYPE: CFNumberType = 4;
    pub const K_CFNUMBER_INT_TYPE: CFNumberType = 9;
    pub const K_CFSTRING_ENCODING_UTF8: u32 = 0x08000100;

    // RAII wrapper for CFNumber
    declare_TCFType!(CFNumber, CFNumberRef);
    impl_TCFType!(CFNumber, CFNumberRef, CFNumberGetTypeID);

    // RAII wrapper for CFMutableArray
    declare_TCFType!(CFMutableArray, CFMutableArrayRef);
    impl_TCFType!(CFMutableArray, CFMutableArrayRef, CFArrayGetTypeID);

    // RAII wrapper for CFMutableDictionary
    declare_TCFType!(CFMutableDictionary, CFMutableDictionaryRef);
    impl_TCFType!(
        CFMutableDictionary,
        CFMutableDictionaryRef,
        CFDictionaryGetTypeID
    );

    // RAII wrapper for CFData
    declare_TCFType!(CFData, CFDataRef);
    impl_TCFType!(CFData, CFDataRef, CFDataGetTypeID);

    // RAII wrapper for CFError
    declare_TCFType!(CFError, CFErrorRef);
    impl_TCFType!(CFError, CFErrorRef, CFErrorGetTypeID);

    extern "C" {
        pub static kCFAllocatorDefault: CFAllocatorRef;
        pub static kCFTypeArrayCallBacks: *const std::ffi::c_void;
        pub static kCFTypeDictionaryKeyCallBacks: *const std::ffi::c_void;
        pub static kCFTypeDictionaryValueCallBacks: *const std::ffi::c_void;

        pub fn CFRelease(cf: *mut std::ffi::c_void);

        // Type ID functions
        pub fn CFNumberGetTypeID() -> usize;
        pub fn CFArrayGetTypeID() -> usize;
        pub fn CFDictionaryGetTypeID() -> usize;
        pub fn CFDataGetTypeID() -> usize;
        pub fn CFErrorGetTypeID() -> usize;

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
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex};

    use core_foundation::{base::TCFType, declare_TCFType, impl_TCFType};
    use scopeguard::guard;

    // RAII wrapper for CF objects with release capability (like auto_ptr.release())
    pub struct OwnedCFRef {
        ptr: Option<*mut std::ffi::c_void>,
    }

    impl OwnedCFRef {
        pub fn new(ptr: *mut std::ffi::c_void) -> Self {
            Self { ptr: Some(ptr) }
        }

        // Transfer ownership out (like auto_ptr.release())
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

    use libc::{c_int, kevent as libc_kevent, timespec};

    // kqueue types and constants (macOS) - using libc types directly
    pub const EVFILT_USER: i16 = -5; // user events
    pub const NOTE_TRIGGER: u32 = 0x01000000; // trigger the event

    // kqueue vnode event flags (macOS)
    pub const EVFILT_VNODE: i16 = -4; // vnode events
    pub const NOTE_DELETE: u32 = 0x00000001;
    pub const NOTE_WRITE: u32 = 0x00000002;
    pub const NOTE_EXTEND: u32 = 0x00000004;
    pub const NOTE_ATTRIB: u32 = 0x00000008;
    #[cfg_attr(not(test), allow(dead_code))]
    pub const NOTE_LINK: u32 = 0x00000010;
    pub const NOTE_RENAME: u32 = 0x00000020;

    // FSEvents constants
    pub const K_FSEVENT_STREAM_EVENT_FLAG_ITEM_CREATED: u32 = 0x00000100;
    pub const K_FSEVENT_STREAM_EVENT_FLAG_ITEM_REMOVED: u32 = 0x00000200;
    pub const K_FSEVENT_STREAM_EVENT_FLAG_ITEM_MODIFIED: u32 = 0x00001000;
    pub const K_FSEVENT_STREAM_EVENT_FLAG_ITEM_RENAMED: u32 = 0x00000800;
    pub const K_FSEVENT_STREAM_EVENT_FLAG_ITEM_IS_FILE: u32 = 0x00010000;
    pub const K_FSEVENT_STREAM_EVENT_FLAG_ITEM_IS_DIR: u32 = 0x00020000;
}

pub mod interposition {
    // TODO: Move macOS-specific interposition functionality here
}
