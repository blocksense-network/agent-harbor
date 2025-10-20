// Implementation of `ah agent record` command
//
// Spawns a command under a PTY, captures output to .ahr file format,
// and provides a basic viewer for monitoring the session.

use ah_recorder::viewer::GutterPosition;
use ah_recorder::{
    AhrWriter, PtyRecorder, PtyRecorderConfig, RecordingSession, TerminalViewer, ViewerConfig,
    ViewerEventLoop, WriterConfig, now_ns,
};
use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::{signal, spawn};
use tracing::error;
use tracing::{debug, info};

/// Position of the snapshot indicator gutter
#[derive(clap::ValueEnum, Clone, Debug, Default)]
pub enum CliGutterPosition {
    #[default]
    Right,
    Left,
    None,
}

/// Convert CLI gutter position to viewer gutter position
fn cli_gutter_to_viewer_gutter(cli_pos: &CliGutterPosition) -> GutterPosition {
    match cli_pos {
        CliGutterPosition::Left => GutterPosition::Left,
        CliGutterPosition::Right => GutterPosition::Right,
        CliGutterPosition::None => GutterPosition::None,
    }
}

/// Record an agent session with PTY capture
#[derive(Parser, Debug)]
pub struct RecordArgs {
    /// Command to execute
    #[arg(required = true)]
    pub command: String,

    /// Arguments to pass to the command
    #[arg(trailing_var_arg = true)]
    pub args: Vec<String>,

    /// Environment variable to set (can be specified multiple times)
    /// Format: KEY=VALUE
    #[arg(long = "env", value_name = "KEY=VALUE")]
    pub env_vars: Vec<String>,

    /// Output file path for .ahr recording (default: auto-generated)
    #[arg(short, long)]
    pub out_file: Option<PathBuf>,

    /// Brotli compression quality (0-11, default: 4)
    #[arg(long, default_value = "4")]
    pub brotli_q: u32,

    /// Terminal columns (default: current terminal width)
    #[arg(long)]
    pub cols: Option<u16>,

    /// Terminal rows (default: current terminal height)
    #[arg(long)]
    pub rows: Option<u16>,

    /// Disable live viewer (headless mode)
    #[arg(long)]
    pub headless: bool,

    /// Position of the snapshot indicator gutter column
    #[arg(long, default_value = "right", value_enum)]
    pub gutter: CliGutterPosition,
}

/// Extract branch points from a recorded session
#[derive(Parser, Debug)]
pub struct BranchPointsArgs {
    /// Path to the .ahr recording file or session ID
    #[arg(value_name = "SESSION")]
    pub session: String,

    /// Output format: json, md, or csv
    #[arg(short, long, default_value = "json")]
    pub format: String,

    /// Include terminal lines in output (default: true)
    #[arg(long)]
    pub include_lines: Option<bool>,

    /// Include snapshot metadata in output (default: true)
    #[arg(long)]
    pub include_snapshots: Option<bool>,
}

