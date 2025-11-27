// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Auto-propagation of library injection to child processes.
//!
//! This module provides per-library tracking of auto-propagation preferences.
//! Each library that uses stackable-hooks can independently enable or disable
//! auto-propagation. Only libraries that enable it will be propagated to child
//! processes.

use core::ffi::c_void;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};
use std::ffi::{CStr, CString};

// External reference to the global environment pointer
// https://pubs.opengroup.org/onlinepubs/9699919799/functions/environ.html
extern "C" {
    #[allow(improper_ctypes)]
    static mut environ: *mut *mut libc::c_char;
}

/// Node in the per-library propagation registry
///
/// Each library that uses stackable-hooks gets one static node that tracks
/// whether that library wants auto-propagation enabled.
#[repr(C)]
struct PropagationNode {
    next: *mut PropagationNode,
    library_path: *const libc::c_char,
    enabled: AtomicBool,
}

/// Global registry of libraries and their propagation preferences
///
/// This registry is shared across all libraries via the canonical_symbol mechanism,
/// ensuring all libraries see the same list.
#[repr(C)]
struct PropagationRegistry {
    lock: libc::pthread_mutex_t,
    head: *mut PropagationNode,
}

impl PropagationRegistry {
    const fn new() -> Self {
        Self {
            lock: libc::PTHREAD_MUTEX_INITIALIZER,
            head: std::ptr::null_mut(),
        }
    }
}

/// Global registry shared across all libraries
#[no_mangle]
static mut __STACKABLE_PROPAGATION_REGISTRY: PropagationRegistry = PropagationRegistry::new();

/// Per-library propagation node
///
/// Each library that links stackable-hooks gets its own static instance of this node.
/// It's registered in the global registry during library initialization.
#[no_mangle]
static mut __STACKABLE_PROPAGATION_NODE: PropagationNode = PropagationNode {
    next: std::ptr::null_mut(),
    library_path: std::ptr::null(),
    enabled: AtomicBool::new(false),
};

/// Environment variable name for Linux library injection
#[cfg(target_env = "gnu")]
const INJECT_VAR_NAME: &str = "LD_PRELOAD";

/// Environment variable name for macOS library injection
#[cfg(any(target_os = "macos", target_os = "ios"))]
const INJECT_VAR_NAME: &str = "DYLD_INSERT_LIBRARIES";

/// Get the canonical (shared) propagation registry across all libraries
///
/// # Safety
///
/// Returns a pointer to the shared registry. The caller must not hold the registry
/// lock when calling functions that might recursively acquire it.
#[cfg(any(target_os = "macos", target_os = "ios"))]
unsafe fn get_propagation_registry() -> *mut PropagationRegistry {
    crate::dyld_insert_libraries::canonical_symbol(
        "__STACKABLE_PROPAGATION_REGISTRY\0",
        core::ptr::addr_of_mut!(__STACKABLE_PROPAGATION_REGISTRY),
    )
}

/// Get the canonical (shared) propagation registry across all libraries (Linux version)
///
/// On Linux with glibc, we use dlsym(RTLD_DEFAULT) which is similar to canonical_symbol
///
/// # Safety
///
/// Returns a pointer to the shared registry.
#[cfg(target_env = "gnu")]
unsafe fn get_propagation_registry() -> *mut PropagationRegistry {
    let sym_name = CString::new("__STACKABLE_PROPAGATION_REGISTRY").unwrap();
    let ptr = libc::dlsym(libc::RTLD_DEFAULT, sym_name.as_ptr());
    if ptr.is_null() {
        core::ptr::addr_of_mut!(__STACKABLE_PROPAGATION_REGISTRY)
    } else {
        ptr as *mut PropagationRegistry
    }
}

