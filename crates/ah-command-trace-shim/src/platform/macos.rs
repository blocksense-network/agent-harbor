// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! macOS implementation using DYLD interposition

use crate::core::{self, SHIM_STATE, ShimState};
use ah_command_trace_proto::{
    HandshakeMessage, HandshakeResponse, Request, Response, decode_ssz, encode_ssz,
};
use ctor::ctor;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Mutex;

/// Global handshake stream (kept alive for the process lifetime)
static HANDSHAKE_STREAM: Mutex<Option<UnixStream>> = Mutex::new(None);

/// Initialize the shim on library load
#[ctor]
fn initialize_shim() {
    eprintln!("[ah-command-trace-shim] Initializing macOS shim");
    let state = core::initialize_shim_state();

    // Store the state globally
    let _ = SHIM_STATE.set(Mutex::new(state.clone()));

    match &state {
        ShimState::Disabled => {
            // Shim disabled, do nothing
        }
        ShimState::Ready { socket_path, .. } => {
            eprintln!(
                "[ah-command-trace-shim] Ready state - socket: {}",
                socket_path
            );
            core::log_message(&state, "Initializing command trace shim");

            // Perform handshake synchronously - if it fails, the process will exit anyway
            eprintln!("[ah-command-trace-shim] Performing handshake...");
            match perform_handshake(&socket_path) {
                Ok(stream) => {
                    eprintln!("[ah-command-trace-shim] Handshake successful, shim ready");
                    *HANDSHAKE_STREAM.lock().unwrap() = Some(stream);
                }
                Err(e) => {
                    eprintln!("[ah-command-trace-shim] Handshake failed: {}", e);
                    // Update state to error
                    *SHIM_STATE.get().unwrap().lock().unwrap() =
                        ShimState::Error(format!("Handshake failed: {}", e));
                }
            }
        }
        ShimState::Error(msg) => {
            eprintln!("[ah-command-trace-shim] Initialization error: {}", msg);
        }
    }
}

/// Perform handshake with the recorder socket
fn perform_handshake(socket_path: &str) -> Result<UnixStream, Box<dyn std::error::Error>> {
    eprintln!(
        "[ah-command-trace-shim] Connecting to socket: {}",
        socket_path
    );
    eprintln!(
        "[ah-command-trace-shim] Socket exists: {}",
        std::path::Path::new(socket_path).exists()
    );

    let mut stream = match UnixStream::connect(socket_path) {
        Ok(stream) => {
            eprintln!("[ah-command-trace-shim] Connected to socket successfully");
            stream
        }
        Err(e) => {
            eprintln!("[ah-command-trace-shim] Failed to connect to socket: {}", e);
            return Err(e.into());
        }
    };

    // Create handshake message
    let handshake_msg = Request::Handshake(HandshakeMessage {
        version: b"1.0".to_vec(),
        pid: std::process::id(),
        platform: b"macos".to_vec(),
    });

    // Encode to SSZ
    let handshake_bytes = encode_ssz(&handshake_msg);

    // Send length-prefixed message
    let len_bytes = (handshake_bytes.len() as u32).to_le_bytes();
    stream.write_all(&len_bytes)?;
    stream.write_all(&handshake_bytes)?;

    // Read response (length + SSZ message)
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let response_len = u32::from_le_bytes(len_buf) as usize;

    let mut response_buf = vec![0u8; response_len];
    stream.read_exact(&mut response_buf)?;

    // Decode SSZ response
    let response_msg: Response =
        decode_ssz(&response_buf).map_err(|e| format!("SSZ decode error: {:?}", e))?;

    match response_msg {
        Response::Handshake(response) => {
            // For now, we just accept any handshake response
            // In the future, we might check for success/error
            if !response.success {
                return Err("Handshake failed".into());
            }
        }
    }

    // Keep the stream alive for future communication
    Ok(stream)
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
    let mut stream_guard = HANDSHAKE_STREAM.lock().unwrap();
    if let Some(ref mut stream) = *stream_guard {
        let keepalive = serde_json::json!({
            "type": "keepalive",
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        });

        let bytes = serde_json::to_vec(&keepalive)?;
        stream.write_all(&bytes)?;
        stream.write_all(b"\n")?;
        Ok(())
    } else {
        Err("No handshake stream available".into())
    }
}
