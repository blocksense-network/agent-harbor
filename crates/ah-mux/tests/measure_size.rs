// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Helper binary to measure terminal size in a multiplexer pane
//!
//! This binary is designed to be run within multiplexer panes to measure
//! their actual terminal dimensions and output them in a parseable format.

mod common;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Give the pane a moment to initialize
    std::thread::sleep(std::time::Duration::from_millis(100));

    let size = common::measure_terminal_size()?;
    // Emit structured JSON size information via tracing instead of stdout print
    use tracing::info;
    info!(terminal_size = %serde_json::to_string(&size)?, "captured terminal size");

    // Keep the process alive briefly so the multiplexer can capture the output
    std::thread::sleep(std::time::Duration::from_millis(200));

    Ok(())
}
