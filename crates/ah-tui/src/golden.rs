// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Golden snapshot testing functionality for TUI tests
//!
//! This module provides the ability to save, load, and compare golden snapshots
//! of TUI buffer content for regression testing.

use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::PathBuf;

/// Golden file management for golden file testing
pub struct GoldenManager {
    base_dir: PathBuf,
    update_mode: bool,
}

impl GoldenManager {
    /// Create a new golden manager
    pub fn new(update_mode: bool) -> Self {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("ah-tui")
            .join("tests")
            .join("__goldens__");

        Self {
            base_dir,
            update_mode,
        }
    }

    /// Get the path for a golden file
    fn golden_path(&self, scenario_name: &str, step_name: &str) -> PathBuf {
        self.base_dir.join(scenario_name).join(format!("{}.golden", step_name))
    }

    /// Ensure the directory for a golden exists
    fn ensure_golden_dir(&self, scenario_name: &str) -> std::io::Result<()> {
        let dir = self.base_dir.join(scenario_name);
        fs::create_dir_all(&dir)
    }

    /// Save a golden to disk
    pub fn save_golden(
        &self,
        scenario_name: &str,
        step_name: &str,
        content: &str,
    ) -> Result<(), String> {
        self.ensure_golden_dir(scenario_name)
            .map_err(|e| format!("Failed to create golden directory: {}", e))?;

        let path = self.golden_path(scenario_name, step_name);
        fs::write(&path, content)
            .map_err(|e| format!("Failed to write golden to {}: {}", path.display(), e))?;

        Ok(())
    }

    /// Load a golden from disk
    pub fn load_golden(&self, scenario_name: &str, step_name: &str) -> Result<String, String> {
        let path = self.golden_path(scenario_name, step_name);
        fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read golden from {}: {}", path.display(), e))
    }

    /// Compare a golden with expected content
    pub fn compare_golden(
        &self,
        scenario_name: &str,
        step_name: &str,
        actual_content: &str,
    ) -> Result<(), String> {
        if self.update_mode {
            // In update mode, save the new golden
            self.save_golden(scenario_name, step_name, actual_content)?;
            tracing::info!("✅ Updated golden: {} -> {}", scenario_name, step_name);
            return Ok(());
        }

        // Load the expected golden
        let expected_content = self.load_golden(scenario_name, step_name)?;

        // Normalize both contents for comparison
        let expected_normalized = normalize_golden(&expected_content);
        let actual_normalized = normalize_golden(actual_content);

        if expected_normalized == actual_normalized {
            Ok(())
        } else {
            // Generate a diff for better error reporting
            let diff = TextDiff::from_lines(&expected_normalized, &actual_normalized);
            let mut diff_output = String::new();

            for change in diff.iter_all_changes() {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                diff_output.push_str(&format!("{}{}", sign, change));
            }

            Err(format!(
                "Golden mismatch for {} -> {}\n--- Expected\n+++ Actual\n{}",
                scenario_name, step_name, diff_output
            ))
        }
    }
}

/// Normalize golden content for stable comparisons
///
/// This removes volatile metadata and normalizes whitespace to ensure
/// goldens are stable across different environments and runs.
fn normalize_golden(content: &str) -> String {
    // Split into lines and process each line
    let normalized_lines: Vec<String> = content
        .lines()
        .filter_map(|line| {
            // Remove trailing whitespace
            let trimmed = line.trim_end();
            // Skip empty lines at the end
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect();

    // Remove trailing empty lines
    let mut result = normalized_lines.join("\n");

    // Ensure we end with a newline if the original did
    if content.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_golden_save_load() {
        let temp_dir = tempdir().unwrap();
        let manager = GoldenManager {
            base_dir: temp_dir.path().to_path_buf(),
            update_mode: false,
        };

        let content = "Test\nGolden\nContent\n";

        // Save golden
        manager.save_golden("test_scenario", "test_step", content).unwrap();

        // Load golden
        let loaded = manager.load_golden("test_scenario", "test_step").unwrap();
        assert_eq!(loaded, content);
    }

    #[test]
    fn test_golden_comparison() {
        let temp_dir = tempdir().unwrap();
        let manager = GoldenManager {
            base_dir: temp_dir.path().to_path_buf(),
            update_mode: false,
        };

        let content = "Test\nGolden\nContent\n";

        // Save initial golden
        manager.save_golden("test_scenario", "test_step", content).unwrap();

        // Compare with same content - should pass
        manager.compare_golden("test_scenario", "test_step", content).unwrap();

        // Compare with different content - should fail
        let different_content = "Different\nContent\n";
        let result = manager.compare_golden("test_scenario", "test_step", different_content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Golden mismatch"));
    }

    #[test]
    fn test_update_mode() {
        let temp_dir = tempdir().unwrap();
        let manager = GoldenManager {
            base_dir: temp_dir.path().to_path_buf(),
            update_mode: true,
        };

        let original_content = "Original\nContent\n";
        let updated_content = "Updated\nContent\n";

        // Save original
        manager.save_golden("test_scenario", "test_step", original_content).unwrap();

        // Update in update mode
        manager.compare_golden("test_scenario", "test_step", updated_content).unwrap();

        // Verify it was updated
        let loaded = manager.load_golden("test_scenario", "test_step").unwrap();
        assert_eq!(loaded, updated_content);
    }

    #[test]
    fn test_normalize_golden() {
        let input = "Line 1  \nLine 2\t\n  \nLine 3\n";
        let expected = "Line 1\nLine 2\nLine 3\n";

        let normalized = normalize_golden(input);
        assert_eq!(normalized, expected);
    }
}
