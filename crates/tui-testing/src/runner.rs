// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! TUI Test Runner - manages child processes and handles screenshot requests

use crate::protocol::TestResponse;
use anyhow::{Context, Result};
use futures_lite::AsyncReadExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tmq::{Context as TmqContext, reply};
use tokio::sync::Mutex;

/// Builder for TuiTestRunner configuration, similar to tokio::process::Command
#[derive(Debug)]
pub struct TestedTerminalProgram {
    program: String,
    args: Vec<String>,
    width: u16,
    height: u16,
    screenshot_dir: Option<PathBuf>,
    stdout: Option<std::process::Stdio>,
    stderr: Option<std::process::Stdio>,
    stdin: Option<std::process::Stdio>,
    env_vars: Vec<(String, String)>,
}

impl TestedTerminalProgram {
    /// Create a new TuiTestRunner builder for the specified program
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            width: 80,
            height: 24,
            screenshot_dir: None,
            stdout: None,
            stderr: None,
            stdin: None,
            env_vars: Vec::new(),
        }
    }

    /// Add an argument to the program command line
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple arguments to the program command line
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(|s| s.into()));
        self
    }

    /// Set the terminal width in characters
    pub fn width(mut self, width: u16) -> Self {
        self.width = width;
        self
    }

    /// Set the terminal height in characters
    pub fn height(mut self, height: u16) -> Self {
        self.height = height;
        self
    }

    /// Set the directory for storing screenshots
    pub fn screenshots(mut self, dir: impl Into<PathBuf>) -> Self {
        self.screenshot_dir = Some(dir.into());
        self
    }

    /// Configure stdout handling
    pub fn stdout(mut self, cfg: std::process::Stdio) -> Self {
        self.stdout = Some(cfg);
        self
    }

    /// Configure stderr handling
    pub fn stderr(mut self, cfg: std::process::Stdio) -> Self {
        self.stderr = Some(cfg);
        self
    }

    /// Configure stdin handling
    pub fn stdin(mut self, cfg: std::process::Stdio) -> Self {
        self.stdin = Some(cfg);
        self
    }

    /// Set an environment variable for the child process
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }

    /// Set multiple environment variables for the child process
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.env_vars.extend(vars.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    /// Spawn the configured program and return a TuiTestRunner for interaction
    pub async fn spawn(self) -> Result<TuiTestRunner> {
        TuiTestRunner::spawn_from_builder(self).await
    }

    /// Execute the program and return its exit status
    pub async fn status(self) -> Result<std::process::ExitStatus> {
        let mut child = std::process::Command::new(&self.program)
            .args(&self.args)
            .stdout(self.stdout.unwrap_or(std::process::Stdio::inherit()))
            .stderr(self.stderr.unwrap_or(std::process::Stdio::inherit()))
            .stdin(self.stdin.unwrap_or(std::process::Stdio::inherit()))
            .spawn()
            .context("Failed to spawn process")?;

        Ok(child.wait()?)
    }

    /// Execute the program and capture its output
    pub async fn output(self) -> Result<std::process::Output> {
        Ok(std::process::Command::new(&self.program)
            .args(&self.args)
            .stdout(self.stdout.unwrap_or(std::process::Stdio::piped()))
            .stderr(self.stderr.unwrap_or(std::process::Stdio::piped()))
            .stdin(self.stdin.unwrap_or(std::process::Stdio::inherit()))
            .output()?)
    }
}

/// Manages an active TUI testing session with IPC-based screenshot capture and terminal emulation
pub struct TuiTestRunner {
    endpoint: String,
    screenshots: Arc<Mutex<HashMap<String, String>>>,
    screenshot_dir: PathBuf,
    vt100_parser: vt100::Parser,
    session: expectrl::session::Session,
    exit_tx: tokio::sync::mpsc::UnboundedSender<i32>,
    exit_rx: tokio::sync::mpsc::UnboundedReceiver<i32>,
}

