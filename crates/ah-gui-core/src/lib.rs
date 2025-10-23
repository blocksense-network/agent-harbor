// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent Harbor GUI Core - Native Addon
//!
//! This Rust crate provides native functionality for the Electron GUI
//! via N-API bindings. It serves as a foundation for process management,
//! file operations, and other native integrations.
#![deny(clippy::all)]

use napi::bindgen_prelude::*;
use napi_derive::napi;

/**
 * Simple "hello world" function to verify N-API integration
 *
 * This function demonstrates that the Rust native addon is correctly
 * loaded and callable from Node.js/Electron.
 *
 * @returns A greeting message from Rust
 */
#[napi]
pub fn hello_from_rust() -> Result<String> {
    Ok("Hello from Agent Harbor GUI Core (Rust)!".to_string())
}

/**
 * Get the current platform
 *
 * @returns The platform name (darwin, linux, win32)
 */
#[napi]
pub fn get_platform() -> Result<String> {
    Ok(std::env::consts::OS.to_string())
}

// Future additions will include:
// - WebUI process lifecycle management
// - File system operations
// - Native OS integrations
// - Performance-critical operations
