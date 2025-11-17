// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! macOS implementation using DYLD interposition with redhook

use crate::core::{self, SHIM_STATE, ShimState};
use crate::posix;
use ctor::ctor;

/// Initialize the shim on library load
#[ctor]
fn initialize_shim() {
    eprintln!("[ah-command-trace-shim] Initializing macOS shim");

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

// macOS redhook hooks are defined in posix.rs for cross-platform compatibility
