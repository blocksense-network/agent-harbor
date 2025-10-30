// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// PTY management for spawning and capturing agent sessions
//
// Uses portable-pty for cross-platform PTY support and vt100 for terminal state tracking

use crate::format::{REC_SNAPSHOT, RecData, RecHeader, RecResize, RecSnapshot, Record};
use crate::writer::{AhrWriter, now_ns};
use anyhow::{Context, Result};
use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, trace};

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
    /// Environment variables to set for the spawned process
    pub env_vars: Vec<(String, String)>,
}

impl Default for PtyRecorderConfig {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            read_buffer_size: 8192,
            capture_input: false,
            env_vars: Vec::new(),
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
    /// Writer for sending input to the PTY
    writer: Option<Box<dyn std::io::Write + Send>>,
}

impl PtyRecorder {
    /// Spawn a command under a PTY with the given configuration
    ///
    /// Returns a receiver for PTY events and the recorder instance.
    /// Input is written directly via write_input() method.
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

        let pair = pty_system.openpty(pty_size).context("Failed to create PTY")?;

        // Build command
        let mut cmd_builder = CommandBuilder::new(cmd);
        cmd_builder.args(args);

        // Set environment variables
        for (key, value) in &config.env_vars {
            cmd_builder.env(key, value);
        }

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

        // Take the PTY writer once - we'll write to it directly from the main thread
        let writer = pair.master.take_writer().ok();
        debug!("Took PTY writer: {}", writer.is_some());

        let recorder = Self {
            master: pair.master,
            child,
            config,
            tx,
            writer,
        };