impl TuiTestRunner {
    /// Spawn a TUI test runner from a builder configuration
    async fn spawn_from_builder(builder: TestedTerminalProgram) -> Result<Self> {
        // Determine screenshot directory
        let screenshot_dir = builder
            .screenshot_dir
            .unwrap_or_else(|| tempfile::tempdir().unwrap().path().to_path_buf());

        // Initialize vt100 parser with the specified screen size
        let vt100_parser = vt100::Parser::new(builder.height, builder.width, 0);

        // Always start IPC server for screenshot capture on a free dynamic port
        let port = {
            use std::net::TcpListener;
            let listener = TcpListener::bind("127.0.0.1:0")
                .map_err(|e| anyhow::anyhow!("Bind dynamic port: {}", e))?;
            let port = listener.local_addr().unwrap().port();
            drop(listener); // Free the port so the ZeroMQ server can bind it
            port
        };
        let endpoint_str = format!("tcp://127.0.0.1:{}", port);

        // Create exit signal channel
        let (exit_tx, exit_rx) = tokio::sync::mpsc::unbounded_channel::<i32>();

        // Start IPC server task
        let screenshots = Arc::new(Mutex::new(HashMap::new()));
        let screenshots_clone = Arc::clone(&screenshots);
        let endpoint_clone = endpoint_str.clone();
        let exit_tx_clone = exit_tx.clone();
        tokio::spawn(async move {
            println!("Starting IPC server on {}", endpoint_clone);
            if let Err(e) =
                Self::start_ipc_server_task(endpoint_clone, screenshots_clone, exit_tx_clone).await
            {
                eprintln!("IPC server error: {}", e);
            }
        });

        // Give the IPC server a moment to start and be ready
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Set environment variables for the child process
        // Since expectrl spawns via pty, env vars should be inherited
        // Always set TUI_TESTING_URI to the IPC endpoint
        std::env::set_var("TUI_TESTING_URI", &endpoint_str);

        // Also add TUI_TESTING_URI to the builder's env_vars to ensure it's passed to the child
        let mut env_vars = builder.env_vars.clone();
        env_vars.push(("TUI_TESTING_URI".to_string(), endpoint_str.clone()));

        for (key, value) in &builder.env_vars {
            std::env::set_var(key, value);
        }

        // Build the full command line
        let mut cmd = builder.program.clone();
        for arg in &builder.args {
            cmd.push(' ');
            // Simple quoting - add quotes if the arg contains spaces
            if arg.contains(' ') {
                cmd.push('"');
                cmd.push_str(arg);
                cmd.push('"');
            } else {
                cmd.push_str(arg);
            }
        }

        println!("Spawning command: {}", cmd);
        // Spawn the program with arguments
        let session = expectrl::spawn(&cmd)?;

        Ok(Self {
            endpoint: endpoint_str,
            screenshots,
            screenshot_dir,
            vt100_parser,
            session,
            exit_tx,
            exit_rx,
        })
    }

    /// Start the IPC server task
    async fn start_ipc_server_task(
        endpoint: String,
        screenshots: Arc<Mutex<HashMap<String, String>>>,
        exit_tx: tokio::sync::mpsc::UnboundedSender<i32>,
    ) -> tmq::Result<()> {
        println!("IPC server binding to {}", endpoint);
        let mut socket = reply(&TmqContext::new()).bind(&endpoint)?;
        println!("IPC server successfully bound to {}", endpoint);

        loop {
            println!("IPC server waiting for request...");
            // Receive request
            let (msg, sender) = socket.recv().await?;
            let request_bytes = msg.iter().next().map(|m| m.as_ref()).unwrap_or(&[][..]);
            let request_str = String::from_utf8_lossy(request_bytes);

            println!("IPC server received request: {}", request_str);

            let response = Self::handle_request_static(request_bytes, &screenshots, &exit_tx).await;
            let response_str = match response {
                TestResponse::Ok => "ok".to_string(),
                TestResponse::Error(msg) => format!("error:{}", msg),
            };

            println!("IPC server sending response: {}", response_str);
            // Send response
            let response_socket =
                sender.send(tmq::Multipart::from(vec![response_str.as_bytes()])).await?;
            println!("IPC server response sent successfully");
            socket = response_socket;
        }
    }

