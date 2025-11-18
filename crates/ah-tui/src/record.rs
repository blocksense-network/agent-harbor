// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// Implementation of `ah agent record` command
//
// Spawns a command under a PTY, captures output to .ahr file format,
// and provides a basic viewer for monitoring the session.

use crate::terminal::{self, TerminalConfig};
use crate::tui_runtime::{self, UiMsg};
use crate::view::TuiDependencies;
use crate::view_model::autocomplete::AutocompleteDependencies;
use crate::view_model::input::InputState;
use crate::view_model::session_viewer_model::{GutterConfig, GutterPosition, SessionViewerMsg};
use crate::viewer::{
    ViewerConfig, build_session_viewer_view_model, handle_mouse_click_for_view,
    launch_task_from_instruction, render_view_frame, update_row_metadata_with_autofollow,
};
use ah_core::{AgentExecutionConfig, local_task_manager::GenericLocalTaskManager};
use ah_mux::TmuxMultiplexer;
use ah_recorder::{
    AhrWriter, PtyEvent, PtyRecorder, PtyRecorderConfig, RecordingSession, TerminalState,
    WriterConfig, ipc,
};
use ah_rest_api_contract::types::SessionEvent;
use anyhow::{Context, Result};
use clap::Parser;
use crossbeam_channel as chan;
use crossterm::event::{Event, MouseButton, MouseEventKind};
use ratatui::{Terminal, backend::CrosstermBackend};
use ssz::Encode;
use std::env;
use std::os::fd::{FromRawFd, IntoRawFd};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::{signal, sync::mpsc};
use tracing::error;
use tracing::{debug, info, trace, warn};

/// UI message type for record functionality
pub type RecordUiMsg = UiMsg<SessionViewerMsg>;

/// Data needed to construct RecordState on UI thread (separated to avoid Send issues)
struct RecordInitData {
    recording_terminal_state: std::rc::Rc<std::cell::RefCell<ah_recorder::TerminalState>>,
    viewer_config: ViewerConfig,
    recording_session: RecordingSession,
    task_manager: Arc<dyn ah_core::TaskManager>,
    ipc_rx: mpsc::UnboundedReceiver<ipc::IpcCommand>,
    ipc_server: ipc::IpcServer,
    autocomplete_dependencies: Option<crate::view_model::autocomplete::AutocompleteDependencies>,
}

/// State for the recording session that needs to persist across event loop iterations
struct RecordState {
    view_model: crate::view_model::session_viewer_model::SessionViewerViewModel,
    viewer_config: ViewerConfig,
    recording_session: RecordingSession,
    task_manager: Arc<dyn ah_core::TaskManager>,
    ipc_rx: mpsc::UnboundedReceiver<ipc::IpcCommand>,
    ipc_server: ipc::IpcServer,
    events_sender: Option<UnixStream>,
    exit_confirmation_armed: bool,
    pty_data_buffer: String,
}

/// Position of the snapshot indicator gutter
#[derive(clap::ValueEnum, Clone, Debug, Default)]
pub enum CliGutterPosition {
    #[default]
    Right,
    Left,
    None,
}

/// Convert CLI gutter options to viewer gutter config
fn cli_gutter_to_viewer_gutter(
    cli_pos: &CliGutterPosition,
    show_line_numbers: bool,
) -> GutterConfig {
    let position = match cli_pos {
        CliGutterPosition::Left => GutterPosition::Left,
        CliGutterPosition::Right => GutterPosition::Right,
        CliGutterPosition::None => GutterPosition::None,
    };
    GutterConfig {
        position,
        show_line_numbers,
    }
}

/// Record an agent session with PTY capture
#[derive(Parser, Debug, Clone)]
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

    /// Output file path for .ahr recording (no file created if not specified)
    #[arg(short, long)]
    pub out_file: Option<PathBuf>,

    /// Brotli compression quality (0-11, default: 4)
    #[arg(long, default_value = "4")]
    pub brotli_q: u32,

    /// Target repository (filesystem path)
    #[arg(long)]
    pub repo: Option<String>,

    /// Terminal columns (default: current terminal width)
    #[arg(long)]
    pub cols: Option<u16>,

    /// Session ID for logging (optional, for debugging)
    #[arg(long)]
    pub session_id: Option<String>,

    /// Terminal rows (default: current terminal height)
    #[arg(long)]
    pub rows: Option<u16>,

    /// Disable live viewer (headless mode)
    #[arg(long)]
    pub headless: bool,

    /// Position of the snapshot indicator gutter column
    #[arg(long, default_value = "left", value_enum)]
    pub gutter: CliGutterPosition,

    /// Show line numbers in gutter
    #[arg(long, default_value_t = false)]
    pub line_numbers: bool,

    /// Path to the task manager socket for event streaming and coordination
    #[arg(long, value_name = "PATH")]
    pub task_manager_socket: Option<String>,
}