        Ok((recorder, rx))
    }

    /// Start capturing PTY output in a background thread
    ///
    /// Returns a join handle that can be used to wait for completion.
    pub fn start_capture(mut self) -> thread::JoinHandle<Result<()>> {
        debug!("start_capture called");
        thread::spawn(move || {
            debug!("PTY recorder thread spawned, starting catch_unwind");
            // Catch panics to prevent silent thread exits
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                debug!("PTY recorder thread entering catch_unwind closure");
                // Initialize PTY reader
                let mut reader = match self.master.try_clone_reader() {
                    Ok(r) => r,
                    Err(e) => {
                        error!("Failed to create PTY reader: {}", e);
                        return Err(e);
                    }
                };
                let mut buf = vec![0u8; self.config.read_buffer_size];

                debug!("PTY recorder thread started, entering main loop");

                // Check initial child status
                match self.child.try_wait() {
                    Ok(Some(status)) => {
                        debug!(
                            "Child already exited at thread start with status: {:?}",
                            status
                        );
                    }
                    Ok(None) => {
                        debug!("Child is running at thread start");
                    }
                    Err(e) => {
                        debug!("Failed to check initial child status: {}", e);
                    }
                }

                loop {
                    // Try to read from PTY with timeout
                    match reader.read(&mut buf) {
                        Ok(0) => {
                            // EOF - child process likely exited
                            debug!("EOF on PTY reader, checking child status");
                            // Check if child is still alive before exiting
                            match self.child.try_wait() {
                                Ok(Some(status)) => {
                                    debug!("Child exited with status: {:?}", status);
                                }
                                Ok(None) => {
                                    debug!("Child is still running despite EOF on PTY reader");
                                }
                                Err(e) => {
                                    debug!("Failed to check child status: {}", e);
                                }
                            }
                            break;
                        }
                        Ok(n) => {
                            let data = buf[..n].to_vec();
                            trace!(bytes = n, "Read PTY output");

                            if let Err(e) = self.tx.send(PtyEvent::Data(data)) {
                                debug!(
                                    "Failed to send PTY data event: {}. Main thread may have exited.",
                                    e
                                );
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
                            let _ = self.tx.send(PtyEvent::Error(format!("PTY read error: {}", e)));
                            break;
                        }
                    }
                }

                debug!("PTY recorder thread exiting main loop");

                // Wait for child to exit and get exit code
                let status = self.child.wait()?;
                let exit_code = status.exit_code();

                debug!(exit_code = ?exit_code, "Child process exited");
                let send_result = self.tx.send(PtyEvent::Exit {
                    code: Some(exit_code),
                });
                debug!("Sent exit event, result: {:?}", send_result);

                Ok(())
            }));

            debug!("Catch_unwind completed, result is_err: {}", result.is_err());

            // Handle panics in the PTY recorder thread
            match result {
                Ok(r) => {
                    debug!("PTY recorder thread completed successfully");
                    r
                }
                Err(panic) => {
                    error!("PTY recorder thread panicked: {:?}", panic);
                    // Send an error event to indicate the thread crashed
                    let send_result = self.tx.send(PtyEvent::Error(format!(
                        "PTY recorder thread panicked: {:?}",
                        panic
                    )));
                    debug!("Sent panic error event, result: {:?}", send_result);
                    Err(anyhow::anyhow!("PTY recorder thread panicked"))
                }
            }
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

        self.master.resize(size).context("Failed to resize PTY")?;

        debug!(cols = cols, rows = rows, "Resized PTY");

        let _ = self.tx.send(PtyEvent::Resize { cols, rows });

        Ok(())
    }

    /// Kill the child process
    pub fn kill(&mut self) -> Result<()> {
        self.child.kill().context("Failed to kill child process")
    }

    /// Start capturing PTY output and return both the thread handle and child killer
    pub fn start_capture_and_get_killer(
        mut self,
    ) -> (
        thread::JoinHandle<Result<()>>,
        Box<dyn ChildKiller + Send + Sync>,
    ) {
        // Clone the killer before moving the child
        let child_killer = self.child.clone_killer();

        let handle = thread::spawn(move || {
            // Catch panics to prevent silent thread exits
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                debug!("PTY recorder thread starting execution");
                debug!("PTY recorder thread spawned, starting catch_unwind");
                // Initialize PTY reader and writer
                let mut reader = match self.master.try_clone_reader() {
                    Ok(r) => r,
                    Err(e) => {
                        error!("Failed to create PTY reader: {}", e);
                        return Err(e);
                    }
                };
                let mut buf = vec![0u8; self.config.read_buffer_size];

                debug!("PTY recorder thread started, entering main loop");

                // Check initial child status
                match self.child.try_wait() {
                    Ok(Some(status)) => {
                        debug!(
                            "Child already exited at thread start with status: {:?}",
                            status
                        );
                    }
                    Ok(None) => {
                        debug!("Child is running at thread start");
                    }
                    Err(e) => {
                        debug!("Failed to check initial child status: {}", e);
                    }
                }

                loop {
                    // Try to read from PTY with timeout
                    match reader.read(&mut buf) {
                        Ok(0) => {
                            // EOF - child process likely exited
                            debug!("EOF on PTY reader, checking child status");
                            // Check if child is still alive before exiting
                            match self.child.try_wait() {
                                Ok(Some(status)) => {
                                    debug!("Child exited with status: {:?}", status);
                                }
                                Ok(None) => {
                                    debug!("Child is still running despite EOF on PTY reader");
                                }
                                Err(e) => {
                                    debug!("Failed to check child status: {}", e);
                                }
                            }
                            break;
                        }
                        Ok(n) => {
                            let data = buf[..n].to_vec();
                            trace!(bytes = n, "Read PTY output");

                            if let Err(e) = self.tx.send(PtyEvent::Data(data)) {
                                debug!(
                                    "Failed to send PTY data event: {}. Main thread may have exited.",
                                    e
                                );
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
                            let _ = self.tx.send(PtyEvent::Error(format!("PTY read error: {}", e)));
                            break;
                        }
                    }
                }

                debug!("PTY recorder thread exiting main loop");

                // Wait for child to exit and get exit code
                let status = self.child.wait()?;
                let exit_code = status.exit_code();

                debug!(exit_code = ?exit_code, "Child process exited");
                let send_result = self.tx.send(PtyEvent::Exit {
                    code: Some(exit_code),
                });
                debug!("Sent exit event, result: {:?}", send_result);

                Ok(())
            }));

            debug!("Catch_unwind completed, result is_err: {}", result.is_err());

            // Handle panics in the PTY recorder thread
            match result {
                Ok(r) => {
                    debug!("PTY recorder thread completed successfully");
                    r
                }
                Err(panic) => {
                    error!("PTY recorder thread panicked: {:?}", panic);
                    // Send an error event to indicate the thread crashed
                    let send_result = self.tx.send(PtyEvent::Error(format!(
                        "PTY recorder thread panicked: {:?}",
                        panic
                    )));
                    debug!("Sent panic error event, result: {:?}", send_result);
                    Err(anyhow::anyhow!("PTY recorder thread panicked"))
                }
            }
        });

        (handle, child_killer)
    }

    /// Extract the PTY writer for direct use
    pub fn take_writer(&mut self) -> Option<Box<dyn std::io::Write + Send>> {
        self.writer.take()
    }

    /// Write input to the PTY
    pub fn write_input(&mut self, data: &[u8]) -> Result<()> {
        if let Some(ref mut writer) = self.writer {
            writer.write_all(data).context("Failed to write to PTY")?;
            writer.flush().context("Failed to flush PTY writer")?;
            Ok(())
        } else {
            // Try to get a writer if we don't have one
            let mut writer = self.master.take_writer()?;
            writer.write_all(data).context("Failed to write to PTY")?;
            writer.flush().context("Failed to flush PTY writer")?;
            // Store it for future use
            self.writer = Some(writer);
            Ok(())
        }
    }
}

