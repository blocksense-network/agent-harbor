// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// Implementation of `ah agent record` command
//
// Spawns a command under a PTY, captures output to .ahr file format,
// and provides a basic viewer for monitoring the session.

use crate::terminal::{self, TerminalConfig};
use crate::view::TuiDependencies;
use crate::view_model::autocomplete::AutocompleteDependencies;
use crate::view_model::session_viewer_model::{GutterConfig, GutterPosition, SessionViewerMsg};
use crate::viewer::{
    ViewerConfig, build_session_viewer_view_model, handle_mouse_click_for_view,
    launch_task_from_instruction, render_view_frame, update_row_metadata_with_autofollow,
};
use ah_core::task_manager_wire::TaskManagerMessage;
use ah_recorder::{
    AhrWriter, PtyEvent, PtyRecorder, PtyRecorderConfig, RecordingSession, TerminalState,
    WriterConfig,
};
use ah_rest_api_contract::types::SessionEvent;
use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{Event, MouseButton, MouseEventKind};
use ratatui;
use std::env;
// no std::os::fd items needed currently
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use tokio::io::AsyncWriteExt;
use tokio::net::{UnixStream, unix::OwnedWriteHalf};
use tokio::{signal, sync::mpsc};
use tracing::error;
use tracing::{debug, info, trace, warn};

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
async fn send_sse_event(sender: &mut Option<OwnedWriteHalf>, event: SessionEvent) -> Result<()> {
    if let Some(sender) = sender {
        let msg = TaskManagerMessage::SessionEvent(event);
        msg.write_to(sender).await?;
    } else {
        debug!("send_sse_event: no sender available, event not sent");
    }
    Ok(())
}

