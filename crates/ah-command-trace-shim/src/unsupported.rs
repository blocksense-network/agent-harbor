// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Stub implementation for unsupported platforms

use crate::core;

/// Initialize the shim (no-op on unsupported platforms)
pub fn initialize_shim() {
    // Do nothing on unsupported platforms
    let _state = core::initialize_shim_state();
}

/// Check if the shim is enabled (always false on unsupported platforms)
pub fn is_shim_enabled() -> bool {
    false
}
