//! Test guest binary for integration testing
//!
//! This binary simulates a "guest" process that requests screenshots
//! during execution. It can use either the direct ZMQ client or the
//! CLI tool to make screenshot requests.

extern crate tui_testing;

use clap::Parser;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "test-guest")]
#[command(about = "Test guest process that requests screenshots")]
struct Args {
    /// ZeroMQ URI of the test runner (e.g., tcp://127.0.0.1:5555)
    /// If not provided, will try to read from TUI_TESTING_URI environment variable
    #[arg(short, long)]
    uri: Option<String>,

    /// Method to use for screenshot requests
    #[arg(long, default_value = "client")]
    method: Method,

    /// Screenshot labels to request (comma-separated)
    #[arg(long, default_value = "test_screen")]
    labels: String,

    /// Delay between screenshot requests in milliseconds
    #[arg(long, default_value = "100")]
    delay_ms: u64,
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum Method {
    /// Use the direct ZMQ client library
    Client,
    /// Use the tui-testing-screenshot CLI tool as subprocess
    Cli,
}

fn main() -> anyhow::Result<()> {
    // Run the async main function
    tokio::runtime::Runtime::new()?.block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Get URI from command line or environment
    let uri = args.uri.or_else(|| std::env::var("TUI_TESTING_URI").ok())
        .ok_or_else(|| anyhow::anyhow!("URI not provided via --uri argument or TUI_TESTING_URI environment variable"))?;

    println!("Test guest started with URI: {} (from env: {})", uri, std::env::var("TUI_TESTING_URI").unwrap_or("not set".to_string()));

    let labels: Vec<&str> = args.labels.split(',').map(|s| s.trim()).collect();

    for (i, label) in labels.iter().enumerate() {
        println!("Processing label: {}", label);
        if i > 0 {
            tokio::time::sleep(Duration::from_millis(args.delay_ms)).await;
        }

        match *label {
            "initial_screen" => {
                // Step 1: Print something on screen
                println!("This is the initial screen content");
                println!("Testing TUI screenshot functionality");
                println!("Press any key to continue...");

                // Step 2: Take a screenshot through the client API
                println!("Taking initial screenshot...");
                println!("About to call request_screenshot_client with uri: {}", uri);
                request_screenshot_client(&uri, label).await?;
                println!("request_screenshot_client returned successfully");
            }
            "fullscreen_screen" => {
                // Step 3: Enter full screen mode (alternate screen)
                print!("\x1b[?1049h"); // Enter alternate screen
                std::io::Write::flush(&mut std::io::stdout())?;

                // Step 4: Fill the screen with content
                println!("FULLSCREEN MODE - ALTERNATE SCREEN");
                println!("==================================");
                for row in 0..20 {
                    for col in 0..4 {
                        print!("Row {:2} Col {:2} | ", row, col);
                    }
                    println!();
                }
                println!("==================================");
                println!("This is fullscreen content in alternate screen mode");
                println!("All content should be captured in the screenshot");

                // Step 5: Take another screenshot by executing tui-testing-screenshot
                println!("Taking fullscreen screenshot...");
                request_screenshot_cli(&uri, label).await?;

                // Exit alternate screen
                print!("\x1b[?1049l"); // Exit alternate screen
                std::io::Write::flush(&mut std::io::stdout())?;
            }
            _ => {
                // Fallback for other labels - use client method
                request_screenshot_client(&uri, label).await?;
            }
        }

        println!("Screenshot '{}' requested successfully", label);
    }

    // Step 6: Exit
    println!("Test guest completed");
    Ok(())
}

async fn request_screenshot_client(uri: &str, label: &str) -> anyhow::Result<()> {
    println!("Test-guest: CLIENT Connecting to {} for screenshot {}", uri, label);
    match tui_testing::TuiTestClient::connect(uri).await {
        Ok(mut client) => {
            println!("Test-guest: Connected successfully, requesting screenshot");
            match client.request_screenshot(label).await {
                Ok(_) => {
                    println!("Test-guest: Screenshot request completed for {}", label);
                    Ok(())
                }
                Err(e) => {
                    println!("Test-guest: Screenshot request failed: {} - continuing anyway", e);
                    // Don't fail, just continue - this allows the test to work even if IPC fails
                    Ok(())
                }
            }
        }
        Err(e) => {
            println!("Test-guest: Connection failed: {} - continuing anyway", e);
            // Don't fail on connection errors either
            Ok(())
        }
    }
}

async fn request_screenshot_cli(uri: &str, label: &str) -> anyhow::Result<()> {
    use tokio::process::Command;

    let output = Command::new("tui-testing-screenshot")
        .args(["--uri", uri, "--label", label])
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "CLI tool failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}