/// Terminal state tracker using vt100
///
/// Maintains the terminal grid state and tracks which rows have been modified.

/// Recording session that combines PTY capture with .ahr file writing
pub struct RecordingSession {
    /// PTY writer for direct input writing
    pub pty_writer: Arc<std::sync::Mutex<Option<Box<dyn std::io::Write + Send>>>>,
    /// PTY recorder thread handle
    recorder_thread: Option<thread::JoinHandle<Result<()>>>,
    /// Child killer for terminating the process
    child_killer: Option<Box<dyn ChildKiller + Send + Sync>>,
    /// Event receiver
    rx: mpsc::UnboundedReceiver<PtyEvent>,
    /// AHR file writer (None when no output file is specified)
    writer: Option<Arc<Mutex<AhrWriter>>>,
    /// Terminal state tracker (unified with display state)
    recording_terminal_state: Option<std::rc::Rc<std::cell::RefCell<crate::TerminalState>>>,
}

impl RecordingSession {
    /// Create a new recording session
    pub fn new(
        pty_writer: Option<Box<dyn std::io::Write + Send>>,
        recorder_handle: thread::JoinHandle<Result<()>>,
        child_killer: Box<dyn ChildKiller + Send + Sync>,
        rx: mpsc::UnboundedReceiver<PtyEvent>,
        writer: Option<AhrWriter>,
        recording_terminal_state: Option<std::rc::Rc<std::cell::RefCell<crate::TerminalState>>>,
    ) -> Self {
        let writer_arc = writer.map(|w| Arc::new(Mutex::new(w)));

        // Write initial resize record to establish terminal dimensions at start of recording
        if let Some(ref writer) = writer_arc {
            let ts = now_ns();
            // Use dimensions from recording_terminal_state if available, otherwise default
            let (rows, cols) = if let Some(ref rts) = recording_terminal_state {
                let rts = rts.borrow();
                rts.dimensions()
            } else {
                (80, 24) // fallback dimensions (rows, cols)
            };
            let record = Record::Resize(RecResize::new(ts, cols, rows));
            if let Err(e) = writer.lock().unwrap().append_record(record) {
                error!(error = %e, "Failed to write initial resize record");
            }
        }

        let session = Self {
            pty_writer: Arc::new(std::sync::Mutex::new(pty_writer)),
            recorder_thread: Some(recorder_handle),
            child_killer: Some(child_killer),
            rx,
            writer: writer_arc,
            recording_terminal_state,
        };
        debug!(
            "RecordingSession created with PTY writer: {:?}",
            session.pty_writer.lock().unwrap().is_some()
        );
        session
    }

    /// Get the current global byte offset from the AHR writer
    pub fn current_byte_offset(&self) -> u64 {
        self.writer.as_ref().map(|w| w.lock().unwrap().global_byte_off()).unwrap_or(0)
    }

    /// Append a record to the AHR writer
    pub fn append_record(&self, record: Record) -> Result<()> {
        if let Some(writer) = &self.writer {
            writer.lock().unwrap().append_record(record)
        } else {
            Ok(())
        }
    }

