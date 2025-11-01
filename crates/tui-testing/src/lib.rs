// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! # TUI Testing Framework
//!
//! A framework for testing terminal user interfaces using IPC-based screenshot capture.
//!
//! ## Overview
//!
//! This crate provides a testing framework that allows child processes (like TUI applications)
//! to communicate with test runners via ZeroMQ for capturing screenshots during test execution.
//!
//! ## Architecture
//!
//! The framework consists of two main components:
//!
//! 1. **Test Runner** (`TuiTestRunner`): Runs the child process and manages screenshot capture
//! 2. **Child Process Client** (`TuiTestClient`): Used by the child process to request screenshots
//!
//! ## Protocol
//!
//! Communication uses ZeroMQ REQ/REP pattern with simple UTF-8 string messages:
//!
//! ### Screenshot Request (from child to runner)
//! ```text
//! screenshot:my_label
//! ```
//!
//! ### Screenshot Response (from runner to child)
//! ```text
//! ok
//! ```
//!
//! ### Ping Request
//! ```text
//! ping
//! ```
//!
//! ### Ping Response
//! ```text
//! ok
//! ```
//!
//! ## Usage
//!
//! ### In Test Runner
//!
//! ```rust,no_run
//! use tui_testing::TestedTerminalProgram;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let runner = TestedTerminalProgram::new("my-tui-app")
//!     .arg("--verbose")
//!     .spawn()
//!     .await?;
//!
//! // The child process will have TUI_TESTING_URI set automatically
//! // Run test interactions here...
//!
//! Ok(())
//! # }
//! ```
//!
//! ### In Child Process
//!
//! ```rust,no_run
//! use tui_testing::TuiTestClient;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut client = TuiTestClient::connect("tcp://127.0.0.1:5555").await?;
//!
//! // Request a screenshot at key moments
//! client.request_screenshot("initial_screen").await?;
//!
//! // Continue with application logic...
//! # Ok(())
//! # }
//! ```
//!
//! ## Integration with Mock Agent
//!
//! The framework integrates with the mock agent for scenario-based testing:
//!
//! ```bash
//! # Run agent with TUI testing enabled
//! ah agent start --agent mock --agent-flags="--tui-testing-uri=tcp://127.0.0.1:5555"
//! ```

pub mod client;
pub mod runner;

pub use runner::{TestedTerminalProgram, TuiTestRunner};
pub mod protocol;

#[cfg(test)]
mod integration_tests;

pub use client::TuiTestClient;

/// Re-export expectrl utilities for convenience
pub use expectrl;

/// Re-export vt100 parser for terminal emulation
pub use vt100;

/// Re-export insta for snapshot testing
pub use insta;

/// Re-export regex for content normalization
pub use regex;

/// Re-export ratatui types for testing
/// Re-export expectrl session types for process control
pub use expectrl::session;

/// Re-export crossterm for input simulation
pub use crossterm;