/// Detect agent activity patterns in PTY output and send corresponding events
/// This is a basic heuristic implementation that can be enhanced with more sophisticated detection
async fn detect_and_send_agent_events(events_sender: &mut Option<OwnedWriteHalf>, buffer: &str) {
    use ah_rest_api_contract::types::SessionEvent;

    // Look for tool usage patterns (very basic heuristics)
    // This would be enhanced with actual agent protocol integration

    // Check for error patterns
    if buffer.contains("error") || buffer.contains("Error") || buffer.contains("ERROR") {
        // Extract error message (basic heuristic)
        let error_message = buffer.trim().to_string();
        let event = SessionEvent::error(error_message, chrono::Utc::now().timestamp() as u64);
        debug!("Sending error event via task manager socket: {:?}", event);
        if let Err(e) = send_sse_event(events_sender, event).await {
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
                if let Err(e) = send_sse_event(events_sender, event).await {
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

        let session_status = match ah_domain_types::ToolStatus::Started {
            ah_domain_types::ToolStatus::Started => {
                ah_rest_api_contract::SessionToolStatus::Started
            }
            ah_domain_types::ToolStatus::Completed => {
                ah_rest_api_contract::SessionToolStatus::Completed
            }
            ah_domain_types::ToolStatus::Failed => ah_rest_api_contract::SessionToolStatus::Failed,
        };
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
        if let Err(e) = send_sse_event(events_sender, event).await {
            warn!("Failed to send tool use event: {}", e);
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
    debug!("Starting agent record command, headless={}", args.headless);
    debug!("Command: {}, args: {:?}", args.command, args.args);
    info!(
        command = %args.command,
        args = ?args.args,
        "Starting recording session"
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

    // Set up terminal for TUI mode (if not headless)
    let running = Arc::new(AtomicBool::new(true));
    if !args.headless {
        let mut config = TerminalConfig::minimal();
        config.install_signal_handlers = true;
        config.running_flag = Some(running.clone());
        terminal::setup_terminal(config)
            .map_err(|e| anyhow::anyhow!("Failed to setup terminal: {}", e))?;
    }

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

    // Determine output file path
    let out_file = args.out_file;

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

    // Calculate display area for PTY (the PTY should match what will be displayed)
    let gutter_config = cli_gutter_to_viewer_gutter(&args.gutter, args.line_numbers);
    let display_cols = outer_cols.saturating_sub(4).saturating_sub(gutter_config.width() as u16); // horizontal padding + gutter
    let display_rows = outer_rows.saturating_sub(1); // status bar

    info!(
        "Display area for PTY: {}x{} (from terminal {}x{})",
        display_cols, display_rows, outer_cols, outer_rows
    );

    // Create PTY recorder configuration (PTY uses display area dimensions)
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
        terminal_cols = outer_cols,
        terminal_rows = outer_rows,
        "Output configuration"
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

    // Set up IPC server for snapshot notifications (always needed for UI interaction)
    let (ipc_temp_dir, socket_path, ipc_server, mut ipc_rx) =
        setup_recorder_ipc_socket(current_byte_offset.clone()).await?;

    info!("IPC server started, socket: {:?}", socket_path);
    debug!(
        "AH_RECORDER_IPC_SOCKET set to: {:?}",
        env::var("AH_RECORDER_IPC_SOCKET")
    );

    // Channel for inbound inject messages coming from task manager socket
    let (tm_inject_tx, mut tm_inject_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    // Set up task manager socket for event streaming and coordination
    let mut events_sender: Option<OwnedWriteHalf> = if let Some(task_manager_socket) =
        args.task_manager_socket.as_ref()
    {
        debug!(
            "Setting up task manager socket for event streaming, path: {}",
            task_manager_socket
        );
        // Connect to the task manager socket
        match UnixStream::connect(task_manager_socket).await {
            Ok(stream) => {
                // First, send the session ID (length-prefixed)
                let session_id = args.session_id.as_ref().unwrap_or(&"unknown".to_string()).clone();
                let session_id_bytes = session_id.as_bytes();
                let session_id_len = session_id_bytes.len() as u32;
                let len_bytes = session_id_len.to_le_bytes();

                let (mut reader, mut writer) = stream.into_split();

                if let Err(e) = writer.write_all(&len_bytes).await {
                    warn!(
                        "Failed to send session ID length to task manager socket: {}",
                        e
                    );
                    None
                } else if let Err(e) = writer.write_all(session_id_bytes).await {
                    warn!("Failed to send session ID to task manager socket: {}", e);
                    None
                } else {
                    info!("Task manager event streaming socket established");

                    // Spawn listener for inbound inject messages
                    let inject_tx = tm_inject_tx.clone();
                    tokio::spawn(async move {
                        loop {
                            match TaskManagerMessage::read_from(&mut reader).await {
                                Ok(TaskManagerMessage::InjectInput(bytes)) => {
                                    let _ = inject_tx.send(bytes);
                                }
                                Ok(TaskManagerMessage::SessionEvent(_))
                                | Ok(TaskManagerMessage::PtyData(_))
                                | Ok(TaskManagerMessage::PtyResize(_)) => {
                                    // Recorder should not receive these; ignore
                                }
                                Err(e) => {
                                    warn!("Task manager socket read failed: {}", e);
                                    break;
                                }
                            }
                        }
                    });

                    Some(writer)
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

    // Give IPC server time to start accepting connections
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Spawn command in PTY
    let (mut recorder, rx) = PtyRecorder::spawn(&args.command, &args.args, pty_config.clone())
        .context("Failed to spawn command in PTY")?;

    info!("Command spawned, starting capture");

    // Send initial status event if SSE streaming is enabled
    if events_sender.is_some() {
        use ah_domain_types::TaskState;
        use ah_rest_api_contract::types::SessionEvent;
        let status_event = SessionEvent::status(
            TaskState::Running.into(),
            chrono::Utc::now().timestamp() as u64,
        );
        debug!(
            "Sending initial status event via task manager socket: {:?}",
            status_event
        );
        if let Err(e) = send_sse_event(&mut events_sender, status_event).await {
            warn!("Failed to send initial status event: {}", e);
        }
    }

    // Create recording terminal state for accurate snapshot positioning (if not headless)
    // Use the display area dimensions (display_cols/display_rows account for gutter and UI elements)
    let recording_terminal_state = if !args.headless {
        Some(std::rc::Rc::new(std::cell::RefCell::new(
            TerminalState::new_with_scrollback(display_rows, display_cols, 1_000_000),
        )))
    } else {
        None
    };

    // Extract the PTY writer for direct use
    let pty_writer = recorder.take_writer();
    debug!("Extracted PTY writer: {}", pty_writer.is_some());

    // Create recording session
    let (recorder_handle, child_killer) = recorder.start_capture_and_get_killer();
    debug!("About to create RecordingSession");
    let mut session = RecordingSession::new(
        pty_writer,
        recorder_handle,
        child_killer,
        rx,
        writer,
        recording_terminal_state.clone(),
    );
    debug!("RecordingSession created");

    // Set up write_reply callback on TerminalState for DSR responses
    if let Some(ref rts) = recording_terminal_state {
        let pty_writer_clone = session.pty_writer.clone();
        rts.borrow_mut().set_write_reply(std::sync::Arc::new(move |bytes: &[u8]| {
            if let Ok(mut writer_opt) = pty_writer_clone.lock() {
                if let Some(ref mut writer) = *writer_opt {
                    let _ = writer.write_all(bytes);
                    let _ = writer.flush();
                }
            }
        }));
    }

    // Create local task manager for instruction-based task creation
    let task_manager =
        ah_core::create_session_viewer_task_manager().expect("Failed to create local task manager");

    // Set up viewer if not in headless mode
    let mut viewer_setup = if !args.headless {
        info!("Setting up live viewer");

        // Create viewer configuration (viewer gets full terminal size)
        let viewer_config = ViewerConfig {
            terminal_cols: outer_cols,
            terminal_rows: outer_rows,
            scrollback: 1_000_000, // Match the terminal state scrollback
            gutter: gutter_config,
            is_replay_mode: false,
        };

        let view_model = build_session_viewer_view_model(
            recording_terminal_state.clone().unwrap(),
            &viewer_config,
            Some(autocomplete_dependencies.clone()),
        );

        // Set up terminal for rendering
        let mut config = TerminalConfig::minimal();
        debug!("Setting up terminal");
        config.install_signal_handlers = false; // We already set up signal handlers above
        config.mouse_capture = true; // Enable mouse capture for scrolling
        terminal::setup_terminal(config)
            .map_err(|e| anyhow::anyhow!("Failed to setup terminal: {}", e))?;
        let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
        debug!("Creating terminal");
        let terminal = ratatui::Terminal::new(backend)?;

        Some((view_model, viewer_config, terminal))
    } else {
        // Drop the receiver since we won't use it in headless mode
        None
    };

    // Set up input event handling (always available for responsiveness)
    let mut input_rx = {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
        // Event reader thread
        debug!("Setting up input event handling");
        thread::spawn(move || {
            while let Ok(ev) = crossterm::event::read() {
                // Send event to main thread (async)
                let _ = tx.send(ev);
            }
        });
        rx
    };

    debug!("Input event handling set up");

    // Set up signal handling for graceful shutdown
    let _sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
        .context("Failed to set up signal handler")?;
    let _sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
        .context("Failed to set up signal handler")?;

    debug!("Signal handling set up");

    // Main event loop - process events sequentially through unified pipeline
    let mut exited = false;
    let mut exit_confirmation_armed = false;

    // Buffer for accumulating PTY data to detect agent activity patterns
    let mut pty_data_buffer = String::new();

    // Main event loop - handle PTY events, IPC events, user input, and always re-render
    loop {
        trace!("Main loop tick start");

        // Check for shutdown signal first
        if !running.load(Ordering::SeqCst) {
            info!("Received shutdown signal, terminating session");
            if let Err(e) = session.kill_child() {
                error!("Failed to kill child process: {}", e);
            }
            break;
        }

        // Handle all events in unified tokio select (truly event-driven)
        // Includes periodic tick for potential UI animations (similar to dashboard_loop.rs)
        tokio::select! {
            // Handle user input events (always available for responsiveness)
            input_event = input_rx.recv() => {
                let event = match input_event {
                    Some(e) => e,
                    None => {
                        debug!("Input event channel closed");
                        break;
                    }
                };


                // Only process input events if we have a viewer
                if let Some((view_model, viewer_config, terminal)) = &mut viewer_setup {
                    match event {
                        Event::Key(key) => {
                            // Debug logging for key events
                            debug!(
                                key_code = ?key.code,
                                modifiers = ?key.modifiers,
                                key_kind = ?key.kind,
                                focus_element = ?view_model.focus_element,
                                "Key event received in recorder"
                            );

                            // Clear exit confirmation on any non-ESC key
                            if !matches!(key.code, crossterm::event::KeyCode::Esc) {
                                exit_confirmation_armed = false;
                            }

                            if key.code == crossterm::event::KeyCode::Esc {
                                if view_model.task_entry_visible {
                                    view_model.cancel_instruction_overlay();
                                    continue;
                                }

                                if view_model.search_state.is_some() {
                                    view_model.exit_search();
                                    exit_confirmation_armed = false;
                                    continue;
                                }

                                if exit_confirmation_armed {
                                    // Second ESC - exit
                                    info!("ESC pressed again, exiting");
                                    if let Err(e) = session.kill_child() {
                                        error!("Failed to kill child process: {}", e);
                                    }
                                    exited = true;
                                    break;
                                } else {
                                    // First ESC - arm confirmation
                                    info!("ESC pressed, arming exit confirmation");
                                    exit_confirmation_armed = true;
                                    continue;
                                }
                            }

                            if view_model.task_entry_visible {
                                if view_model.handle_instruction_key(&key) {
                                    continue;
                                }

                                if key.code == crossterm::event::KeyCode::Enter {
                                    if let Some(instruction) = view_model.instruction_text() {
                                        let recording_state = view_model.recording_terminal_state.clone();
                                        view_model.cancel_instruction_overlay();
                                        launch_task_from_instruction(
                                            recording_state,
                                            Arc::clone(&task_manager),
                                            instruction,
                                            &view_model.task_entry.selected_agents,
                                        )
                                        .await;
                                    }
                                }

                                continue;
                            }

                            // First try view model's keyboard operation handling
                            let msgs = view_model.update(SessionViewerMsg::Key(key));
                            if !msgs.is_empty() {
                                // View model handled the key
                                continue;
                            }

                            // Forward unhandled key events to the PTY for interactive input
                            let app_cursor_mode = session.term_features().app_cursor_1;
                            if let Some(key_bytes) = terminal::key_event_to_bytes_with_features(&key, app_cursor_mode) {
                                debug!(?key_bytes, app_cursor_mode, "Writing key input to PTY");
                                // write_input writes directly to PTY
                                let _ = session.write_input(&key_bytes);
                            }
                        }
                        Event::Mouse(mouse_event) => {
                            // Handle mouse events if needed
                            match mouse_event.kind {
                                MouseEventKind::Down(MouseButton::Left) => {
                                    // Handle mouse clicks in viewer
                                    handle_mouse_click_for_view(
                                        view_model,
                                        viewer_config,
                                        mouse_event.column,
                                        mouse_event.row,
                                    );
                                }
                                MouseEventKind::ScrollUp => {
                                    // Handle mouse scroll up
                                    let _ = view_model.update(crate::view_model::session_viewer_model::SessionViewerMsg::MouseScrollUp);
                                }
                                MouseEventKind::ScrollDown => {
                                    // Handle mouse scroll down
                                    let _ = view_model.update(crate::view_model::session_viewer_model::SessionViewerMsg::MouseScrollDown);
                                }
                                _ => {}
                            }
                        }
                        Event::Resize(_width, _height) => {
                            // Handle resize events
                            let _ = terminal.autoresize();
                        }
                        _ => {}
                    }
                }
            }
            // Handle injected bytes coming from task manager socket (e.g., ACP prompt injection)
            injected = tm_inject_rx.recv() => {
                if let Some(bytes) = injected {
                    debug!("Received injected input from task manager ({} bytes)", bytes.len());
                    let _ = session.write_input(&bytes);
                } else {
                    debug!("Task manager inject channel closed");
                }
            }
            // Process PTY events directly from the session
            pty_event = session.next_event() => {
                match pty_event {
                    Some(PtyEvent::Exit { code }) => {
                        info!(?code, "Process exited");

                        // Send exit status event if SSE streaming is enabled
                        if events_sender.is_some() {
                            use ah_rest_api_contract::types::SessionEvent;
                            use ah_domain_types::TaskState;
                            let status = if code == Some(0) {
                                TaskState::Completed
                            } else {
                                TaskState::Failed
                            };
                            let status_event = SessionEvent::status(
                                status.into(),
                                chrono::Utc::now().timestamp() as u64,
                            );
                            if let Err(e) = send_sse_event(&mut events_sender, status_event).await {
                                warn!("Failed to send exit status event: {}", e);
                            }
                        }

                        exited = true;
                        break;
                    }
                    Some(PtyEvent::Error(err)) => {
                        tracing::error!(error = %err, "PTY error");
                    }
                    Some(pty_event) => {
                        // Process PTY event through the pipeline: AHR writer -> TerminalState
                        if let Err(e) = session.process_pty_event(pty_event.clone()) {
                            error!(error = %e, "Failed to process PTY event");
                        }

                        // Extract data for activity detection if SSE streaming is enabled
                        if let Some(sender) = events_sender.as_mut() {
                            match &pty_event {
                                PtyEvent::Data(data) => {
                                    // forward raw PTY bytes over task manager socket for followers
                                    let _ = TaskManagerMessage::PtyData(data.clone()).write_to(sender).await;

                                    // Try to decode as UTF-8 and accumulate for heuristics
                                    if let Ok(text) = std::str::from_utf8(data) {
                                        pty_data_buffer.push_str(text);

                                        // Look for agent activity patterns (basic heuristics)
                                        detect_and_send_agent_events(&mut events_sender, &pty_data_buffer).await;

                                        // Limit buffer size to prevent unbounded growth
                                        if pty_data_buffer.len() > 10000 {
                                            // Keep only the last 5000 characters
                                            let keep_len = 5000;
                                            if pty_data_buffer.len() > keep_len {
                                                pty_data_buffer = pty_data_buffer.chars().rev().take(keep_len).collect::<String>().chars().rev().collect();
                                            }
                                        }
                                    }
                                }
                                PtyEvent::Resize { cols, rows } => {
                                    let resize = TaskManagerMessage::PtyResize((*cols, *rows));
                                    let _ = resize.write_to(sender).await;
                                }
                                _ => {}
                            }
                        }
                    }
                    None => {
                        debug!("PTY event channel closed");
                        break;
                    }
                }
            }
            // Process IPC snapshot commands
            ipc_cmd = ipc_rx.recv() => {
                match ipc_cmd {
                    Some(ah_recorder::ipc::IpcCommand::Snapshot { snapshot_id, label, response_tx }) => {
                        let ts_ns = ah_recorder::now_ns();
                        debug!("Processing snapshot notification: id={}, label={}", snapshot_id, label);
                        info!(snapshot_id, label = %label, "Processing snapshot notification");

                        // Process snapshot through the pipeline: AHR writer -> TerminalState -> viewer
                        if let Err(e) = session.process_snapshot_event(snapshot_id, Some(&label), ts_ns) {
                            error!(error = %e, "Failed to process snapshot event");
                        }

                        // Get current byte offset for IPC response
                        let current_offset = session.current_byte_offset();

                        // Send response with anchor byte
                        debug!("Sending IPC response: snapshot_id={}, anchor_byte={}", snapshot_id, current_offset);
                        let _ = response_tx.send(ah_recorder::ipc::Response::Success((
                            snapshot_id,
                            current_offset,
                            ts_ns,
                        )));
                    }
                    Some(ah_recorder::ipc::IpcCommand::Shutdown) => {
                        info!("Received IPC shutdown command");
                        break;
                    }
                    None => {
                        debug!("IPC command channel closed");
                        break;
                    }
                }
            }
            // Periodic tick for UI animations (similar to dashboard_loop.rs ~60 FPS)
            // This enables smooth animations, blinking cursors, etc.
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(16)) => {
                // Tick occurred - could be used for animations if needed
                // For now, we just ensure the UI stays responsive
            }
        }

        // Always re-render viewer after handling any event
        if let Some((view_model, viewer_config, terminal)) = &mut viewer_setup {
            update_row_metadata_with_autofollow(view_model, viewer_config);
            let recorded_dims = view_model.recording_dims();
            if let Err(e) = terminal.draw(|f| {
                render_view_frame(
                    f,
                    view_model,
                    viewer_config,
                    exit_confirmation_armed,
                    recorded_dims,
                );
            }) {
                error!("Failed to render viewer: {}", e);
            }
        }
    }

    // Shutdown IPC server
    info!("Shutting down IPC server");
    ipc_server.shutdown().await;

    // Finalize recording
    info!("Finalizing recording");
    session.finalize().await.context("Failed to finalize recording")?;

    // Viewer cleanup is handled automatically when it goes out of scope

    // Clean up IPC temp directory (only if it was created)
    drop(ipc_temp_dir);

    // Clean up terminal state
    if !args.headless {
        terminal::cleanup_terminal();
    }

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
#[allow(clippy::disallowed_methods)]
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
#[allow(clippy::disallowed_methods)]
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
#[allow(clippy::disallowed_methods)]
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
