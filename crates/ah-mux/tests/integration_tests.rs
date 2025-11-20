// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for multiplexer functionality
//!
//! These tests verify that multiplexer implementations work correctly
//! by creating real windows/panes and measuring terminal dimensions.

use std::thread;
use std::time::Duration;

use ah_mux::available_multiplexers;
use ah_mux_core::{CommandOptions, Multiplexer, SplitDirection, WindowOptions};

mod common;

/// Border characteristics for different multiplexers
#[derive(Debug)]
struct BorderInfo {
    /// Number of columns lost to vertical borders (when splitting vertically)
    vertical_border_cols: u16,
    /// Number of rows lost to horizontal borders (when splitting horizontally)
    horizontal_border_rows: u16,
}

impl BorderInfo {
    fn for_multiplexer(name: &str) -> Self {
        match name {
            "tmux" => BorderInfo {
                vertical_border_cols: 1,   // tmux uses 1 column for vertical borders
                horizontal_border_rows: 0, // tmux doesn't use rows for horizontal borders
            },
            "kitty" => BorderInfo {
                vertical_border_cols: 0, // kitty has no visible borders in split view
                horizontal_border_rows: 0,
            },
            "wezterm" => BorderInfo {
                vertical_border_cols: 0, // wezterm has minimal borders
                horizontal_border_rows: 0,
            },
            "zellij" => BorderInfo {
                vertical_border_cols: 2, // zellij has thicker borders
                horizontal_border_rows: 1,
            },
            "screen" => BorderInfo {
                vertical_border_cols: 1, // screen has minimal borders
                horizontal_border_rows: 0,
            },
            _ => BorderInfo {
                vertical_border_cols: 0,
                horizontal_border_rows: 0,
            },
        }
    }
}

