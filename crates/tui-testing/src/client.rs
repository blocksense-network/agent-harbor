//! TUI Test Client - used by child processes to communicate with test runner

use crate::protocol::{TestCommand, TestResponse};
use anyhow::{Context, Result};
use std::time::Duration;
use tokio::time;
use tmq::{request, Context as TmqContext, request_reply::RequestSender, AsZmqSocket};

/// Client for communicating with the TUI test runner from child processes
pub struct TuiTestClient {
    socket: Option<RequestSender>,
    context: TmqContext,
    endpoint: String,
    timeout: Duration,
}

impl TuiTestClient {
    /// Connect to a test runner at the given ZeroMQ URI
    pub async fn connect(uri: &str) -> Result<Self> {
        let context = TmqContext::new();

        // Create the socket
        let mut socket = request(&context)
            .connect(uri)
            .with_context(|| format!("Failed to create connection to {}", uri))?;

        // Set socket timeouts to prevent hanging
        socket.get_socket().set_rcvtimeo(100)?; // 100ms receive timeout
        socket.get_socket().set_sndtimeo(100)?; // 100ms send timeout

        Ok(Self {
            socket: Some(socket),
            context,
            endpoint: uri.to_string(),
            timeout: Duration::from_millis(500),
        })
    }

    /// Request a screenshot capture with the given label
    /// Request a screenshot capture with the given label
    pub async fn request_screenshot(&mut self, label: &str) -> Result<()> {
        let command = TestCommand::Screenshot(label.to_string());
        let response = self.send_command(command).await?;
        match response {
            TestResponse::Ok => Ok(()),
            TestResponse::Error(msg) => Err(anyhow::anyhow!("Screenshot request failed: {}", msg)),
        }
    }

    /// Send a ping to check connectivity
    pub async fn ping(&mut self) -> Result<()> {
        let response = self.send_command(TestCommand::Ping).await?;
        match response {
            TestResponse::Ok => Ok(()),
            TestResponse::Error(msg) => Err(anyhow::anyhow!("Ping failed: {}", msg)),
        }
    }

    /// Send a command and receive response
    async fn send_command(&mut self, command: TestCommand) -> Result<TestResponse> {
        // Convert command to simple string format
        let message = match command {
            TestCommand::Screenshot(label) => format!("screenshot:{}", label),
            TestCommand::Ping => "ping".to_string(),
        };

        println!("TuiTestClient: Sending command: {}", message);

        // Get the current socket
        let socket = self.socket.take().context("No socket available")?;

        // Send request and receive response with timeout using select
        println!("TuiTestClient: Sending request and waiting for response with timeout {}ms", self.timeout.as_millis());

        let send_future = async move {
            let receiver = socket.send(tmq::Multipart::from(vec![message.as_bytes()])).await
                .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;
            receiver.recv().await.map_err(|e| anyhow::anyhow!("Failed to receive response: {}", e))
        };

        let timeout_future = time::sleep(self.timeout);
        tokio::pin!(send_future);
        tokio::pin!(timeout_future);

        let result = tokio::select! {
            result = &mut send_future => {
                result.context("Failed to receive response")
            }
            _ = &mut timeout_future => {
                Err(anyhow::anyhow!("Timeout waiting for response"))
            }
        }?;

        let (response_msg, new_sender) = result;
        println!("TuiTestClient: Received response");

        // Store the new sender for the next request
        self.socket = Some(new_sender);

        // Parse response
        let response_bytes = response_msg.iter().next().map(|m| m.as_ref()).unwrap_or(&[][..]);
        let response_str = String::from_utf8_lossy(response_bytes);
        println!("TuiTestClient: Parsed response: '{}' ({} bytes)", response_str, response_bytes.len());
        let response = match response_str.as_ref() {
            "ok" => TestResponse::Ok,
            s if s.starts_with("error:") => TestResponse::Error(s[6..].to_string()),
            _ => return Err(anyhow::anyhow!("Invalid response format: {}", response_str)),
        };

        Ok(response)
    }
}

impl Drop for TuiTestClient {
    fn drop(&mut self) {
        // Socket will be closed when ZmqContext is dropped
        // ZeroMQ handles cleanup automatically
    }
}

