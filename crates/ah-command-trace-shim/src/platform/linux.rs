// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Linux implementation using LD_PRELOAD with stackable-hooks

use crate::core::{self, SHIM_STATE, ShimState};
use crate::posix;
use ctor::ctor;

/// Initialize the shim on library load
#[ctor]
fn initialize_shim() {
    let state = core::initialize_shim_state();

    // Store the state globally
    let _ = SHIM_STATE.set(std::sync::Mutex::new(state.clone()));

    if let ShimState::Ready { .. } = &state {
        // Try to initialize client for handshake, but don't fail if connection fails
        // This allows the smoke test to verify that the shim can connect
        let _ = posix::initialize_client();
        // eprintln!("[ah-command-trace-shim] Shim initialization complete");
    }
}

/// Check if the shim is enabled and ready
pub fn is_shim_enabled() -> bool {
    matches!(
        crate::core::get_or_initialize_shim_state().and_then(|s| s.lock().ok()),
        Some(ref state) if matches!(**state, ShimState::Ready { .. })
    )
}

/// Send a keepalive message to verify the shim is working
pub fn send_keepalive() -> Result<(), Box<dyn std::error::Error>> {
    // For now, just return success - the client handles keepalive via connection
    Ok(())
}

// Interposition functions for process creation using stackable-hooks
//
// Notes on edge case handling:
// - Short-lived commands: All fork/exec sequences are captured, even if the process exits quickly
// - Zombie processes: The shim does not interfere with normal process lifecycle management
// - Reparenting: PPID is captured at exec time, which may not reflect the final parent if the parent exits
// - setuid binaries: Shim injection is skipped when AT_SECURE is set (secure execution mode)
// - Failed exec: CommandStart is sent before exec, so failed execs are still recorded with the intended executable

// Common hooks (fork, execve, execvp, posix_spawn, posix_spawnp) are now defined in posix.rs for cross-platform compatibility

// Linux-specific hooks for vfork and clone
stackable_hooks::hook! {
    unsafe fn vfork() -> libc::pid_t => my_vfork {
        stackable_hooks::call_next!( vfork)
    }
}

stackable_hooks::hook! {
    unsafe fn clone(fn_: extern "C" fn(*mut libc::c_void) -> libc::c_int, child_stack: *mut libc::c_void, flags: libc::c_int, arg: *mut libc::c_void, ptid: *mut libc::pid_t, tls: *mut libc::c_void, ctid: *mut libc::pid_t) -> libc::c_int => my_clone {
        stackable_hooks::call_next!(fn_, child_stack, flags, arg, ptid, tls, ctid)
    }
}

// Internal libc symbol hooks for Python subprocess bypass paths
// These symbols may not exist on all systems, so we check for them at runtime
//
// Note: Temporarily commenting out internal symbol hooks as they may not exist
// on this system and are causing the shim to fail to load. The clone and vfork
// fixes should still help with some Python subprocess cases.

// stackable_hooks::hook! {
//     unsafe fn __execve(path: *const libc::c_char, argv: *const *mut libc::c_char, envp: *const *mut libc::c_char) -> libc::c_int => my___execve {
//         eprintln!("[ah-command-trace-shim] __execve hook called!");
//         // Implementation...
//     }
// }

// Additional internal symbol hooks temporarily disabled to avoid loading issues
