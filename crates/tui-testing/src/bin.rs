//! Simple command-line client for sending commands to TUI test runner

use clap::Parser;
use std::time::Duration;
use tmq::{Context as TmqContext, request};

/// Simple command-line client for sending commands to TUI test runner
#[derive(Parser)]
#[command(name = "tui-testing-cmd")]
#[command(about = "Send commands to TUI test runner via ZMQ")]
struct Args {
    /// ZeroMQ URI of the test runner (e.g., tcp://127.0.0.1:5555)
    /// If not provided, will try to read from TUI_TESTING_URI environment variable
    #[arg(short, long)]
    uri: Option<String>,

    /// Command to send (screenshot:<label> or exit:<exit-code>)
    #[arg(short, long)]
    cmd: String,

    /// Timeout in seconds for the request
    #[arg(short, long, default_value = "5")]
    timeout: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Get URI from command line or environment
    let uri = args.uri.or_else(|| std::env::var("TUI_TESTING_URI").ok()).ok_or_else(|| {
        anyhow::anyhow!(
            "URI not provided via --uri argument or TUI_TESTING_URI environment variable"
        )
    })?;

    // Create tmq request socket
    let socket = request(&TmqContext::new())
        .connect(&uri)
        .map_err(|e| anyhow::anyhow!("Failed to connect to {}: {}", uri, e))?;

    // Send command request
    let message = args.cmd.clone();
    println!("Sending command: {}", message);

    let receiver = socket
        .send(tmq::Multipart::from(vec![message.as_bytes()]))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send request: {}", e))?;

    // Receive response with timeout
    let timeout_duration = Duration::from_secs(args.timeout as u64);
    match tokio::time::timeout(timeout_duration, receiver.recv()).await {
        Ok(Ok((response_msg, _))) => {
            let response_bytes = response_msg.iter().next().map(|m| m.as_ref()).unwrap_or(&[][..]);
            let response = String::from_utf8_lossy(response_bytes);
            match response.as_ref() {
                "ok" => {
                    println!("✓ Command '{}' executed successfully", args.cmd);
                    Ok(())
                }
                s if s.starts_with("error:") => {
                    eprintln!("✗ Command '{}' failed: {}", args.cmd, &s[6..]);
                    std::process::exit(1);
                }
                _ => {
                    eprintln!("✗ Unexpected response: {}", response);
                    std::process::exit(1);
                }
            }
        }
        Ok(Err(e)) => {
            eprintln!("✗ Failed to receive response: {}", e);
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("✗ Timeout waiting for response");
            std::process::exit(1);
        }
    }
}
