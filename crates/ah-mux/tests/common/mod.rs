// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Common helpers for integration tests

/// Terminal dimensions returned by our test binary
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

/// Measure the current terminal size (columns, rows).
///
/// This is shared across integration tests and helper binaries.
pub fn measure_terminal_size() -> Result<TerminalSize, Box<dyn std::error::Error>> {
    let (cols, rows) = crossterm::terminal::size()?;
    Ok(TerminalSize { cols, rows })
}
