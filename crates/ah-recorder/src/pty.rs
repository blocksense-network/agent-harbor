// PTY management for spawning and capturing agent sessions
//
// Uses portable-pty for cross-platform PTY support and vt100 for terminal state tracking

use crate::format::{RecData, RecResize, Record};
use crate::writer::{now_ns, AhrWriter};
use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, trace, warn};

/// Configuration for PTY session recording
#[derive(Debug, Clone)]
pub struct PtyRecorderConfig {
    /// Initial terminal size
    pub cols: u16,
    pub rows: u16,
    /// Read buffer size for PTY output
    pub read_buffer_size: usize,
    /// Whether to capture input (not implemented in MVP)
    pub capture_input: bool,
}

impl Default for PtyRecorderConfig {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            read_buffer_size: 8192,
            capture_input: false,
        }
    }
}

/// Events emitted by the PTY recorder
#[derive(Debug, Clone)]
pub enum PtyEvent {
    /// Output data from the PTY
    Data(Vec<u8>),
    /// Terminal was resized
    Resize { cols: u16, rows: u16 },
    /// Child process exited
    Exit { code: Option<u32> },
    /// Error occurred
    Error(String),
}

/// PTY recorder that spawns a command under a PTY and captures output
///
/// The recorder runs in a background thread and sends events through a channel.
pub struct PtyRecorder {
    /// PTY master handle
    master: Box<dyn MasterPty + Send>,
    /// Child process handle
    child: Box<dyn Child + Send + Sync>,
    /// Configuration
    config: PtyRecorderConfig,
    /// Event sender
    tx: mpsc::UnboundedSender<PtyEvent>,
}

impl PtyRecorder {
    /// Spawn a command under a PTY with the given configuration
    ///
    /// Returns a receiver for PTY events and the recorder instance.
    pub fn spawn(
        cmd: &str,
        args: &[String],
        config: PtyRecorderConfig,
    ) -> Result<(Self, mpsc::UnboundedReceiver<PtyEvent>)> {
        let pty_system = portable_pty::native_pty_system();

        // Create PTY with specified size
        let pty_size = PtySize {
            rows: config.rows,
            cols: config.cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(pty_size)
            .context("Failed to create PTY")?;

        // Build command
        let mut cmd_builder = CommandBuilder::new(cmd);
        cmd_builder.args(args);

        // Spawn child process
        let child = pair
            .slave
            .spawn_command(cmd_builder)
            .context("Failed to spawn command in PTY")?;

        debug!(
            cmd = cmd,
            args = ?args,
            cols = config.cols,
            rows = config.rows,
            "Spawned command in PTY"
        );

        let (tx, rx) = mpsc::unbounded_channel();

        let recorder = Self {
            master: pair.master,
            child,
            config,
            tx,
        };

        Ok((recorder, rx))
    }

    /// Start capturing PTY output in a background thread
    ///
    /// Returns a join handle that can be used to wait for completion.
    pub fn start_capture(mut self) -> thread::JoinHandle<Result<()>> {
        thread::spawn(move || {
            let mut reader = self.master.try_clone_reader()?;
            let mut buf = vec![0u8; self.config.read_buffer_size];

            loop {
                // Try to read from PTY with timeout
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // EOF - child process likely exited
                        debug!("EOF on PTY, checking child status");
                        break;
                    }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        trace!(bytes = n, "Read PTY output");

                        if self.tx.send(PtyEvent::Data(data)).is_err() {
                            debug!("Event receiver dropped, stopping capture");
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No data available, sleep briefly
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(e) => {
                        error!(error = %e, "PTY read error");
                        let _ = self
                            .tx
                            .send(PtyEvent::Error(format!("PTY read error: {}", e)));
                        break;
                    }
                }
            }

            // Wait for child to exit and get exit code
            let status = self.child.wait()?;
            let exit_code = status.exit_code();

            debug!(exit_code = ?exit_code, "Child process exited");
            let _ = self.tx.send(PtyEvent::Exit { code: Some(exit_code) });

            Ok(())
        })
    }

    /// Resize the PTY
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        self.master
            .resize(size)
            .context("Failed to resize PTY")?;

        debug!(cols = cols, rows = rows, "Resized PTY");

        let _ = self.tx.send(PtyEvent::Resize { cols, rows });

        Ok(())
    }

    /// Write input to the PTY
    pub fn write_input(&mut self, data: &[u8]) -> Result<()> {
        let mut writer = self.master.take_writer()?;
        writer.write_all(data).context("Failed to write to PTY")?;
        writer.flush()?;
        Ok(())
    }
}

/// Terminal state tracker using vt100
///
/// Maintains the terminal grid state and tracks which rows have been modified.
pub struct TerminalState {
    /// vt100 parser
    parser: vt100::Parser,
    /// Map of row index to last byte offset that modified that row
    ///
    /// This is used to map instruction anchor points to terminal lines.
    row_last_write: HashMap<u16, u64>,
    /// Current byte offset in the PTY stream
    current_byte_off: u64,
}