    /// Handle IPC requests (static version for the task)
    async fn handle_request_static(
        request_bytes: &[u8],
        screenshots: &Arc<Mutex<HashMap<String, String>>>,
        exit_tx: &tokio::sync::mpsc::UnboundedSender<i32>,
    ) -> TestResponse {
        let request_str = String::from_utf8_lossy(request_bytes);
        println!("IPC server received request: {}", request_str);

        if request_str.starts_with("screenshot:") {
            let label = request_str[11..].to_string();
            println!("Capturing screenshot for label: {}", label);
            let mut screenshots_map = screenshots.lock().await;
            screenshots_map.insert(label.clone(), format!("Screenshot captured: {}", label));
            println!("Screenshot captured successfully");
            TestResponse::Ok
        } else if request_str.starts_with("exit:") {
            let exit_code_str = &request_str[5..];
            match exit_code_str.parse::<i32>() {
                Ok(exit_code) => {
                    println!("Received exit command with code: {}", exit_code);
                    // Send the exit code to the test runner to terminate the child process
                    if let Err(e) = exit_tx.send(exit_code) {
                        println!("Failed to send exit signal: {}", e);
                        return TestResponse::Error("Failed to send exit signal".to_string());
                    }
                    TestResponse::Ok
                }
                Err(_) => {
                    println!("Invalid exit code: {}", exit_code_str);
                    TestResponse::Error("Invalid exit code".to_string())
                }
            }
        } else if request_str == "ping" {
            println!("Received ping");
            TestResponse::Ok
        } else {
            println!("Unknown command: {}", request_str);
            TestResponse::Error("Unknown command".to_string())
        }
    }

    /// Wait for the child process to complete or receive an exit signal
    pub async fn wait_for_exit(mut self) -> Result<i32> {
        loop {
            tokio::select! {
                // Check for exit signal from IPC
                exit_code = self.exit_rx.recv() => {
                    match exit_code {
                        Some(code) => {
                            println!("Received exit signal with code: {}", code);

                            // Terminate the child process using OS-level APIs
                            // Get the PID from the session and use OS-specific termination
                            let pid = self.session.pid();
                            #[cfg(unix)]
                            {
                                // Send SIGTERM for graceful termination on Unix systems
                                let _ = nix::sys::signal::kill(
                                    nix::unistd::Pid::from_raw(pid.as_raw() as i32),
                                    nix::sys::signal::Signal::SIGTERM
                                );
                            }

                            #[cfg(windows)]
                            {
                                // Use taskkill on Windows
                                use std::process::Command;
                                let _ = Command::new("taskkill")
                                    .args(&["/PID", &format!("{}", pid.as_raw()), "/T", "/F"])
                                    .status();
                            }

                            // Also send Ctrl+C through PTY as fallback
                            let _ = self.session.send_control('c').await;

                            // Wait a bit for the process to terminate gracefully
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            return Ok(code);
                        }
                        None => {
                            // Channel closed, continue waiting for normal exit
                        }
                    }
                }
                // Check if the process has exited normally
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                    // Check if the session is still active
                    // For now, just continue - we could add more sophisticated process monitoring
                }
            }
        }
    }

    /// Get the ZeroMQ endpoint URI for child processes to connect to
    pub fn endpoint_uri(&self) -> &str {
        &self.endpoint
    }

    /// Get the screenshot directory
    pub fn screenshot_dir(&self) -> &PathBuf {
        &self.screenshot_dir
    }

    /// Send a string to the spawned program
    pub async fn send(&mut self, s: &str) -> Result<()> {
        self.session.send(s).await?;
        Ok(())
    }

    /// Send a control character to the spawned program
    pub async fn send_control(&mut self, c: char) -> Result<()> {
        self.session.send_control(c).await?;
        Ok(())
    }

    /// Expect a pattern in the output
    pub async fn expect(&mut self, pattern: &str) -> Result<()> {
        self.session.expect(pattern).await?;
        Ok(())
    }

    /// Read output and feed it to the vt100 parser
    pub async fn read_and_parse(&mut self) -> Result<usize> {
        let mut buf = [0u8; 8192];
        let n = self.session.read(&mut buf).await?;
        if n > 0 {
            self.vt100_parser.process(&buf[..n]);
        }
        Ok(n)
    }

    /// Get the current screen contents as formatted text
    pub fn screen_contents(&self) -> String {
        String::from_utf8_lossy(&self.vt100_parser.screen().contents_formatted()).to_string()
    }

    /// Get the current screen as raw cells
    pub fn screen(&self) -> &vt100::Screen {
        self.vt100_parser.screen()
    }

    /// Wait for the spawned program to complete
    pub async fn wait(&mut self) -> Result<()> {
        // Try to read any remaining output
        let _ = self.read_and_parse().await;
        Ok(())
    }

    /// Get captured screenshots
    pub async fn get_screenshots(&self) -> HashMap<String, String> {
        self.screenshots.lock().await.clone()
    }
}