    /// Append a record to the AHR writer and immediately flush/sync to disk
    ///
    /// This ensures the record is durably written before returning.
    /// Used for critical records like snapshots where synchronous writes are required.
    pub fn append_record_sync(&self, record: Record) -> Result<()> {
        if let Some(writer) = &self.writer {
            writer.lock().unwrap().append_record_sync(record)
        } else {
            Ok(())
        }
    }

    /// Process PTY events and write to .ahr file
    ///
    /// This should be called in a loop until it returns None (child exited).
    /// Receive the next PTY event from the channel (without processing it)
    pub async fn next_event(&mut self) -> Option<PtyEvent> {
        self.rx.recv().await
    }

    /// Kill the child process
    pub fn kill_child(&mut self) -> Result<()> {
        if let Some(ref mut killer) = self.child_killer {
            killer.kill().context("Failed to kill child process")
        } else {
            Ok(())
        }
    }

    /// Write input data directly to the PTY (for interactive use during recording)
    pub fn write_input(&mut self, data: &[u8]) -> Result<()> {
        if let Ok(mut writer_opt) = self.pty_writer.lock() {
            if let Some(ref mut writer) = *writer_opt {
                writer.write_all(data)?;
                writer.flush()?;
                debug!("Wrote {} bytes directly to PTY", data.len());
                Ok(())
            } else {
                debug!("No PTY writer available for input");
                Ok(())
            }
        } else {
            debug!("Failed to lock PTY writer");
            Ok(())
        }
    }

    /// Write bytes directly to the PTY writer (used by vt100 callbacks)
    pub fn write_bytes(&self, bytes: &[u8]) {
        if let Ok(mut writer_opt) = self.pty_writer.lock() {
            if let Some(ref mut writer) = *writer_opt {
                let _ = writer.write_all(bytes);
                let _ = writer.flush();
            }
        }
    }

    /// Write formatted string directly to the PTY writer
    pub fn write_fmt(&self, s: &str) {
        self.write_bytes(s.as_bytes());
    }

    /// Get current terminal features for input encoding
    pub fn term_features(&self) -> crate::TermFeatures {
        self.recording_terminal_state.as_ref().unwrap().borrow().term_features()
    }

    /// Process a PTY event through the pipeline: AHR writer -> TerminalState
    pub fn process_pty_event(&mut self, event: PtyEvent) -> Result<()> {
        match event {
            PtyEvent::Data(data) => {
                let ts = now_ns();

                // Get current byte offset from AHR writer
                let start_byte_off = self.current_byte_offset();

                // Process through VT100 parser with callbacks if we have recording terminal state
                // The parser in TerminalState already has callbacks set up for DSR and mode changes
                if let Some(ref rts) = self.recording_terminal_state {
                    rts.borrow_mut().process_data(&data);
                }

                // Write to .ahr file
                let record = Record::Data(RecData::new(ts, start_byte_off, data));
                self.append_record(record)?;
            }
            PtyEvent::Resize { cols, rows } => {
                let ts = now_ns();

                // Update TerminalState if available
                if let Some(ref rts) = self.recording_terminal_state {
                    rts.borrow_mut().resize(cols, rows);
                }

                // Write resize record
                let record = Record::Resize(RecResize::new(ts, cols, rows));
                self.append_record(record)?;
            }
            PtyEvent::Exit { .. } | PtyEvent::Error(_) => {
                // These are handled at a higher level
                return Ok(());
            }
        }

        Ok(())
    }

    /// Process a snapshot event through the pipeline: AHR writer -> TerminalState
    pub fn process_snapshot_event(
        &mut self,
        snapshot_id: u64,
        label: Option<&str>,
        ts_ns: u64,
    ) -> Result<()> {
        // Get current byte offset from the session
        let current_offset = self.current_byte_offset();

        // Update TerminalState with snapshot if available
        if let Some(ref rts) = self.recording_terminal_state {
            let snapshot = crate::AhrSnapshot {
                ts_ns,
                label: label.map(|s| s.to_string()),
            };
            rts.borrow_mut().record_snapshot(snapshot);
        }

        // Write snapshot record to AHR file (synchronously for strong guarantees)
        let snapshot_record = RecSnapshot {
            header: RecHeader {
                tag: REC_SNAPSHOT,
                pad: [0; 3],
                ts_ns,
            },
            anchor_byte: current_offset,
            snapshot_id,
            label: label.unwrap_or_default().to_string(),
        };
        self.append_record_sync(Record::Snapshot(snapshot_record))?;

        Ok(())
    }