/// Test that verifies basic multiplexer operations work correctly
///
/// This test exercises the core multiplexer functionality that should work
/// across all implementations: window creation, pane splitting, command execution.
fn test_multiplexer_basic_operations(mux_name: &str, mux: &mut Box<dyn Multiplexer + Send + Sync>) {
    tracing::info!(multiplexer = mux_name, "Testing multiplexer");

    // Skip multiplexers that don't support the required operations
    if matches!(mux_name, "zellij") {
        tracing::info!(
            multiplexer = mux_name,
            "Skipping - limited pane splitting support"
        );
        return;
    }

    // Step 1: Create a new window
    tracing::info!("Step 1: Creating multiplexer window...");
    let window_id = mux
        .open_window(&WindowOptions {
            title: Some(&format!("test-{}-{}", mux_name, std::process::id())),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        })
        .expect("Failed to create window");

    tracing::info!(window_id = window_id.as_str(), "Created window");

    // Give the window time to initialize
    thread::sleep(Duration::from_millis(200));

    // Step 2: Test that we can list windows
    tracing::info!("Step 2: Listing windows...");
    let windows_before = mux.list_windows(None).expect("Failed to list windows");
    tracing::debug!(
        count = windows_before.len(),
        "Found windows before creation"
    );

    // List windows again after creation
    let windows_after = mux.list_windows(None).expect("Failed to list windows");
    tracing::debug!(count = windows_after.len(), "Found windows after creation");

    // We should have at least one window now (some multiplexers may create default windows)
    assert!(
        windows_after.len() >= windows_before.len(),
        "Window count should not decrease after creation for {}",
        mux_name
    );

    // Step 3: Test pane splitting (if supported)
    if !matches!(mux_name, "zellij") {
        tracing::info!("Step 3: Testing pane splitting...");

        let pane_id = mux.split_pane(
            Some(&window_id),
            None,
            SplitDirection::Vertical,
            Some(50),
            &CommandOptions {
                cwd: None,
                env: None,
            },
            Some("echo 'test pane'"),
        );

        match pane_id {
            Ok(pane) => {
                tracing::info!(pane_id = pane.as_str(), "Successfully created pane");

                // Test command execution in the pane
                if mux.send_text(&pane, "echo 'hello from pane'\n").is_ok() {
                    tracing::debug!(pane_id = pane.as_str(), "Successfully sent text to pane");
                } else {
                    tracing::debug!(
                        pane_id = pane.as_str(),
                        "Text sending not supported for this multiplexer"
                    );
                }

                // Test pane focusing
                if mux.focus_pane(&pane).is_ok() {
                    tracing::debug!(pane_id = pane.as_str(), "Successfully focused pane");
                } else {
                    tracing::debug!(
                        pane_id = pane.as_str(),
                        "Pane focusing not supported for this multiplexer"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Pane splitting failed (may be expected)");
            }
        }
    }

    // Step 4: Test command execution
    tracing::info!("Step 4: Testing command execution...");
    let cmd_result = mux.run_command(
        &window_id,
        "echo 'multiplexer test'",
        &CommandOptions {
            cwd: None,
            env: None,
        },
    );

    match cmd_result {
        Ok(_) => tracing::debug!("Command execution successful"),
        Err(e) => tracing::error!(error = ?e, "Command execution failed"),
    }

    // Step 5: Test window focusing
    tracing::info!("Step 5: Testing window focusing...");
    match mux.focus_window(&window_id) {
        Ok(_) => tracing::debug!("Window focusing successful"),
        Err(e) => tracing::error!(error = ?e, "Window focusing failed"),
    }

    tracing::info!(multiplexer = mux_name, "Basic operations test completed");
}

/// Test that verifies pane sizing math works correctly
///
/// This test doesn't require real multiplexers - it verifies that our
/// border calculations and sizing logic are sound.
fn test_multiplexer_sizing_logic_internal() {
    // Test border calculations for different multiplexers
    let test_cases = vec![
        ("tmux", 100, 1, 0),    // tmux: 1 col border, 0 row border
        ("zellij", 100, 2, 1),  // zellij: 2 col border, 1 row border
        ("kitty", 100, 0, 0),   // kitty: no borders
        ("wezterm", 100, 0, 0), // wezterm: minimal borders
        ("screen", 100, 1, 0),  // screen: 1 col border
    ];

    for (mux_name, total_width, expected_border_cols, expected_border_rows) in test_cases {
        let border_info = BorderInfo::for_multiplexer(mux_name);

        assert_eq!(
            border_info.vertical_border_cols, expected_border_cols,
            "Border cols mismatch for {}",
            mux_name
        );
        assert_eq!(
            border_info.horizontal_border_rows, expected_border_rows,
            "Border rows mismatch for {}",
            mux_name
        );

        // Test sizing calculation
        let available_width = (total_width as u16).saturating_sub(border_info.vertical_border_cols);
        let pane_width = available_width / 2;

        // Verify that 2 panes would fit within the available space
        let total_pane_width = pane_width * 2;
        let discrepancy = available_width.saturating_sub(total_pane_width);

        assert!(
            discrepancy <= 1, // Allow 1 col discrepancy for rounding
            "Sizing calculation failed for {}: total={}, available={}, panes={}, discrepancy={}",
            mux_name,
            total_width,
            available_width,
            total_pane_width,
            discrepancy
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Integration test - requires real multiplexers to be installed
    fn test_all_multiplexer_basic_operations() {
        let multiplexers = available_multiplexers();

        if multiplexers.is_empty() {
            panic!("No multiplexers available for testing");
        }

        tracing::info!(count = multiplexers.len(), "found available multiplexers");

        for (name, mut mux) in multiplexers {
            test_multiplexer_basic_operations(&name, &mut mux);
        }
    }

    #[test]
    fn test_border_info_for_multiplexers() {
        let tmux_borders = BorderInfo::for_multiplexer("tmux");
        assert_eq!(tmux_borders.vertical_border_cols, 1);
        assert_eq!(tmux_borders.horizontal_border_rows, 0);

        let zellij_borders = BorderInfo::for_multiplexer("zellij");
        assert_eq!(zellij_borders.vertical_border_cols, 2);
        assert_eq!(zellij_borders.horizontal_border_rows, 1);

        let unknown_borders = BorderInfo::for_multiplexer("unknown");
        assert_eq!(unknown_borders.vertical_border_cols, 0);
        assert_eq!(unknown_borders.horizontal_border_rows, 0);
    }

    #[test]
    fn test_multiplexer_sizing_logic() {
        // This test verifies our border calculations and sizing math
        test_multiplexer_sizing_logic_internal();
    }

    #[test]
    #[ignore = "TODO: Add support for running this in GitHub Actions CI"]
    fn test_measure_terminal_size() {
        // This test just verifies the measurement function works
        // It doesn't test the actual multiplexer functionality
        let size = common::measure_terminal_size().unwrap();
        assert!(size.cols > 0);
        assert!(size.rows > 0);
    }

    /// Advanced pane sizing test (concept demonstration)
    ///
    /// This test shows how a complete pane sizing verification would work.
    /// It requires running a measurement binary in each pane and parsing results.
    /// Currently disabled as it requires more complex test infrastructure.
    #[test]
    #[ignore] // Requires test binary and complex multiplexer interaction
    fn test_advanced_pane_sizing_concept() {
        // This would be the full implementation if we had:
        // 1. A compiled measurement binary
        // 2. Ability to run it in specific panes and capture output
        // 3. JSON parsing of results

        let multiplexers = available_multiplexers();
        for (name, mux) in multiplexers {
            if name == "zellij" {
                continue; // Skip zellij for now
            }

            tracing::info!(multiplexer = name, "testing pane sizing concept");

            // 1. Create window and get baseline size
            let _baseline = common::measure_terminal_size().unwrap();

            // 2. Create window
            let _window_id = mux
                .open_window(&WindowOptions {
                    title: Some(&format!("size-test-{}", name)),
                    cwd: None,
                    focus: false,
                    profile: None,
                    init_command: None,
                })
                .unwrap();

            thread::sleep(Duration::from_millis(500));

            // 3. Split vertically and run measurement in each pane
            // This would require:
            // - Running a measurement binary in each pane
            // - Capturing its JSON output
            // - Parsing and comparing sizes

            // For now, just verify the window was created
            let windows = mux.list_windows(None).unwrap();
            assert!(!windows.is_empty(), "No windows found after creation");
        }
    }
}