/// Execute the record command
pub async fn execute(args: RecordArgs) -> Result<()> {
    info!(
        command = %args.command,
        args = ?args.args,
        "Starting recording session"
    );

    // Determine terminal size (use current terminal or defaults)
    let (cols, rows) = if let (Some(c), Some(r)) = (args.cols, args.rows) {
        (c, r)
    } else {
        // Try to get current terminal size
        match crossterm::terminal::size() {
            Ok((c, r)) => (c, r),
            Err(_) => {
                // Fallback to standard size
                debug!("Could not determine terminal size, using defaults");
                (80, 24)
            }
        }
    };

    // Determine output file path
    let out_file = if let Some(path) = args.out_file {
        path
    } else {
        // Generate a unique filename with timestamp
        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let filename = format!("recording-{}.ahr", timestamp);
        PathBuf::from(&filename)
    };

    info!(
        path = ?out_file,
        cols = cols,
        rows = rows,
        "Output configuration"
    );

    // Parse environment variables
    let env_vars = args
        .env_vars
        .iter()
        .map(|env_var| {
            let parts: Vec<&str> = env_var.splitn(2, '=').collect();
            if parts.len() != 2 {
                anyhow::bail!(
                    "Invalid environment variable format: {}. Expected KEY=VALUE",
                    env_var
                );
            }
            Ok((parts[0].to_string(), parts[1].to_string()))
        })
        .collect::<Result<Vec<(String, String)>>>()?;

    // Create PTY recorder configuration
    let pty_config = PtyRecorderConfig {
        cols,
        rows,
        env_vars,
        ..Default::default()
    };

    // Create AHR writer
    let writer_config = WriterConfig::default().with_brotli_quality(args.brotli_q);
    let writer =
        AhrWriter::create(&out_file, writer_config).context("Failed to create AHR writer")?;

    // Set up IPC server for snapshot notifications
    use ah_recorder::ipc::{IpcServer, IpcServerConfig};
    use std::env;
    use tempfile::tempdir;

    // Create temporary directory for IPC socket
    let ipc_temp_dir = tempdir().context("Failed to create temp directory for IPC")?;
    let socket_path = ipc_temp_dir.path().join("recorder.sock");

    let ipc_config = IpcServerConfig {
        socket_path: socket_path.clone(),
    };

    // Set environment variable so external commands know how to connect
    env::set_var(
        "AH_RECORDER_IPC_SOCKET",
        socket_path.to_string_lossy().to_string(),
    );

    // Create shared byte offset counter for IPC
    let current_byte_offset = Arc::new(std::sync::atomic::AtomicU64::new(0));

    // Start IPC server
    let (ipc_server, mut ipc_rx) = IpcServer::start(ipc_config, current_byte_offset.clone())
        .await
        .context("Failed to start IPC server")?;

    info!("IPC server started, socket: {:?}", socket_path);
    eprintln!(
        "DEBUG: AH_RECORDER_IPC_SOCKET set to: {:?}",
        env::var("AH_RECORDER_IPC_SOCKET")
    );

    // Give IPC server time to start accepting connections
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Spawn command in PTY
    let (recorder, rx) = PtyRecorder::spawn(&args.command, &args.args, pty_config.clone())
        .context("Failed to spawn command in PTY")?;

    info!("Command spawned, starting capture");

    // Create recording session
    let recorder_handle = recorder.start_capture();
    let mut session = RecordingSession::new(recorder_handle, rx, writer, &pty_config);

    // Set up viewer if not in headless mode
    let viewer_handle = if !args.headless {
        info!("Setting up live viewer");

        // Create viewer configuration
        let viewer_config = ViewerConfig {
            cols,
            rows,
            scrollback: 1_000_000, // Match the terminal state scrollback
            gutter: cli_gutter_to_viewer_gutter(&args.gutter),
        };

        // Create terminal viewer with shared parser from recording session
        let viewer = TerminalViewer::new(session.terminal(), viewer_config);

        // Create event loop for the viewer
        let mut event_loop = ViewerEventLoop::new(viewer, Vec::new()) // Start with empty snapshots, will be updated dynamically
            .context("Failed to create viewer event loop")?;

        // Spawn viewer in background thread
        Some(tokio::spawn(async move {
            if let Err(e) = event_loop.run().await {
                error!("Viewer event loop failed: {}", e);
            }
        }))
    } else {
        None
    };

    // Set up signal handling for graceful shutdown
    let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
        .context("Failed to set up signal handler")?;
    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
        .context("Failed to set up signal handler")?;

    // Main event loop
    let mut exited = false;
    loop {
        tokio::select! {
            // Process PTY events
            event = session.process_event() => {
                match event {
                    Some(ah_recorder::PtyEvent::Exit { code }) => {
                        info!(?code, "Process exited");
                        exited = true;
                        break;
                    }
                    Some(ah_recorder::PtyEvent::Error(err)) => {
                        tracing::error!(error = %err, "PTY error");
                    }
                    Some(_) => {
                        // Data or resize events are handled internally
                    }
                    None => {
                        debug!("Event channel closed");
                        break;
                    }
                }
            }

            // Process IPC commands (snapshot notifications)
            ipc_cmd = ipc_rx.recv() => {
                match ipc_cmd {
                    Some(ah_recorder::ipc::IpcCommand::Snapshot { snapshot_id, label, response_tx }) => {
                        eprintln!("DEBUG: Processing IPC snapshot notification: id={}, label={}", snapshot_id, label);
                        info!(snapshot_id, label = ?label, "Processing snapshot notification");

                        // Get current byte offset from the session
                        let current_offset = session.current_byte_offset();

                        // Update the shared byte offset counter for IPC
                        current_byte_offset.store(current_offset, std::sync::atomic::Ordering::SeqCst);

                        // Write snapshot record to AHR file
                        let snapshot_record = ah_recorder::format::RecSnapshot {
                            header: ah_recorder::format::RecHeader {
                                tag: ah_recorder::format::REC_SNAPSHOT,
                                pad: [0; 3],
                                ts_ns: ah_recorder::now_ns(),
                            },
                            anchor_byte: current_offset,
                            snapshot_id,
                            label: label.clone(),
                        };
                        session.append_record(ah_recorder::Record::Snapshot(snapshot_record))
                            .context("Failed to write snapshot record to AHR")?;

                        // Send response with anchor byte
                        eprintln!("DEBUG: Sending IPC response: snapshot_id={}, anchor_byte={}", snapshot_id, current_offset);
                        let _ = response_tx.send(ah_recorder::ipc::Response::Success((
                            snapshot_id,
                            current_offset,
                            ah_recorder::now_ns(),
                        )));
                    }
                    Some(ah_recorder::ipc::IpcCommand::Shutdown) => {
                        info!("Received IPC shutdown command");
                        break;
                    }
                    None => {
                        debug!("IPC command channel closed");
                    }
                }
            }

            // Handle signals
            _ = sigint.recv() => {
                info!("Received SIGINT, shutting down");
                break;
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM, shutting down");
                break;
            }
        }
    }

    // Shutdown IPC server
    info!("Shutting down IPC server");
    ipc_server.shutdown().await;

    // Finalize recording
    info!("Finalizing recording");
    session.finalize().await.context("Failed to finalize recording")?;

    // Clean up viewer if it was running
    if let Some(viewer_handle) = viewer_handle {
        info!("Waiting for viewer to shut down");
        if let Err(e) = viewer_handle.await {
            error!("Viewer task failed: {}", e);
        }
    }

    // Clean up IPC temp directory
    drop(ipc_temp_dir);

    info!(ahr_file = ?out_file, "Recording complete");

    if !exited {
        anyhow::bail!("Process did not exit cleanly");
    }

    Ok(())
}