/// Extract branch points from a recorded session
#[derive(Parser, Debug, Clone)]
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

/// Send an SSE event through the unnamed pipe
async fn send_sse_event(sender: &mut UnixStream, event: SessionEvent) -> Result<()> {
    // Create a readable version of the event for logging
    let event_description = match &event {
        SessionEvent::Error(e) => {
            format!("Error({})", String::from_utf8_lossy(&e.message))
        }
        SessionEvent::Status(s) => {
            format!("Status({:?})", s.status)
        }
        SessionEvent::FileEdit(_) => "FileEdit".to_string(),
        SessionEvent::ToolUse(_) => "ToolUse".to_string(),
        SessionEvent::ToolResult(_) => "ToolResult".to_string(),
        SessionEvent::Log(_) => "Log".to_string(),
        SessionEvent::Thought(_) => "Thought".to_string(),
    };
    debug!(
        "send_sse_event: sending event via task manager socket: {}",
        event_description
    );
    // Serialize event to SSZ (using JSON encoding for compatibility)
    let ssz_bytes = event.as_ssz_bytes();
    // Length-prefix the SSZ data
    let len_bytes = (ssz_bytes.len() as u32).to_le_bytes();
    let mut data = len_bytes.to_vec();
    data.extend_from_slice(&ssz_bytes);

    // Send through socket
    sender.write_all(&data).await?;
    debug!("send_sse_event: sent SSZ-encoded SSE event successfully");
    Ok(())
}

/// Detect agent activity patterns in PTY output and send corresponding events
/// This is a basic heuristic implementation that can be enhanced with more sophisticated detection
async fn detect_and_send_agent_events(events_sender: Option<&mut UnixStream>, buffer: &str) {
    if let Some(sender) = events_sender {
        use ah_rest_api_contract::types::SessionEvent;

        // Look for tool usage patterns (very basic heuristics)
        // This would be enhanced with actual agent protocol integration

        // Check for error patterns
        if buffer.contains("error") || buffer.contains("Error") || buffer.contains("ERROR") {
            // Extract error message (basic heuristic)
            let error_message = buffer.trim().to_string();
            let event = SessionEvent::error(error_message, chrono::Utc::now().timestamp() as u64);
            debug!("Sending error event via task manager socket: {:?}", event);
            if let Err(e) = send_sse_event(sender, event).await {
                warn!("Failed to send error event: {}", e);
            }
        }

        // Check for file operation patterns
        if buffer.contains("writing to")
            || buffer.contains("created file")
            || buffer.contains("modified")
        {
            // Extract file path if possible (basic regex)
            if let Some(file_match) = buffer.rfind('/') {
                let start = &buffer[file_match..];
                if let Some(end) = start.find('\n').or_else(|| start.find(' ')) {
                    let file_path = &start[..end];
                    let event = SessionEvent::file_edit(
                        file_path.trim().to_string(),
                        0, // Would need more sophisticated parsing
                        0,
                        None,
                        chrono::Utc::now().timestamp() as u64,
                    );
                    debug!(
                        "Sending file edit event via task manager socket: {:?}",
                        event
                    );
                    if let Err(e) = send_sse_event(sender, event).await {
                        warn!("Failed to send file edit event: {}", e);
                    }
                }
            }
        }

        // Check for tool execution patterns
        if buffer.contains("running") || buffer.contains("executing") || buffer.contains("tool:") {
            // Extract tool name (basic heuristic)
            let tool_name = if buffer.contains("grep") {
                "grep"
            } else if buffer.contains("sed") {
                "sed"
            } else if buffer.contains("find") {
                "find"
            } else {
                "unknown_tool"
            };

            let session_status = ah_rest_api_contract::SessionToolStatus::Started;
            let event = SessionEvent::tool_use(
                tool_name.to_string(),
                "[]".to_string(), // Would need more sophisticated parsing - store as JSON string
                format!("tool_{}", chrono::Utc::now().timestamp()),
                session_status,
                chrono::Utc::now().timestamp() as u64,
            );
            debug!(
                "Sending tool use event via task manager socket: {:?}",
                event
            );
            if let Err(e) = send_sse_event(sender, event).await {
                warn!("Failed to send tool use event: {}", e);
            }
        }
    }
}

/// Attempt to resize the terminal window using Crossterm
/// This uses the SetSize command which may not work on all terminals
/// Note: Not all terminals support programmatic resizing
fn try_resize_terminal(rows: u16, cols: u16) -> Result<()> {
    use crossterm::{execute, terminal::SetSize};
    use std::io::stdout;

    // Attempt to resize the terminal
    let _ = execute!(stdout(), SetSize(cols, rows));

    // Give the terminal a moment to process the resize request
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Check if the resize actually worked
    match crossterm::terminal::size() {
        Ok((actual_cols, actual_rows)) => {
            if actual_cols == cols && actual_rows == rows {
                debug!("Terminal successfully resized to {}x{}", cols, rows);
            } else {
                warn!(
                    "Terminal resize requested ({}x{}) but current size is {}x{}. \
                     This may happen if the terminal doesn't support programmatic resizing \
                     or if resizing is disabled.",
                    cols, rows, actual_cols, actual_rows
                );
            }
        }
        Err(e) => {
            debug!("Could not verify terminal size after resize attempt: {}", e);
        }
    }

    Ok(())
}

