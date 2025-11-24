// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::server::get_log_directory;
use ah_logging::CliLogLevel;
use anyhow::Result;
use clap::Parser;
use ssz::{Decode, Encode};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal::unix::{SignalKind, signal};
use tracing::{debug, error, info};

mod fuse_manager;
mod interpose_manager;
mod operations;
mod server;
mod types;

use server::{DaemonServer, DaemonState};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the Unix socket for listening
    #[arg(long, default_value = "/tmp/agent-harbor/ah-fs-snapshots-daemon")]
    socket_path: PathBuf,

    /// Run in stdin mode (read SSZ commands from stdin instead of socket)
    #[arg(long)]
    stdin_mode: bool,

    #[command(flatten)]
    logging: ah_logging::CliLoggingArgs,
}

#[tokio::main]
async fn main() -> Result<()> {
    let Args {
        socket_path,
        stdin_mode,
        logging,
    } = Args::parse();

    // Initialize logging with ah_logging
    // Initialize logging - fs-snapshots-daemon is not a TUI app
    // It logs to console by default, or to file when --log-file/--log-dir is specified
    let log_level = logging.log_level.unwrap_or(CliLogLevel::Info);
    let log_level_str = log_level.to_string();
    let should_log_to_file = logging.log_file.is_some() || logging.log_dir.is_some();

    // Determine the log directory for agentfs-daemon (reuse the same logic)
    let log_dir_for_agentfs = get_log_directory();

    if should_log_to_file {
        let level = log_level.into();

        // Create component-specific log path in user's home directory (even when running as root)
        let mut log_path = log_dir_for_agentfs.clone();
        log_path.push("ah-fs-snapshots-daemon.log");

        ah_logging::init_to_file(
            "ah-fs-snapshots-daemon",
            level,
            ah_logging::LogFormat::Json,
            &log_path,
        )?;
        info!(operation = "logging_init", log_level = %log_level_str, log_format = "json", log_destination = %log_path.display(), "Logging initialized to file");
    } else {
        logging.init_with_default_level("ah-fs-snapshots-daemon", false, CliLogLevel::Info)?;
    }

    // Create a span that sets the component field for all log messages in this daemon
    let span = tracing::info_span!("daemon", component = "ah-fs-snapshots-daemon");
    let _enter = span.enter();

    info!("Starting AH filesystem snapshots daemon");
    debug!(
        operation = "daemon_startup",
        socket_path = %socket_path.display(),
        stdin_mode,
        "Daemon startup parameters"
    );

    let daemon_state = Arc::new(DaemonState::new(
        log_level_str,
        should_log_to_file,
        log_dir_for_agentfs,
    ));
    debug!(
        operation = "daemon_state_init",
        "Daemon state initialized successfully"
    );

    if stdin_mode {
        info!("Running in stdin mode");
        run_stdin_mode(daemon_state.clone()).await?;
    } else {
        info!(
            operation = "start_daemon",
            socket_path = %socket_path.display(),
            "Running in socket mode"
        );
        run_socket_mode(socket_path, daemon_state).await?;
    }

    Ok(())
}

async fn run_socket_mode(socket_path: PathBuf, state: Arc<DaemonState>) -> Result<()> {
    debug!(operation = "server_init", socket_path = %socket_path.display(), "Initializing daemon server");
    let mut server = DaemonServer::new(socket_path.clone(), state)?;
    debug!(operation = "server_created", socket_path = %socket_path.display(), "Daemon server created successfully");

    // Set up signal handlers for graceful shutdown
    debug!(
        operation = "signal_handlers_setup",
        "Setting up SIGINT and SIGTERM signal handlers"
    );
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    debug!(
        operation = "signal_handlers_ready",
        "Signal handlers configured successfully"
    );

    debug!(
        operation = "server_run_loop_start",
        "Entering server run loop with signal handling"
    );
    tokio::select! {
        result = server.run() => {
            if let Err(e) = result {
                error!(error = %e, operation = "server_run", "Server error during execution");
                return Err(e);
            } else {
                debug!(operation = "server_run", "Server run completed normally");
            }
        }
        _ = sigint.recv() => {
            info!(operation = "shutdown", signal = "SIGINT", "Received SIGINT, shutting down");
            debug!(operation = "shutdown_initiated", signal = "SIGINT", "Starting graceful shutdown procedure");
            server.shutdown().await?;
            debug!(operation = "shutdown_completed", signal = "SIGINT", "Graceful shutdown completed");
        }
        _ = sigterm.recv() => {
            info!(operation = "shutdown", signal = "SIGTERM", "Received SIGTERM, shutting down");
            debug!(operation = "shutdown_initiated", signal = "SIGTERM", "Starting graceful shutdown procedure");
            server.shutdown().await?;
            debug!(operation = "shutdown_completed", signal = "SIGTERM", "Graceful shutdown completed");
        }
    }

    Ok(())
}

async fn run_stdin_mode(state: Arc<DaemonState>) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader, stdin};
    use types::Request;

    debug!(
        operation = "stdin_mode_init",
        "Initializing stdin mode for SSZ command processing"
    );
    let stdin = BufReader::new(stdin());
    let mut lines = stdin.lines();
    debug!(
        operation = "stdin_reader_ready",
        "Stdin reader initialized and ready for input"
    );

    let mut request_count = 0;
    while let Some(line) = lines.next_line().await? {
        request_count += 1;
        let line_trimmed = line.trim();

        // Skip empty lines
        if line_trimmed.is_empty() {
            debug!(operation = "stdin_skip_empty", request_count = %request_count, "Skipping empty line");
            continue;
        }

        debug!(operation = "stdin_request_received", request_count = %request_count, line_length = %line.len(), "Received line from stdin");

        // Parse SSZ-encoded request from hex string
        let request_bytes = hex::decode(line_trimmed)?;
        debug!(operation = "stdin_hex_decode", request_count = %request_count, bytes_len = %request_bytes.len(), "Decoded hex string to bytes");

        let request: Request = Request::from_ssz_bytes(&request_bytes).map_err(|e| {
            debug!(operation = "stdin_ssz_decode_error", request_count = %request_count, error = ?e, "SSZ decode error");
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("SSZ decode error: {:?}", e),
            )
        })?;
        debug!(operation = "stdin_request_parsed", request_count = %request_count, "Successfully parsed SSZ request");

        // Process the request
        let response = state.process_request(request, "stdin-session".to_string()).await;
        debug!(operation = "stdin_request_processed", request_count = %request_count, "Request processed by daemon state");

        // Encode response as SSZ and output as hex
        let response_bytes = Encode::as_ssz_bytes(&response);
        tracing::info!(operation = "process_request", request_count = %request_count, response_hex = %hex::encode(&response_bytes), "daemon response");
        debug!(operation = "stdin_response_encoded", request_count = %request_count, response_bytes_len = %response_bytes.len(), "Response encoded as SSZ hex");
    }

    debug!(operation = "stdin_mode_complete", total_requests = %request_count, "Stdin mode processing completed");

    Ok(())
}