/// Register this library's propagation node in the global registry.
///
/// This captures the library path using dladdr and adds the node to the registry.
///
/// # Safety
///
/// Must be called during library initialization.
unsafe fn register_propagation_node() {
    // Get our library path using dladdr on a symbol from this library
    let mut dl_info: MaybeUninit<libc::Dl_info> = MaybeUninit::uninit();
    let node_ptr = core::ptr::addr_of_mut!(__STACKABLE_PROPAGATION_NODE);
    if libc::dladdr(node_ptr as *const c_void, dl_info.as_mut_ptr()) != 0 {
        let dl_info = dl_info.assume_init();
        (*node_ptr).library_path = dl_info.dli_fname;

        // Register in the global registry
        let registry = get_propagation_registry();
        let lock_rc = libc::pthread_mutex_lock(&mut (*registry).lock as *mut _);
        if lock_rc != 0 {
            // Can't panic in ctor, just return
            return;
        }

        // Add to front of list
        (*node_ptr).next = (*registry).head;
        (*registry).head = node_ptr;

        libc::pthread_mutex_unlock(&mut (*registry).lock as *mut _);
    }
}

/// Enable automatic propagation of this library to child processes.
///
/// When enabled, this specific library will be included in the LD_PRELOAD or
/// DYLD_INSERT_LIBRARIES environment variables of spawned subprocesses.
///
/// This must be called explicitly to opt into auto-propagation. Each library
/// can independently choose whether to propagate.
pub fn enable_auto_propagation() {
    unsafe {
        let node_ptr = core::ptr::addr_of_mut!(__STACKABLE_PROPAGATION_NODE);
        (*node_ptr).enabled.store(true, Ordering::Release);
    }
}

/// Disable automatic propagation of this library to child processes.
pub fn disable_auto_propagation() {
    unsafe {
        let node_ptr = core::ptr::addr_of_mut!(__STACKABLE_PROPAGATION_NODE);
        (*node_ptr).enabled.store(false, Ordering::Release);
    }
}

/// Get the list of libraries to propagate to child processes.
///
/// This traverses the registry and collects all libraries that have enabled
/// auto-propagation, returning them as a colon-separated (or semicolon on Windows)
/// string suitable for LD_PRELOAD or DYLD_INSERT_LIBRARIES.
///
/// Returns None if no libraries want propagation.
fn get_libraries_to_propagate() -> Option<CString> {
    unsafe {
        let registry = get_propagation_registry();
        let lock_rc = libc::pthread_mutex_lock(&mut (*registry).lock as *mut _);
        if lock_rc != 0 {
            return None;
        }

        let mut paths: Vec<&[u8]> = Vec::new();
        let mut current = (*registry).head;

        while !current.is_null() {
            if (*current).enabled.load(Ordering::Acquire) && !(*current).library_path.is_null() {
                let path = CStr::from_ptr((*current).library_path);
                paths.push(path.to_bytes());
            }
            current = (*current).next;
        }

        libc::pthread_mutex_unlock(&mut (*registry).lock as *mut _);

        if paths.is_empty() {
            return None;
        }

        // Join paths with separator
        let separator = if cfg!(unix) { b":" } else { b";" };
        let mut result = Vec::new();
        for (i, path) in paths.iter().enumerate() {
            if i > 0 {
                result.extend_from_slice(separator);
            }
            result.extend_from_slice(path);
        }

        CString::new(result).ok()
    }
}

