// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Command Trace Protocol — SSZ-based communication protocol
//!
//! This crate defines the message schemas for command trace events,
//! used by the command trace shim and recorder for lossless capture
//! of shell command execution and output streams.
//!
//! Messages are serialized using genuine SSZ binary protocol.

pub mod messages;

// Re-export key types
pub use messages::{
    CommandChunk, CommandEnd, CommandEvent, CommandStart, CommandTraceMessage, HandshakeMessage,
    HandshakeResponse, Request, Response,
};

// SSZ encoding/decoding functions for interpose communication
pub fn encode_ssz(data: &impl ssz::Encode) -> Vec<u8> {
    data.as_ssz_bytes()
}

pub fn decode_ssz<T: ssz::Decode>(data: &[u8]) -> Result<T, ssz::DecodeError> {
    T::from_ssz_bytes(data)
}
