// Implementation of `ah agent record` command
//
// Spawns a command under a PTY, captures output to .ahr file format,
// and provides a basic viewer for monitoring the session.

use ah_recorder::{
    now_ns, AhrWriter, PtyRecorder, PtyRecorderConfig, RecordingSession,
    WriterConfig, create_shared_writer,
};
use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;
use tokio::signal;
use tracing::{debug, info};

/// Record an agent session with PTY capture
#[derive(Parser, Debug)]
pub struct RecordArgs {
    /// Command to execute
    #[arg(required = true)]
    pub command: String,

    /// Arguments to pass to the command
    #[arg(trailing_var_arg = true)]
    pub args: Vec<String>,

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

    // Create PTY recorder configuration
    let pty_config = PtyRecorderConfig {
        cols,
        rows,
        ..Default::default()
    };

    // Create AHR writer
    let writer_config = WriterConfig::default().with_brotli_quality(args.brotli_q);
    let writer = AhrWriter::create(&out_file, writer_config)
        .context("Failed to create AHR writer")?;

    // Create snapshots writer
    let snapshots_path = out_file.with_extension("snapshots.jsonl");
    let snapshots_writer = create_shared_writer(&snapshots_path)
        .context("Failed to create snapshots writer")?;

    // Spawn command in PTY
    let (recorder, rx) = PtyRecorder::spawn(&args.command, &args.args, pty_config.clone())
        .context("Failed to spawn command in PTY")?;

    info!("Command spawned, starting capture");

    // Create recording session
    let recorder_handle = recorder.start_capture();
    let mut session = RecordingSession::new(recorder_handle, rx, writer, &pty_config);

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

    // Finalize recording
    info!("Finalizing recording");
    session.finalize().await.context("Failed to finalize recording")?;

    // Finalize snapshots
    let writer = std::sync::Arc::try_unwrap(snapshots_writer)
        .map_err(|_| anyhow::anyhow!("Snapshots writer still has references"))?
        .into_inner()
        .unwrap();
    writer.finalize().context("Failed to finalize snapshots")?;

    info!(
        ahr_file = ?out_file,
        snapshots_file = ?snapshots_path,
        "Recording complete"
    );

    if !exited {
        anyhow::bail!("Process did not exit cleanly");
    }

    Ok(())
}
