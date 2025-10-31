// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS Daemon - Production-ready filesystem daemon with interpose support
//!
//! This crate provides the main AgentFS daemon that handles interpose requests from
//! filesystem clients. It acts as a library that can be embedded in executables or
//! used as a standalone daemon.

pub mod daemon;
pub mod handshake;
pub mod watch_service;

// Re-export the main daemon types
pub use daemon::AgentFsDaemon;
pub use watch_service::{WatchService, WatchServiceEventSink};

// Re-export handshake types for clients
pub use handshake::*;

// SSZ encoding/decoding utilities
use ssz::{Decode, Encode};

/// Encode a message using SSZ
pub fn encode_ssz_message(data: &impl Encode) -> Vec<u8> {
    data.as_ssz_bytes()
}

/// Decode a message from SSZ bytes
pub fn decode_ssz_message<T: Decode>(data: &[u8]) -> Result<T, ssz::DecodeError> {
    T::from_ssz_bytes(data)
}