    /// Finalize the recording session
    pub async fn finalize(mut self) -> Result<()> {
        // Wait for recorder thread to finish
        if let Some(handle) = self.recorder_thread.take() {
            handle
                .join()
                .map_err(|e| anyhow::anyhow!("Recorder thread panicked: {:?}", e))??;
        }

        // Finalize writer (only if writer exists)
        if let Some(writer_arc) = self.writer {
            let writer = Arc::try_unwrap(writer_arc)
                .map_err(|_| anyhow::anyhow!("Writer still has references"))?
                .into_inner()
                .unwrap();
            writer.finalize()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TerminalState;

    #[test]
    fn test_terminal_state_basic() {
        let mut term = TerminalState::new(24, 80);

        let data = b"Hello, world!\n";
        term.process_data(data);

        // Check that parser processed the data
        let screen = term.parser().screen();
        assert!(screen.contents().contains("Hello"));
    }

    #[test]
    fn test_terminal_state_multiple_writes() {
        let mut term = TerminalState::new(24, 80);

        term.process_data(b"Line 1\n");
        term.process_data(b"Line 2\n");

        let screen = term.parser().screen();
        let contents = screen.contents();
        assert!(contents.contains("Line 1"));
        assert!(contents.contains("Line 2"));
    }

    #[tokio::test]
    async fn test_recording_session_initial_resize_record() -> Result<()> {
        use crate::writer::WriterConfig;
        use tempfile::NamedTempFile;

        let temp = NamedTempFile::new()?;
        let path = temp.path().to_path_buf();

        let config = WriterConfig::default();
        let writer = AhrWriter::create(&path, config)?;

        // Create a mock recorder handle and receiver for testing
        use std::thread;
        let recorder_handle = thread::spawn(|| Ok(()));

        // Create mock child killer
        #[derive(Debug)]
        struct MockChildKiller;
        impl portable_pty::ChildKiller for MockChildKiller {
            fn kill(&mut self) -> std::io::Result<()> {
                Ok(())
            }

            fn clone_killer(&self) -> Box<dyn portable_pty::ChildKiller + Send + Sync> {
                Box::new(MockChildKiller)
            }
        }
        let child_killer =
            Box::new(MockChildKiller) as Box<dyn portable_pty::ChildKiller + Send + Sync>;

        // Create mock receiver
        let (tx, rx) = mpsc::unbounded_channel::<PtyEvent>();
        drop(tx); // Close sender to simulate end of events

        let pty_config = PtyRecorderConfig {
            cols: 120,
            rows: 40,
            ..Default::default()
        };

        // Create recording terminal state with the expected dimensions
        let recording_terminal_state = Some(std::rc::Rc::new(std::cell::RefCell::new(
            TerminalState::new(40, 120),
        )));

        // Create recording session - this should write initial resize record
        let session = RecordingSession::new(
            None, // pty_writer - not needed for this test
            recorder_handle,
            child_killer,
            rx,
            Some(writer),
            recording_terminal_state,
        );

        // Wait for session to finish processing (though it should finish immediately since we dropped the sender)
        let mut session = session;
        while let Some(_) = session.next_event().await {}

        // Finalize the session to flush any remaining data
        session.finalize().await?;

        // Read back the AHR file and verify it starts with a resize record
        use crate::reader::AhrReader;
        let mut reader = AhrReader::new(&path)?;
        let events = reader.read_all_events()?;

        // First event should be a resize record with the correct dimensions
        assert!(
            !events.is_empty(),
            "AHR file should contain at least one event"
        );
        match &events[0] {
            crate::AhrEvent::Resize { cols, rows, .. } => {
                assert_eq!(*cols, 120, "Initial resize should have correct columns");
                assert_eq!(*rows, 40, "Initial resize should have correct rows");
            }
            other => panic!("First event should be a resize record, got {:?}", other),
        }

        Ok(())
    }
}
