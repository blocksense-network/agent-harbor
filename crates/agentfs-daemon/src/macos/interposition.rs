// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! macOS-specific interposition helpers for the AgentFS daemon.

use std::collections::HashMap;

use core_foundation::base::TCFType;

pub use super::kqueue::SInt32;

pub type CFAllocatorRef = *mut std::ffi::c_void;
pub type CFStringRef = *mut std::ffi::c_void;
pub type CFMessagePortRef = *mut std::ffi::c_void;
pub type CFDataRef = *mut std::ffi::c_void;
pub type CFIndex = isize;
pub type CFTimeInterval = f64;

pub const K_CFSTRING_ENCODING_UTF8: u32 = 0x0800_0100;

extern "C" {
    pub static kCFAllocatorDefault: CFAllocatorRef;

    fn CFMessagePortCreateRemote(allocator: CFAllocatorRef, name: CFStringRef) -> CFMessagePortRef;
    fn CFMessagePortSendRequest(
        remote: CFMessagePortRef,
        msgid: SInt32,
        data: CFDataRef,
        send_timeout: CFTimeInterval,
        rcv_timeout: CFTimeInterval,
        reply_mode: CFStringRef,
        return_data: *mut CFDataRef,
    ) -> i32; // SInt32, 0 on success
    #[allow(dead_code)]
    fn CFMessagePortInvalidate(port: CFMessagePortRef);
}

#[derive(Clone)]
pub struct CFMessagePortWrapper(pub CFMessagePortRef);

unsafe impl Send for CFMessagePortWrapper {}
unsafe impl Sync for CFMessagePortWrapper {}

impl CFMessagePortWrapper {
    pub fn as_raw(&self) -> CFMessagePortRef {
        self.0
    }
}

pub fn create_remote_port(port_name: &str) -> Result<CFMessagePortWrapper, String> {
    use core_foundation::string::CFString;

    let cf_name = CFString::new(port_name);
    let port = unsafe {
        CFMessagePortCreateRemote(
            kCFAllocatorDefault,
            cf_name.as_concrete_TypeRef() as CFStringRef,
        )
    };
    if port.is_null() {
        Err("Failed to create CFMessagePort".to_string())
    } else {
        Ok(CFMessagePortWrapper(port))
    }
}

pub fn register_fsevents_port(
    ports: &mut HashMap<u32, CFMessagePortWrapper>,
    pid: u32,
    port: CFMessagePortWrapper,
) {
    ports.insert(pid, port);
}

/// # Safety
/// * `ports` must not be mutated concurrently in a way that invalidates existing port entries.
/// * `data` must be a valid CoreFoundation CFDataRef for the duration of the call.
/// * The provided `pid` must have a registered port, otherwise we return an error.
/// * `msgid` must correspond to the protocol understood by the remote shim.
///   Violating these preconditions could lead to undefined behaviour inside CoreFoundation APIs.
pub unsafe fn send_fsevents_batch(
    ports: &HashMap<u32, CFMessagePortWrapper>,
    pid: u32,
    msgid: SInt32,
    data: CFDataRef,
) -> Result<(), String> {
    if let Some(port_wrapper) = ports.get(&pid) {
        let result = CFMessagePortSendRequest(
            port_wrapper.as_raw(),
            msgid,
            data,
            1.0,
            0.0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );

        if result == 0 {
            Ok(())
        } else {
            Err(format!(
                "CFMessagePortSendRequest failed with code {}",
                result
            ))
        }
    } else {
        Err(format!("No FSEvents port registered for pid {}", pid))
    }
}
