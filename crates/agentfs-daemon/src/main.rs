// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS Daemon executable - thin wrapper around the library

use agentfs_core::{FsConfig, config::BackstoreMode};
use agentfs_daemon::AgentFsDaemon;
use agentfs_proto::{Request, Response};
use ssz::{Decode, Encode};
use std::env;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

fn encode_ssz_message(data: &impl Encode) -> Vec<u8> {
    data.as_ssz_bytes()
}

fn decode_ssz_message<T: Decode>(data: &[u8]) -> Result<T, ssz::DecodeError> {
    T::from_ssz_bytes(data)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let socket_path = if args.len() > 1 {
        args[1].clone()
    } else {
        std::env::var("AGENTFS_DAEMON_SOCKET")
            .unwrap_or_else(|_| "/tmp/agentfs-daemon.sock".to_string())
    };

    // Create daemon (uses in-memory backend by default)
    let daemon = AgentFsDaemon::new()?;

    // Remove socket if it exists
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)?;
    println!("AgentFS Daemon listening on {}", socket_path);

    loop {
        let (mut socket, _) = listener.accept().await?;
        let daemon_clone = daemon.clone();

        tokio::spawn(async move {
            let mut buf = [0u8; 65536];

            loop {
                let n = match socket.read(&mut buf).await {
                    Ok(0) => return, // Connection closed
                    Ok(n) => n,
                    Err(_) => return,
                };

                // Try to decode the message
                match decode_ssz_message::<Request>(&buf[..n]) {
                    Ok(request) => match daemon_clone.handle_watch_request(&request) {
                        Ok(response) => {
                            let response_bytes = encode_ssz_message(&response);
                            let _ = socket.write_all(&response_bytes).await;
                        }
                        Err(e) => {
                            eprintln!("Failed to handle request: {}", e);
                        }
                    },
                    Err(e) => {
                        eprintln!("Failed to decode request: {:?}", e);
                    }
                }
            }
        });
    }
}