/// Execute the branch-points command
pub async fn execute_branch_points(args: BranchPointsArgs) -> Result<()> {
    info!(
        session = %args.session,
        format = %args.format,
        "Extracting branch points from recording"
    );

    // For now, assume the session argument is a path to an .ahr file
    let ahr_path = std::path::PathBuf::from(&args.session);

    if !ahr_path.exists() {
        anyhow::bail!("Recording file not found: {}", args.session);
    }

    // Create branch points from the recording
    let branch_points = ah_recorder::create_branch_points(&ahr_path, None)
        .context("Failed to create branch points from recording")?;

    // Debug: print information about the results
    eprintln!("DEBUG: AHR file: {:?}", ahr_path);
    eprintln!("DEBUG: Found {} branch points", branch_points.items.len());

    // Output in the requested format
    match args.format.as_str() {
        "json" => output_json(&branch_points)?,
        "md" => output_markdown(&branch_points)?,
        "csv" => output_csv(&branch_points)?,
        _ => {
            anyhow::bail!("Unsupported output format: {}", args.format);
        }
    }

    Ok(())
}

/// Output branch points in JSON format
fn output_json(branch_points: &ah_recorder::BranchPointsResult) -> Result<()> {
    use serde_json::json;

    let items: Vec<serde_json::Value> = branch_points
        .items
        .iter()
        .map(|item| match item {
            ah_recorder::InterleavedItem::Line(line) => {
                json!({
                    "kind": "line",
                    "index": line.index,
                    "text": line.text,
                    "last_write_byte": line.last_write_byte
                })
            }
            ah_recorder::InterleavedItem::Snapshot(snapshot) => {
                json!({
                    "kind": "snapshot",
                    "id": snapshot.id,
                    "ts_ns": snapshot.ts_ns,
                    "label": snapshot.label,
                    "anchor_byte": snapshot.anchor_byte
                })
            }
        })
        .collect();

    let output = json!({
        "total_bytes": branch_points.total_bytes,
        "items": items
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

/// Output branch points in Markdown format
fn output_markdown(branch_points: &ah_recorder::BranchPointsResult) -> Result<()> {
    println!("# Branch Points");
    println!("Total bytes processed: {}", branch_points.total_bytes);
    println!();

    for item in &branch_points.items {
        match item {
            ah_recorder::InterleavedItem::Line(line) => {
                println!(
                    "**Line {}** (byte {}): {}",
                    line.index, line.last_write_byte, line.text
                );
            }
            ah_recorder::InterleavedItem::Snapshot(snapshot) => {
                let label = snapshot.label.as_deref().unwrap_or("unnamed");
                println!(
                    "📸 **Snapshot {}** (byte {}): {}",
                    snapshot.id, snapshot.anchor_byte, label
                );
            }
        }
    }

    Ok(())
}

/// Output branch points in CSV format
fn output_csv(branch_points: &ah_recorder::BranchPointsResult) -> Result<()> {
    // CSV header
    println!("kind,index_or_id,position,text_or_label");

    for item in &branch_points.items {
        match item {
            ah_recorder::InterleavedItem::Line(line) => {
                // Escape quotes in text
                let escaped_text = line.text.replace("\"", "\"\"");
                println!(
                    "line,{},{},\"{}\"",
                    line.index, line.last_write_byte, escaped_text
                );
            }
            ah_recorder::InterleavedItem::Snapshot(snapshot) => {
                let label = snapshot.label.as_deref().unwrap_or("");
                let escaped_label = label.replace("\"", "\"\"");
                println!(
                    "snapshot,{},{},\"{}\"",
                    snapshot.id, snapshot.anchor_byte, escaped_label
                );
            }
        }
    }

    Ok(())
}
