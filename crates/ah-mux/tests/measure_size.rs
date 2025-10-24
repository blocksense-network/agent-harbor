// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Helper binary to measure terminal size in a multiplexer pane
//!
//! This binary is designed to be run within multiplexer panes to measure
//! their actual terminal dimensions and output them in a parseable format.

use std::io::{self, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Give the pane a moment to initialize
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Measure terminal size using tput
    let cols_output = std::process::Command::new("tput").arg("cols").output()?;

    let rows_output = std::process::Command::new("tput").arg("lines").output()?;

    if !cols_output.status.success() || !rows_output.status.success() {
        eprintln!("Failed to measure terminal size");
        std::process::exit(1);
    }

    let cols = String::from_utf8(cols_output.stdout)?.trim().parse::<u16>()?;

    let rows = String::from_utf8(rows_output.stdout)?.trim().parse::<u16>()?;

    // Output in JSON format for easy parsing
    println!("{{\"cols\": {}, \"rows\": {}}}", cols, rows);

    // Keep the process alive briefly so the multiplexer can capture the output
    std::thread::sleep(std::time::Duration::from_millis(200));

    Ok(())
}