/// Modify an environment array to include the library injection variable.
///
/// # Safety
///
/// The caller must ensure that:
/// - `envp` is either NULL or points to a NULL-terminated array of valid C strings
/// - The returned pointer must be freed using `free_modified_envp` when no longer needed
/// - The original `envp` array remains valid during this call
///
/// # Returns
///
/// Returns a new NULL-terminated array with the injection variable set/modified/removed.
/// If no libraries want propagation, removes the injection variable if it exists.
/// Returns NULL only if memory allocation fails.
pub unsafe fn modify_envp_with_injection(envp: *const *mut libc::c_char) -> *mut *mut libc::c_char {
    // Get the libraries to propagate
    let libs_to_inject = get_libraries_to_propagate();

    // Determine the source environment
    let source_envp = if envp.is_null() {
        environ
    } else {
        envp as *mut *mut libc::c_char
    };

    // Count existing environment variables and check if injection var already exists
    let mut count = 0;
    let mut has_inject_var = false;
    let mut inject_var_index = 0;

    if !source_envp.is_null() {
        while !(*source_envp.offset(count)).is_null() {
            let env_entry = CStr::from_ptr(*source_envp.offset(count));
            if let Ok(entry_str) = env_entry.to_str() {
                if entry_str.starts_with(INJECT_VAR_NAME)
                    && entry_str.as_bytes().get(INJECT_VAR_NAME.len()) == Some(&b'=')
                {
                    has_inject_var = true;
                    inject_var_index = count;
                }
            }
            count += 1;
        }
    }

    // Determine what to do based on whether we have libraries to inject
    match libs_to_inject {
        Some(libs) => {
            // We have libraries to propagate - add/replace the injection variable
            let inject_entry = format!("{}={}", INJECT_VAR_NAME, libs.to_string_lossy());
            let inject_cstring = match CString::new(inject_entry) {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };

            // Allocate new environment array
            let new_size = if has_inject_var {
                (count + 1) as usize // Replace existing entry
            } else {
                (count + 2) as usize // Add new entry + NULL terminator
            };

            let new_envp = libc::malloc(new_size * core::mem::size_of::<*mut libc::c_char>())
                as *mut *mut libc::c_char;
            if new_envp.is_null() {
                return std::ptr::null_mut();
            }

            // Copy existing entries, replacing or adding the injection variable
            let mut dest_idx = 0;
            let mut src_idx = 0;

            while src_idx < count {
                if has_inject_var && src_idx == inject_var_index {
                    // Replace the existing injection variable
                    *new_envp.offset(dest_idx) = libc::strdup(inject_cstring.as_ptr());
                } else {
                    // Copy the existing entry
                    *new_envp.offset(dest_idx) = libc::strdup(*source_envp.offset(src_idx));
                }
                dest_idx += 1;
                src_idx += 1;
            }

            // If we didn't replace an existing entry, add a new one
            if !has_inject_var {
                *new_envp.offset(dest_idx) = libc::strdup(inject_cstring.as_ptr());
                dest_idx += 1;
            }

            // NULL terminator
            *new_envp.offset(dest_idx) = std::ptr::null_mut();

            new_envp
        }
        None => {
            // No libraries want propagation - remove the injection variable if it exists
            if !has_inject_var {
                // No injection variable in the environment, no modification needed
                return std::ptr::null_mut();
            }

            // Allocate new environment array (one less entry since we're removing the injection var)
            let new_size = count as usize; // count includes the inject var we're removing, so final size is count
            let new_envp = libc::malloc(new_size * core::mem::size_of::<*mut libc::c_char>())
                as *mut *mut libc::c_char;
            if new_envp.is_null() {
                return std::ptr::null_mut();
            }

            // Copy all entries except the injection variable
            let mut dest_idx = 0;
            let mut src_idx = 0;

            while src_idx < count {
                if src_idx != inject_var_index {
                    *new_envp.offset(dest_idx) = libc::strdup(*source_envp.offset(src_idx));
                    dest_idx += 1;
                }
                src_idx += 1;
            }

            // NULL terminator
            *new_envp.offset(dest_idx) = std::ptr::null_mut();

            new_envp
        }
    }
}

/// Free an environment array created by `modify_envp_with_injection`.
///
/// # Safety
///
/// The caller must ensure that `envp` was created by `modify_envp_with_injection`
/// and has not been freed already.
pub unsafe fn free_modified_envp(envp: *mut *mut libc::c_char) {
    if envp.is_null() {
        return;
    }

    // Free each string in the array
    let mut i = 0;
    while !(*envp.offset(i)).is_null() {
        libc::free(*envp.offset(i) as *mut libc::c_void);
        i += 1;
    }

    // Free the array itself
    libc::free(envp as *mut libc::c_void);
}

/// Initialize the auto-propagation module.
/// This is called automatically via a ctor function.
#[ctor::ctor]
fn init_auto_propagation() {
    unsafe {
        register_propagation_node();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enable_disable() {
        disable_auto_propagation();
        enable_auto_propagation();
        disable_auto_propagation();
        // Can't easily test the atomic bool from outside, but at least it doesn't crash
    }
}
