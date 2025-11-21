// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_logging::{Level, init_plaintext};
use anyhow::Result;
use clap::Parser;
use ssz::{Decode, Encode};
use std::path::PathBuf;
use tokio::signal::unix::{SignalKind, signal};
use tracing::{error, info};

mod operations;
mod server;
mod types;

use server::DaemonServer;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the Unix socket for listening
    #[arg(long, default_value = "/tmp/agent-harbor/ah-fs-snapshots-daemon")]
    socket_path: PathBuf,

    /// Run in stdin mode (read SSZ commands from stdin instead of socket)
    #[arg(long)]
    stdin_mode: bool,

    /// Log level (error, warn, info, debug, trace)
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging with ah_logging
    let level = match args.log_level.as_str() {
        "error" => Level::ERROR,
        "warn" => Level::WARN,
        "info" => Level::INFO,
        "debug" => Level::DEBUG,
        "trace" => Level::TRACE,
        _ => Level::INFO,
    };

    init_plaintext("ah-fs-snapshots-daemon", level)?;

    // Create a span that sets the component field for all log messages in this daemon
    let span = tracing::info_span!("daemon", component = "ah-fs-snapshots-daemon");
    let _enter = span.enter();

    info!("Starting AH filesystem snapshots daemon");

    if args.stdin_mode {
        info!("Running in stdin mode");
        run_stdin_mode().await?;
    } else {
        info!(operation = "start_daemon", socket_path = %args.socket_path.display(), "Running in socket mode");
        run_socket_mode(args.socket_path).await?;
    }

    Ok(())
}

async fn run_socket_mode(socket_path: PathBuf) -> Result<()> {
    let mut server = DaemonServer::new(socket_path)?;

    // Set up signal handlers for graceful shutdown
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    tokio::select! {
        result = server.run() => {
            if let Err(e) = result {
                error!(error = %e, "Server error");
                return Err(e);
            }
        }
        _ = sigint.recv() => {
            info!(operation = "shutdown", signal = "SIGINT", "Received SIGINT, shutting down");
            server.shutdown().await?;
        }
        _ = sigterm.recv() => {
            info!(operation = "shutdown", signal = "SIGTERM", "Received SIGTERM, shutting down");
            server.shutdown().await?;
        }
    }

    Ok(())
}

async fn run_stdin_mode() -> Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader, stdin};
    use types::Request;

    let stdin = BufReader::new(stdin());
    let mut lines = stdin.lines();

    while let Some(line) = lines.next_line().await? {
        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Parse SSZ-encoded request from hex string
        let request_bytes = hex::decode(&line)?;
        let request: Request = Request::from_ssz_bytes(&request_bytes).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("SSZ decode error: {:?}", e),
            )
        })?;

        // Process the request
        let response = operations::process_request(request).await;

        // Encode response as SSZ and output as hex
        let response_bytes = Encode::as_ssz_bytes(&response);
        tracing::info!(operation = "process_request", response_hex = %hex::encode(&response_bytes), "daemon response");
    }

    Ok(())
}
