// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Linux implementation using LD_PRELOAD with redhook

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
        if let Err(e) = posix::initialize_client() {
            eprintln!("[ah-command-trace-shim] Failed to initialize client: {}", e);
            // Update state to error
            *SHIM_STATE.get().unwrap().lock().unwrap() =
                ShimState::Error(format!("Client initialization failed: {}", e));
        }
    }
}

/// Check if the shim is enabled and ready
pub fn is_shim_enabled() -> bool {
    matches!(
        SHIM_STATE.get().and_then(|s| s.lock().ok()),
        Some(ref state) if matches!(**state, ShimState::Ready { .. })
    )
}

/// Send a keepalive message to verify the shim is working
pub fn send_keepalive() -> Result<(), Box<dyn std::error::Error>> {
    // For now, just return success - the client handles keepalive via connection
    Ok(())
}

// Interposition functions for process creation using redhook
//
// Notes on edge case handling:
// - Short-lived commands: All fork/exec sequences are captured, even if the process exits quickly
// - Zombie processes: The shim does not interfere with normal process lifecycle management
// - Reparenting: PPID is captured at exec time, which may not reflect the final parent if the parent exits
// - setuid binaries: Shim injection is skipped when AT_SECURE is set (secure execution mode)
// - Failed exec: CommandStart is sent before exec, so failed execs are still recorded with the intended executable

// Common hooks (fork, execve, execvp, posix_spawn, posix_spawnp) are now defined in posix.rs for redhook compatibility

// Linux-specific hooks for vfork and clone
redhook::hook! {
    unsafe fn vfork() -> libc::pid_t => my_vfork {
        let result = redhook::real!(vfork)();

        if result == 0 {
            // Child process - send CommandStart if we're tracking
            if let Some(ref state) = crate::core::SHIM_STATE.get().and_then(|s| s.lock().ok()) {
                if matches!(**state, crate::core::ShimState::Ready { .. }) {
                    // Get process info
                    let pid = std::process::id() as u32;
                    let ppid = unsafe { libc::getppid() } as u32;
                    let executable = std::env::current_exe()
                        .ok()
                        .and_then(|p| p.to_str().map(|s| s.as_bytes().to_vec()))
                        .unwrap_or_else(|| b"<unknown>".to_vec());

                    let args: Vec<Vec<u8>> = std::env::args()
                        .map(|s| s.into_bytes())
                        .collect();

                    let env = crate::posix::get_environment();
                    let cwd = crate::posix::get_current_dir();

                    crate::posix::send_command_start(pid, ppid, &executable, args, env, cwd);
                }
            }
        }

        result
    }
}

redhook::hook! {
    unsafe fn clone(fn_: extern "C" fn(*mut libc::c_void) -> libc::c_int, child_stack: *mut libc::c_void, flags: libc::c_int, arg: *mut libc::c_void, ptid: *mut libc::pid_t, tls: *mut libc::c_void, ctid: *mut libc::pid_t) -> libc::c_int => my_clone {
        let result = redhook::real!(clone)(fn_, child_stack, flags, arg, ptid, tls, ctid);

        // If this is a new process (not just a thread), send CommandStart
        if result > 0 && (flags & libc::CLONE_VM) == 0 {
            // Child process - send CommandStart if we're tracking
            if let Some(ref state) = crate::core::SHIM_STATE.get().and_then(|s| s.lock().ok()) {
                if matches!(**state, crate::core::ShimState::Ready { .. }) {
                    let pid = result as u32;
                    let ppid = std::process::id() as u32;
                    let executable = std::env::current_exe()
                        .ok()
                        .and_then(|p| p.to_str().map(|s| s.as_bytes().to_vec()))
                        .unwrap_or_else(|| b"<unknown>".to_vec());

                    let args: Vec<Vec<u8>> = std::env::args()
                        .map(|s| s.into_bytes())
                        .collect();

                    let env = crate::posix::get_environment();
                    let cwd = crate::posix::get_current_dir();

                    crate::posix::send_command_start(pid, ppid, &executable, args, env, cwd);
                }
            }
        }

        result
    }
}