impl TerminalState {
    /// Create a new terminal state with the given size
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: vt100::Parser::new(rows, cols, 1_000_000), // 1M scrollback
            row_last_write: HashMap::new(),
            current_byte_off: 0,
        }
    }

    /// Process PTY output data and update terminal state
    ///
    /// Returns the byte offset before processing this data.
    pub fn process_data(&mut self, data: &[u8]) -> u64 {
        let start_byte_off = self.current_byte_off;

        // Feed data to vt100 parser
        self.parser.process(data);

        // Track which rows were modified
        // For now, we conservatively mark all visible rows as potentially modified
        // A more sophisticated implementation would track actual changes
        let screen = self.parser.screen();
        for row in 0..screen.size().0 {
            self.row_last_write.insert(row, self.current_byte_off + data.len() as u64);
        }

        self.current_byte_off += data.len() as u64;

        start_byte_off
    }

    /// Handle terminal resize
    pub fn resize(&mut self, rows: u16, cols: u16) {
        // vt100::Parser doesn't provide a resize method in the public API
        // We need to create a new parser with the new size
        let old_parser = std::mem::replace(
            &mut self.parser,
            vt100::Parser::new(rows, cols, 1_000_000),
        );

        // Copy the terminal state from old to new parser if possible
        // This is a limitation of the vt100 crate API
        // For MVP, we accept losing state on resize
        drop(old_parser);

        debug!(rows = rows, cols = cols, "Resized terminal state");
    }

    /// Get the parser for rendering
    pub fn parser(&self) -> &vt100::Parser {
        &self.parser
    }

    /// Get the last write byte offset for a given row
    pub fn get_row_last_write(&self, row: u16) -> Option<u64> {
        self.row_last_write.get(&row).copied()
    }
}

/// Recording session that combines PTY capture with .ahr file writing
pub struct RecordingSession {
    /// PTY recorder thread handle
    recorder: Option<thread::JoinHandle<Result<()>>>,
    /// Event receiver
    rx: mpsc::UnboundedReceiver<PtyEvent>,
    /// AHR file writer
    writer: Arc<Mutex<AhrWriter>>,
    /// Terminal state tracker
    terminal: Arc<Mutex<TerminalState>>,
}

impl RecordingSession {
    /// Create a new recording session
    pub fn new(
        recorder_handle: thread::JoinHandle<Result<()>>,
        rx: mpsc::UnboundedReceiver<PtyEvent>,
        writer: AhrWriter,
        config: &PtyRecorderConfig,
    ) -> Self {
        let terminal = Arc::new(Mutex::new(TerminalState::new(
            config.rows,
            config.cols,
        )));

        Self {
            recorder: Some(recorder_handle),
            rx,
            writer: Arc::new(Mutex::new(writer)),
            terminal,
        }
    }

    /// Process PTY events and write to .ahr file
    ///
    /// This should be called in a loop until it returns None (child exited).
    pub async fn process_event(&mut self) -> Option<PtyEvent> {
        let event = self.rx.recv().await?;

        match &event {
            PtyEvent::Data(data) => {
                let ts = now_ns();

                // Update terminal state
                let start_byte_off = {
                    let mut term = self.terminal.lock().unwrap();
                    term.process_data(data)
                };

                // Write to .ahr file
                let record = Record::Data(RecData::new(ts, start_byte_off, data.clone()));
                if let Err(e) = self.writer.lock().unwrap().append_record(record) {
                    error!(error = %e, "Failed to write data record");
                }
            }
            PtyEvent::Resize { cols, rows } => {
                let ts = now_ns();

                // Update terminal state
                {
                    let mut term = self.terminal.lock().unwrap();
                    term.resize(*cols, *rows);
                }

                // Write to .ahr file
                let record = Record::Resize(RecResize::new(ts, *cols, *rows));
                if let Err(e) = self.writer.lock().unwrap().append_record(record) {
                    error!(error = %e, "Failed to write resize record");
                }
            }
            PtyEvent::Exit { code } => {
                debug!(exit_code = ?code, "Recording session ended");
            }
            PtyEvent::Error(err) => {
                warn!(error = %err, "PTY error event");
            }
        }

        Some(event)
    }

    /// Get a reference to the terminal state
    pub fn terminal(&self) -> Arc<Mutex<TerminalState>> {
        Arc::clone(&self.terminal)
    }

    /// Finalize the recording session
    pub async fn finalize(mut self) -> Result<()> {
        // Wait for recorder thread to finish
        if let Some(handle) = self.recorder.take() {
            handle
                .join()
                .map_err(|e| anyhow::anyhow!("Recorder thread panicked: {:?}", e))??;
        }

        // Finalize writer
        let writer = Arc::try_unwrap(self.writer)
            .map_err(|_| anyhow::anyhow!("Writer still has references"))?
            .into_inner()
            .unwrap();

        writer.finalize()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_state_basic() {
        let mut term = TerminalState::new(24, 80);

        let data = b"Hello, world!\n";
        let byte_off = term.process_data(data);

        assert_eq!(byte_off, 0);
        assert_eq!(term.current_byte_off, data.len() as u64);

        // Check that parser processed the data
        let screen = term.parser().screen();
        assert!(screen.contents().contains("Hello"));
    }

    #[test]
    fn test_terminal_state_multiple_writes() {
        let mut term = TerminalState::new(24, 80);

        term.process_data(b"Line 1\n");
        assert_eq!(term.current_byte_off, 7);

        term.process_data(b"Line 2\n");
        assert_eq!(term.current_byte_off, 14);

        let screen = term.parser().screen();
        let contents = screen.contents();
        assert!(contents.contains("Line 1"));
        assert!(contents.contains("Line 2"));
    }
}