/// Set up IPC socket for recorder when recording to a file
/// Returns the IPC directory, socket path, server, and receiver
async fn setup_recorder_ipc_socket(
    current_byte_offset: Arc<std::sync::atomic::AtomicU64>,
) -> Result<(
    tempfile::TempDir,
    std::path::PathBuf,
    ah_recorder::ipc::IpcServer,
    mpsc::UnboundedReceiver<ah_recorder::ipc::IpcCommand>,
)> {
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

    // Start IPC server
    let (ipc_server, ipc_rx) = IpcServer::start(ipc_config, current_byte_offset)
        .await
        .context("Failed to start IPC server")?;

    Ok((ipc_temp_dir, socket_path, ipc_server, ipc_rx))
}

/// Execute the record command
pub async fn execute(deps: TuiDependencies, args: RecordArgs) -> Result<()> {
    if args.headless {
        return execute_headless(deps, args).await;
    }

    // For viewer mode, use the shared TUI runtime
    execute_with_viewer(deps, args)
}

/// Execute recording in headless mode (no TUI)
async fn execute_headless(deps: TuiDependencies, args: RecordArgs) -> Result<()> {
    debug!("Starting agent record command in headless mode");
    debug!("Command: {}, args: {:?}", args.command, args.args);
    info!(
        command = %args.command,
        args = ?args.args,
        "Starting headless recording session"
    );

    if let Some(ref session_id) = args.session_id {
        info!("Session ID: {}", session_id);
    }

    // Set up signal handling for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let _sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
        .context("Failed to set up signal handler")?;
    let _sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
        .context("Failed to set up signal handler")?;

    // Run headless recording without TUI
    run_headless_recording(deps, args, running).await
}

/// Run the actual headless recording logic
async fn run_headless_recording(
    deps: TuiDependencies,
    args: RecordArgs,
    running: Arc<AtomicBool>,
) -> Result<()> {
    // Set up basic terminal configuration for headless mode
    // We still need some terminal setup for PTY creation
    let terminal_config = TerminalConfig {
        install_signal_handlers: false, // We handle signals manually
        mouse_capture: false,
        raw_mode: false, // Don't need raw mode in headless
        running_flag: Some(running.clone()),
        alternate_screen: false,
        keyboard_enhancement: false,
    };
    terminal::setup_terminal(terminal_config)
        .map_err(|e| anyhow::anyhow!("Failed to setup terminal for headless mode: {}", e))?;

    // Determine terminal size (use current terminal or defaults)
    let (outer_cols, outer_rows) = if let (Some(c), Some(r)) = (args.cols, args.rows) {
        // Explicit dimensions provided - try to resize terminal
        try_resize_terminal(r, c)?;
        info!("Requested terminal size: {}x{}", c, r);
        (c, r)
    } else {
        // Try to get current terminal size
        match crossterm::terminal::size() {
            Ok((c, r)) => {
                debug!("Detected terminal size: {}x{}", c, r);
                (c, r)
            }
            Err(_) => {
                // Fallback to standard size
                debug!("Could not determine terminal size, using defaults");
                (120, 40) // Use larger defaults to ensure display area is reasonable
            }
        }
    };

    info!(
        "Terminal size: {}x{} (full size passed to session viewer)",
        outer_cols, outer_rows
    );

    // For headless mode, we don't need display area calculations since there's no viewer
    let display_cols = outer_cols;
    let display_rows = outer_rows;

    // Determine output file path
    let out_file = if let Some(ref path) = args.out_file {
        Some(path.clone())
    } else {
        None
    };

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

    // Create PTY recorder configuration (PTY uses full terminal dimensions in headless mode)
    let pty_config = PtyRecorderConfig {
        cols: display_cols,
        rows: display_rows,
        env_vars,
        ..Default::default()
    };

    info!(
        path = ?out_file,
        pty_cols = display_cols,
        pty_rows = display_rows,
        "Headless recording configuration"
    );

    // Create AHR writer (only if output file is specified)
    let writer = if let Some(ref path) = out_file {
        let writer_config = WriterConfig::default().with_brotli_quality(args.brotli_q);
        Some(AhrWriter::create(path, writer_config).context("Failed to create AHR writer")?)
    } else {
        None
    };

    // Create shared byte offset counter for IPC
    let current_byte_offset = Arc::new(std::sync::atomic::AtomicU64::new(0));

    // Set up IPC server for snapshot notifications
    let (ipc_temp_dir, socket_path, ipc_server, mut ipc_rx) =
        setup_recorder_ipc_socket(current_byte_offset.clone()).await?;

    info!("IPC server started, socket: {:?}", socket_path);

    // Set up task manager socket for event streaming (if requested)
    let mut events_sender = if let Some(task_manager_socket) = args.task_manager_socket.as_ref() {
        debug!(
            "Setting up task manager socket for event streaming, path: {}",
            task_manager_socket
        );
        match UnixStream::connect(task_manager_socket).await {
            Ok(mut stream) => {
                let session_id = args.session_id.as_ref().unwrap_or(&"unknown".to_string()).clone();
                let session_id_bytes = session_id.as_bytes();
                let session_id_len = session_id_bytes.len() as u32;
                let len_bytes = session_id_len.to_le_bytes();

                if let Err(e) = stream.write_all(&len_bytes).await {
                    warn!("Failed to send session ID length: {}", e);
                    None
                } else if let Err(e) = stream.write_all(session_id_bytes).await {
                    warn!("Failed to send session ID: {}", e);
                    None
                } else {
                    info!("Task manager socket established");
                    Some(stream)
                }
            }
            Err(e) => {
                warn!(
                    "Failed to connect to task manager socket {}: {}",
                    task_manager_socket, e
                );
                None
            }
        }
    } else {
        debug!("No task manager socket requested");
        None
    };

    // Give IPC server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Spawn command in PTY
    let (mut recorder, rx) = PtyRecorder::spawn(&args.command, &args.args, pty_config.clone())
        .context("Failed to spawn command in PTY")?;

    info!("Command spawned, starting headless capture");

    // SSE events are sent from the interactive TUI event loop, not headless mode

    // For headless mode, we don't need TerminalState for rendering
    let recording_terminal_state = None;

    // Extract PTY writer
    let pty_writer = recorder.take_writer();

    // Create recording session
    let (recorder_handle, child_killer) = recorder.start_capture_and_get_killer();
    let mut session = RecordingSession::new(
        pty_writer,
        recorder_handle,
        child_killer,
        rx,
        writer,
        recording_terminal_state.clone(),
    );

    // Set up write_reply callback if we have terminal state (we don't in headless)
    // This is only needed for interactive DSR responses

    // Run headless event loop
    run_headless_event_loop(session, ipc_server, ipc_rx, events_sender, running, args).await?;

    // Clean up IPC
    drop(ipc_temp_dir);

    // Clean up terminal
    terminal::cleanup_terminal();

    Ok(())
}

