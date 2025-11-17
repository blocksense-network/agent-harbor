// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! SSZ-based message types for command trace protocol

// Note: Using u32 for serialization instead of c_int to work with SSZ
use ssz_derive::{Decode, Encode};

// SSZ Union-based request/response types for type-safe communication
// Using Vec<u8> for strings as SSZ supports variable-length byte vectors

/// Request union - each variant contains operation-specific data
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum Request {
    /// Handshake request from shim
    Handshake(HandshakeMessage),
    /// Command start notification from shim
    CommandStart(CommandStart),
}

/// Response union - each variant contains operation-specific response data
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum Response {
    /// Handshake response from recorder
    Handshake(HandshakeResponse),
}

/// Top-level message union for command trace communication
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum CommandTraceMessage {
    /// Handshake request from shim
    HandshakeRequest(HandshakeMessage),
    /// Handshake response from recorder
    HandshakeResponse(HandshakeResponse),
    /// Command execution started
    CommandStart(CommandStart),
    /// Command output chunk (stdout/stderr)
    CommandChunk(CommandChunk),
    /// Command execution ended
    CommandEnd(CommandEnd),
}

/// Handshake message sent by shim on initialization
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct HandshakeMessage {
    /// Protocol version (e.g., "1.0")
    pub version: Vec<u8>,
    /// Process ID of the shim
    pub pid: u32,
    /// Platform identifier ("macos", "linux")
    pub platform: Vec<u8>,
}

/// Response to handshake (acknowledgement)
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct HandshakeResponse {
    /// Success flag
    pub success: bool,
    /// Optional error message if success is false
    pub error_message: Option<Vec<u8>>,
}

/// Command execution started event
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct CommandStart {
    /// Unique command ID assigned by recorder
    pub command_id: u64,
    /// Process ID of the command
    pub pid: u32,
    /// Parent process ID
    pub ppid: u32,
    /// Working directory path
    pub cwd: Vec<u8>,
    /// Executable path
    pub executable: Vec<u8>,
    /// Command line arguments (argv)
    pub args: Vec<Vec<u8>>,
    /// Environment variables (key=value pairs)
    pub env: Vec<Vec<u8>>,
    /// Timestamp when command started (nanoseconds since Unix epoch)
    pub start_time_ns: u64,
}

/// Command output chunk event
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct CommandChunk {
    /// Command ID this chunk belongs to
    pub command_id: u64,
    /// Stream type: 0=stdout, 1=stderr
    pub stream_type: u8,
    /// Sequence number within this stream (for ordering)
    pub sequence_no: u64,
    /// Raw output data
    pub data: Vec<u8>,
    /// PTY byte offset if available (for correlation with .ahr files)
    pub pty_offset: Option<u64>,
    /// Timestamp when chunk was captured (nanoseconds since Unix epoch)
    pub timestamp_ns: u64,
}

/// Command execution ended event
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct CommandEnd {
    /// Command ID that ended
    pub command_id: u64,
    /// Exit code (0 for success)
    pub exit_code: u32,
    /// Signal number if terminated by signal (0 otherwise)
    pub signal: u32,
    /// Timestamp when command ended (nanoseconds since Unix epoch)
    pub end_time_ns: u64,
}

/// Legacy enum for backward compatibility (used in domain types)
#[derive(Clone, Debug, PartialEq)]
pub enum CommandEvent {
    Start(CommandStart),
    Chunk(CommandChunk),
    End(CommandEnd),
}

impl From<CommandStart> for CommandEvent {
    fn from(event: CommandStart) -> Self {
        CommandEvent::Start(event)
    }
}

impl From<CommandChunk> for CommandEvent {
    fn from(event: CommandChunk) -> Self {
        CommandEvent::Chunk(event)
    }
}

impl From<CommandEnd> for CommandEvent {
    fn from(event: CommandEnd) -> Self {
        CommandEvent::End(event)
    }
}