/// Run the headless event loop for recording
async fn run_headless_event_loop(
    mut session: RecordingSession,
    ipc_server: ipc::IpcServer,
    mut ipc_rx: mpsc::UnboundedReceiver<ipc::IpcCommand>,
    mut events_sender: Option<UnixStream>,
    running: Arc<AtomicBool>,
    args: RecordArgs,
) -> Result<()> {
    let mut pty_data_buffer = String::new();

    loop {
        // Check for shutdown signal
        if !running.load(Ordering::SeqCst) {
            info!("Received shutdown signal, terminating session");
            if let Err(e) = session.kill_child() {
                error!("Failed to kill child process: {}", e);
            }
            break;
        }

        tokio::select! {
            // Handle PTY events
            pty_event = session.next_event() => {
                match pty_event {
                    Some(PtyEvent::Exit { code }) => {
                        info!(?code, "Process exited");

                        // Send exit status event if streaming enabled
                        if events_sender.is_some() {
                            use ah_rest_api_contract::types::SessionEvent;
                            use ah_domain_types::TaskState;
                            let status = if code == Some(0) {
                                TaskState::Completed
                            } else {
                                TaskState::Failed
                            };
                            // SSE events are only sent in interactive TUI mode
                        }

                        break;
                    }
                    Some(PtyEvent::Error(err)) => {
                        error!(error = %err, "PTY error");
                    }
                    Some(pty_event) => {
                        // Process PTY event
                        if let Err(e) = session.process_pty_event(pty_event.clone()) {
                            error!(error = %e, "Failed to process PTY event");
                        }

                        // Extract data for activity detection if streaming enabled
                        if events_sender.is_some() {
                            if let PtyEvent::Data(data) = &pty_event {
                                if let Ok(text) = std::str::from_utf8(data) {
                                    pty_data_buffer.push_str(text);
                                    detect_and_send_agent_events(events_sender.as_mut(), &pty_data_buffer).await;

                                    // Limit buffer size
                                    if pty_data_buffer.len() > 10000 {
                                        let keep_len = 5000;
                                        if pty_data_buffer.len() > keep_len {
                                            pty_data_buffer = pty_data_buffer.chars().rev().take(keep_len).collect::<String>().chars().rev().collect();
                                        }
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        debug!("PTY event channel closed");
                        break;
                    }
                }
            }

            // Handle IPC snapshot commands
            ipc_cmd = ipc_rx.recv() => {
                match ipc_cmd {
                    Some(ipc::IpcCommand::Snapshot { snapshot_id, label, response_tx }) => {
                        let ts_ns = ah_recorder::now_ns();
                        debug!("Processing snapshot: id={}, label={}", snapshot_id, label);

                        if let Err(e) = session.process_snapshot_event(snapshot_id, Some(&label), ts_ns) {
                            error!(error = %e, "Failed to process snapshot event");
                        }

                        let current_offset = session.current_byte_offset();
                        let _ = response_tx.send(ipc::Response::Success((
                            snapshot_id,
                            current_offset,
                            ts_ns,
                        )));
                    }
                    Some(ipc::IpcCommand::Shutdown) => {
                        info!("Received IPC shutdown command");
                        break;
                    }
                    None => {
                        debug!("IPC command channel closed");
                        break;
                    }
                }
            }

            // Periodic tick (not strictly needed in headless mode, but keeps the loop responsive)
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                // Just keep the loop running
            }
        }
    }

    // Shutdown IPC server
    ipc_server.shutdown().await;

    // Finalize recording
    session.finalize().await.context("Failed to finalize recording")?;

    info!(ahr_file = ?args.out_file, "Headless recording complete");

    Ok(())
}

/// Execute recording with live viewer using shared TUI runtime
fn execute_with_viewer(deps: TuiDependencies, args: RecordArgs) -> Result<()> {
    // Run using shared TUI runtime - all async initialization happens inside the closure
    tui_runtime::run_tui_with_single_tokio_thread::<SessionViewerMsg, _, _>(
        deps,
        move |deps, rx_ui, tx_ui, rx_tick, terminal, input_state| async move {
            debug!("Starting agent record command with viewer");
            debug!("Command: {}, args: {:?}", args.command, args.args);
            info!(
                command = %args.command,
                args = ?args.args,
                "Starting recording session with viewer"
            );

            if let Some(ref session_id) = args.session_id {
                info!("Session ID: {}", session_id);
            }

            // Create autocomplete dependencies from TuiDependencies
            let autocomplete_dependencies = Arc::new(AutocompleteDependencies {
                workspace_files: deps.workspace_files.clone(),
                workspace_workflows: deps.workspace_workflows.clone(),
                workspace_terms: deps.workspace_terms.clone(),
                settings: deps.settings.clone(),
            });

            // Determine terminal size
            let (outer_cols, outer_rows) = if let (Some(c), Some(r)) = (args.cols, args.rows) {
                try_resize_terminal(r, c)?;
                info!("Requested terminal size: {}x{}", c, r);
                (c, r)
            } else {
                match crossterm::terminal::size() {
                    Ok((c, r)) => {
                        debug!("Detected terminal size: {}x{}", c, r);
                        (c, r)
                    }
                    Err(_) => {
                        debug!("Could not determine terminal size, using defaults");
                        (120, 40)
                    }
                }
            };

            info!(
                "Terminal size: {}x{} (full size passed to session viewer)",
                outer_cols, outer_rows
            );

            // Calculate display area for PTY
            let gutter_config = cli_gutter_to_viewer_gutter(&args.gutter, args.line_numbers);
            let display_cols =
                outer_cols.saturating_sub(4).saturating_sub(gutter_config.width() as u16);
            let display_rows = outer_rows.saturating_sub(1);

            info!(
                "Display area for PTY: {}x{} (from terminal {}x{})",
                display_cols, display_rows, outer_cols, outer_rows
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
                cols: display_cols,
                rows: display_rows,
                env_vars,
                ..Default::default()
            };

            info!(
                pty_cols = display_cols,
                pty_rows = display_rows,
                terminal_cols = outer_cols,
                terminal_rows = outer_rows,
                "Output configuration"
            );

            // Create AHR writer
            let writer = if let Some(ref path) = args.out_file {
                let writer_config = WriterConfig::default().with_brotli_quality(args.brotli_q);
                Some(
                    AhrWriter::create(path, writer_config)
                        .context("Failed to create AHR writer")?,
                )
            } else {
                None
            };

            // Create shared byte offset counter for IPC
            let current_byte_offset = Arc::new(std::sync::atomic::AtomicU64::new(0));

            // Set up IPC server for snapshot notifications
            let (ipc_temp_dir, socket_path, ipc_server, ipc_rx) =
                setup_recorder_ipc_socket(current_byte_offset.clone()).await?;

            info!("IPC server started, socket: {:?}", socket_path);

            // Set up task manager socket for event streaming
            let events_sender = if let Some(task_manager_socket) = args.task_manager_socket.as_ref()
            {
                debug!(
                    "Setting up task manager socket for event streaming, path: {}",
                    task_manager_socket
                );
                match UnixStream::connect(task_manager_socket).await {
                    Ok(mut stream) => {
                        let session_id =
                            args.session_id.as_ref().unwrap_or(&"unknown".to_string()).clone();
                        let session_id_bytes = session_id.as_bytes();
                        let session_id_len = session_id_bytes.len() as u32;
                        let len_bytes = session_id_len.to_le_bytes();

                        if let Err(e) = stream.write_all(&len_bytes).await {
                            warn!("Failed to send session ID length: {}", e);
                            None
                        } else if let Err(e) = stream.write_all(session_id_bytes).await {
                            warn!("Failed to send session ID: {}", e);
                            None
                        } else {
                            info!("Task manager socket established");
                            Some(stream)
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to connect to task manager socket {}: {}",
                            task_manager_socket, e
                        );
                        None
                    }
                }
            } else {
                debug!("No task manager socket requested");
                None
            };

            // Give IPC server time to start
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // Spawn command in PTY
            let (mut recorder, rx) =
                PtyRecorder::spawn(&args.command, &args.args, pty_config.clone())
                    .context("Failed to spawn command in PTY")?;

            info!("Command spawned, starting capture");

            // Create recording terminal state for accurate snapshot positioning
            let recording_terminal_state = std::rc::Rc::new(std::cell::RefCell::new(
                TerminalState::new_with_scrollback(display_rows, display_cols, 1_000_000),
            ));

            // Extract PTY writer
            let pty_writer = recorder.take_writer();

            // Create recording session
            let (recorder_handle, child_killer) = recorder.start_capture_and_get_killer();
            let recording_session = RecordingSession::new(
                pty_writer,
                recorder_handle,
                child_killer,
                rx,
                writer,
                Some(recording_terminal_state.clone()),
            );

            // Set up write_reply callback on TerminalState for DSR responses
            {
                let pty_writer_clone = recording_session.pty_writer.clone();
                recording_terminal_state.borrow_mut().set_write_reply(std::sync::Arc::new(
                    move |bytes: &[u8]| {
                        if let Ok(mut writer_opt) = pty_writer_clone.lock() {
                            if let Some(ref mut writer) = *writer_opt {
                                let _ = writer.write_all(bytes);
                                let _ = writer.flush();
                            }
                        }
                    },
                ));
            }

            // Create task manager
            let task_manager: Arc<dyn ah_core::TaskManager> =
                ah_core::create_session_viewer_task_manager()
                    .expect("Failed to create local task manager");

            // Create viewer configuration
            let viewer_config = ViewerConfig {
                terminal_cols: outer_cols,
                terminal_rows: outer_rows,
                scrollback: 1_000_000,
                gutter: gutter_config,
                is_replay_mode: false,
            };

            let view_model = build_session_viewer_view_model(
                recording_terminal_state,
                &viewer_config,
                Some(autocomplete_dependencies),
            );

            // Construct the record state with captured components
            let record_state = RecordState {
                view_model,
                viewer_config,
                recording_session,
                task_manager,
                ipc_rx,
                ipc_server,
                events_sender,
                exit_confirmation_armed: false,
                pty_data_buffer: String::new(),
            };
            run_record_event_loop(
                record_state,
                rx_ui,
                tx_ui,
                rx_tick,
                terminal,
                input_state,
                args,
            )
            .await
        },
    )
    .map_err(|e| anyhow::anyhow!("{}", e))
}

/// Run the recording event loop using the shared TUI runtime
async fn run_record_event_loop(
    mut record_state: RecordState,
    mut rx_ui: chan::Receiver<RecordUiMsg>,
    _tx_ui: chan::Sender<RecordUiMsg>,
    mut rx_tick: chan::Receiver<std::time::Instant>,
    mut terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    mut input_state: InputState,
    args: RecordArgs,
) -> Result<(), anyhow::Error> {
    loop {
        // Use biased select to prefer UI messages over ticks
        chan::select_biased! {
            recv(rx_ui) -> ui_msg => {
                let ui_msg = match ui_msg {
                    Ok(msg) => msg,
                    Err(_) => break,
                };

                match ui_msg {
                    RecordUiMsg::UserInput(event) => {
                        handle_record_user_input_event(
                            &mut record_state,
                            &mut input_state,
                            &mut terminal,
                            event,
                        ).await?;
                    }
                    RecordUiMsg::Tick => {
                        handle_record_tick_event(&mut record_state, &mut terminal).await?;
                    }
                    RecordUiMsg::AppMsg(session_viewer_msg) => {
                        handle_record_session_viewer_message(
                            &mut record_state,
                            &mut terminal,
                            session_viewer_msg,
                        ).await?;
                    }
                }

                // Check for exit
                if record_state.view_model.exit_requested {
                    break;
                }
            }
            recv(rx_tick) -> _ => {
                handle_record_tick_event(&mut record_state, &mut terminal).await?;

                if record_state.view_model.exit_requested {
                    break;
                }
            }
        }
    }

    // Shutdown IPC server
    info!("Shutting down IPC server");
    record_state.ipc_server.shutdown().await;

    // Finalize recording
    info!("Finalizing recording");
    record_state
        .recording_session
        .finalize()
        .await
        .context("Failed to finalize recording")?;

    info!(ahr_file = ?args.out_file, "Recording complete");
    Ok(())
}

/// Handle user input events for recording
async fn handle_record_user_input_event(
    record_state: &mut RecordState,
    input_state: &mut InputState,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    event: Event,
) -> Result<(), anyhow::Error> {
    match event {
        Event::Key(key) => {
            debug!(
                key_code = ?key.code,
                modifiers = ?key.modifiers,
                key_kind = ?key.kind,
                focus_element = ?record_state.view_model.focus_element,
                "Key event received in recorder"
            );

            // Clear exit confirmation on any non-ESC key
            if !matches!(key.code, crossterm::event::KeyCode::Esc) {
                record_state.exit_confirmation_armed = false;
            }

            if key.code == crossterm::event::KeyCode::Esc {
                if record_state.view_model.task_entry_visible {
                    record_state.view_model.cancel_instruction_overlay();
                    return Ok(());
                }

                if record_state.view_model.search_state.is_some() {
                    record_state.view_model.exit_search();
                    record_state.exit_confirmation_armed = false;
                    return Ok(());
                }

                if record_state.exit_confirmation_armed {
                    info!("ESC pressed again, exiting");
                    if let Err(e) = record_state.recording_session.kill_child() {
                        error!("Failed to kill child process: {}", e);
                    }
                    record_state.view_model.exit_requested = true;
                    return Ok(());
                } else {
                    info!("ESC pressed, arming exit confirmation");
                    record_state.exit_confirmation_armed = true;
                    return Ok(());
                }
            }

            if record_state.view_model.task_entry_visible {
                if record_state.view_model.handle_instruction_key(&key) {
                    return Ok(());
                }

                if key.code == crossterm::event::KeyCode::Enter {
                    if let Some(instruction) = record_state.view_model.instruction_text() {
                        let recording_state =
                            record_state.view_model.recording_terminal_state.clone();
                        record_state.view_model.cancel_instruction_overlay();
                        launch_task_from_instruction(
                            recording_state,
                            Arc::clone(&record_state.task_manager),
                            instruction,
                            &record_state.view_model.task_entry.selected_agents,
                        )
                        .await;
                    }
                }
                return Ok(());
            }

            // Try view model's keyboard operation handling
            let msgs = record_state.view_model.update(SessionViewerMsg::Key(key.clone()));
            if !msgs.is_empty() {
                return Ok(());
            }

            // Forward unhandled key events to the PTY for interactive input
            let app_cursor_mode = record_state.recording_session.term_features().app_cursor_1;
            if let Some(key_bytes) =
                crate::terminal::key_event_to_bytes_with_features(&key, app_cursor_mode)
            {
                debug!(?key_bytes, app_cursor_mode, "Writing key input to PTY");
                let _ = record_state.recording_session.write_input(&key_bytes);
            }
        }
        Event::Mouse(mouse_event) => match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                handle_mouse_click_for_view(
                    &mut record_state.view_model,
                    &record_state.viewer_config,
                    mouse_event.column,
                    mouse_event.row,
                );
            }
            MouseEventKind::ScrollUp => {
                let _ = record_state.view_model.update(SessionViewerMsg::MouseScrollUp);
            }
            MouseEventKind::ScrollDown => {
                let _ = record_state.view_model.update(SessionViewerMsg::MouseScrollDown);
            }
            _ => {}
        },
        Event::Resize(_width, _height) => {
            let _ = terminal.autoresize();
        }
        _ => {}
    }
    Ok(())
}

/// Handle tick events for recording
async fn handle_record_tick_event(
    record_state: &mut RecordState,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), anyhow::Error> {
    // Handle PTY events
    while let Some(pty_event) = record_state.recording_session.next_event().await {
        match pty_event {
            PtyEvent::Exit { code } => {
                info!(?code, "Process exited");

                if let Some(ref mut sender) = record_state.events_sender {
                    use ah_domain_types::TaskState;
                    use ah_rest_api_contract::types::SessionEvent;
                    let status = if code == Some(0) {
                        TaskState::Completed
                    } else {
                        TaskState::Failed
                    };
                    let status_event =
                        SessionEvent::status(status.into(), chrono::Utc::now().timestamp() as u64);
                    if let Err(e) = send_sse_event(sender, status_event).await {
                        warn!("Failed to send exit status event: {}", e);
                    }
                }

                record_state.view_model.exit_requested = true;
                return Ok(());
            }
            PtyEvent::Error(err) => {
                error!(error = %err, "PTY error");
            }
            PtyEvent::Data(ref data) => {
                if let Err(e) = record_state.recording_session.process_pty_event(pty_event.clone())
                {
                    error!(error = %e, "Failed to process PTY event");
                }

                // Extract data for activity detection if streaming enabled
                if let Some(ref mut sender) = record_state.events_sender {
                    if let Ok(text) = std::str::from_utf8(&data) {
                        record_state.pty_data_buffer.push_str(text);
                        detect_and_send_agent_events(Some(sender), &record_state.pty_data_buffer)
                            .await;

                        // Limit buffer size
                        if record_state.pty_data_buffer.len() > 10000 {
                            let keep_len = 5000;
                            if record_state.pty_data_buffer.len() > keep_len {
                                record_state.pty_data_buffer = record_state
                                    .pty_data_buffer
                                    .chars()
                                    .rev()
                                    .take(keep_len)
                                    .collect::<String>()
                                    .chars()
                                    .rev()
                                    .collect();
                            }
                        }
                    }
                }
            }
            _ => {
                if let Err(e) = record_state.recording_session.process_pty_event(pty_event) {
                    error!(error = %e, "Failed to process PTY event");
                }
            }
        }
    }

    // Handle IPC commands
    while let Ok(ipc_cmd) = record_state.ipc_rx.try_recv() {
        match ipc_cmd {
            ipc::IpcCommand::Snapshot {
                snapshot_id,
                label,
                response_tx,
            } => {
                let ts_ns = ah_recorder::now_ns();
                debug!("Processing snapshot: id={}, label={}", snapshot_id, label);

                if let Err(e) = record_state.recording_session.process_snapshot_event(
                    snapshot_id,
                    Some(&label),
                    ts_ns,
                ) {
                    error!(error = %e, "Failed to process snapshot event");
                }

                let current_offset = record_state.recording_session.current_byte_offset();
                let _ =
                    response_tx.send(ipc::Response::Success((snapshot_id, current_offset, ts_ns)));
            }
            ipc::IpcCommand::Shutdown => {
                info!("Received IPC shutdown command");
                record_state.view_model.exit_requested = true;
                return Ok(());
            }
        }
    }

    // Always re-render viewer
    update_row_metadata_with_autofollow(&mut record_state.view_model, &record_state.viewer_config);
    let recorded_dims = record_state.view_model.recording_dims();
    terminal.draw(|f| {
        render_view_frame(
            f,
            &mut record_state.view_model,
            &record_state.viewer_config,
            record_state.exit_confirmation_armed,
            recorded_dims,
        );
    })?;

    Ok(())
}

/// Handle session viewer messages for recording
async fn handle_record_session_viewer_message(
    record_state: &mut RecordState,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    msg: SessionViewerMsg,
) -> Result<(), anyhow::Error> {
    // Handle session viewer messages if needed
    let _msgs = record_state.view_model.update(msg);
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
    debug!("AHR file: {:?}", ahr_path);
    debug!("Found {} branch points", branch_points.items.len());

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
        .enumerate()
        .map(|(idx, item)| match item {
            ah_recorder::InterleavedItem::Line(line) => {
                json!({
                    "kind": "line",
                    "index": idx,
                    "text": line
                })
            }
            ah_recorder::InterleavedItem::Snapshot(snapshot) => {
                json!({
                    "kind": "snapshot",
                    "ts_ns": snapshot.ts_ns,
                    "label": snapshot.label,
                    "anchor_byte": snapshot.anchor_byte,
                    "line": snapshot.line.as_usize(),
                    "column": snapshot.column.as_usize()
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

    for (idx, item) in branch_points.items.iter().enumerate() {
        match item {
            ah_recorder::InterleavedItem::Line(line) => {
                println!("**Line {}**: {}", idx, line);
            }
            ah_recorder::InterleavedItem::Snapshot(snapshot) => {
                let label = snapshot.label.as_deref().unwrap_or("unnamed");
                println!(
                    "📸 **Snapshot {}**: {} (line {}, column {})",
                    snapshot.anchor_byte,
                    label,
                    snapshot.line.as_usize() + 1,
                    snapshot.column.as_usize() + 1
                );
            }
        }
    }

    Ok(())
}

/// Output branch points in CSV format
fn output_csv(branch_points: &ah_recorder::BranchPointsResult) -> Result<()> {
    // CSV header
    println!("kind,index_or_id,text_or_label,line,column");

    for (idx, item) in branch_points.items.iter().enumerate() {
        match item {
            ah_recorder::InterleavedItem::Line(line) => {
                // Escape quotes in text
                let escaped_text = line.replace("\"", "\"\"");
                println!("line,{},\"{}\",,", idx, escaped_text);
            }
            ah_recorder::InterleavedItem::Snapshot(snapshot) => {
                let label = snapshot.label.as_deref().unwrap_or("");
                let escaped_label = label.replace("\"", "\"\"");
                println!(
                    "snapshot,{},\"{}\",{},{}",
                    snapshot.anchor_byte,
                    escaped_label,
                    snapshot.line.as_usize() + 1,
                    snapshot.column.as_usize() + 1
                );
            }
        }
    }

    Ok(())
}
