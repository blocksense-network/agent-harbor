// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Interactive editor integration for task content creation.
//!
//! This module provides functionality for interactively editing task content using
//! the user's preferred text editor, with fallback to common editors. It handles
//! temporary file creation, template processing, and validation.

use std::env;
use std::fs;
use std::io::Write;
use std::process::Command;

/// Template text added to temporary files when editing tasks interactively.
///
/// This exact text must be present in the template for proper comment stripping.
pub const EDITOR_HINT: &str = r#"# Please write your task prompt above.
# Enter an empty prompt to abort the task creation process.
# Feel free to leave this comment in the file. It will be ignored.
# Saving and exiting will deliver work according to the configured delivery method."#;

/// Error type for editor operations.
#[derive(Debug, thiserror::Error)]
pub enum EditorError {
    #[error("Failed to find a suitable text editor")]
    NoEditorFound,
    #[error("Editor failed to start or exited with error")]
    EditorFailed,
    #[error("Failed to create temporary file: {0}")]
    TempFileError(#[from] std::io::Error),
    #[error("Aborted: empty task prompt.")]
    EmptyTaskPrompt,
    #[error("Non-interactive environment, cannot prompt for input")]
    NonInteractive,
}

/// Result type for editor operations.
pub type EditorResult<T> = std::result::Result<T, EditorError>;

/// Discover the user's preferred text editor.
///
/// This function implements the same editor discovery logic as the Ruby implementation:
/// 1. Check the EDITOR environment variable
/// 2. Fall back to common editors in order: nano, pico, micro, vim, helix, vi
/// 3. Default to nano if none are found
///
/// # Returns
/// The name of the editor command to use.
pub fn discover_editor() -> &'static str {
    // First try EDITOR environment variable
    if let Ok(editor) = env::var("EDITOR") {
        if !editor.trim().is_empty() {
            return Box::leak(editor.into_boxed_str());
        }
    }

    // Fall back to common editors in order
    let editors = ["nano", "pico", "micro", "vim", "helix", "vi"];

    for editor in &editors {
        if editor_available(editor) {
            return editor;
        }
    }

    // Final fallback
    "nano"
}

/// Check if an editor command is available in PATH.
///
/// # Arguments
/// * `editor` - The editor command name to check
///
/// # Returns
/// `true` if the editor is available, `false` otherwise
fn editor_available(editor: &str) -> bool {
    Command::new("command")
        .args(["-v", editor])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Edit task content interactively using the user's editor.
///
/// This function creates a temporary file with the editor hint template,
/// opens it in the user's preferred editor, reads the result back,
/// processes the template, and validates the content.
///
/// # Arguments
/// * `initial_content` - Optional initial content to pre-fill in the editor
///
/// # Returns
/// The processed task content, or an error if editing fails or content is empty.
///
/// # Errors
/// Returns `EditorError::EmptyTaskPrompt` if the processed content is empty.
/// Returns `EditorError::EditorFailed` if the editor exits with a non-zero status.
/// Returns `EditorError::NoEditorFound` if no suitable editor is available.
pub fn edit_content_interactive(initial_content: Option<&str>) -> EditorResult<String> {
    edit_content_interactive_with_hint(initial_content, EDITOR_HINT)
}

/// Interactive editor flow with a custom hint block (used by CLI to respect comment prefixes).
pub fn edit_content_interactive_with_hint(
    initial_content: Option<&str>,
    hint: &str,
) -> EditorResult<String> {
    let editor = discover_editor();

    // Create temporary file
    let mut temp_file = tempfile::NamedTempFile::new()?;
    let temp_path = temp_file.path().to_path_buf();

    // Write initial content and template
    if let Some(content) = initial_content {
        write!(temp_file, "{}\n\n", content)?;
    } else {
        writeln!(temp_file)?;
    }
    write!(temp_file, "{}", hint)?;
    temp_file.flush()?;

    // Close the file so the editor can access it
    drop(temp_file);

    // Run the editor
    let status = Command::new(editor)
        .arg(&temp_path)
        .status()
        .map_err(|_| EditorError::EditorFailed)?;

    if !status.success() {
        return Err(EditorError::EditorFailed);
    }

    // Read the result back
    let content = fs::read_to_string(&temp_path)?;

    // Process the template
    let processed = process_template(content);

    // Validate content
    if processed.trim().is_empty() {
        return Err(EditorError::EmptyTaskPrompt);
    }

    Ok(processed)
}

/// Process editor template by removing hints and normalizing line endings.
///
/// This function performs the same processing as the Ruby implementation:
/// 1. Remove the EDITOR_HINT text (with or without leading newline)
/// 2. Normalize line endings from CRLF to LF
///
/// # Arguments
/// * `content` - The raw content from the edited file
///
/// # Returns
/// The processed content with template hints removed and line endings normalized
fn process_template(mut content: String) -> String {
    // Remove the hint with leading newline (most common case)
    content = content.replace(&format!("\n{}", EDITOR_HINT), "");

    // Remove the hint without leading newline (edge case)
    content = content.replace(EDITOR_HINT, "");

    // Normalize line endings from CRLF to LF
    content.replace("\r\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::NamedTempFile;

    #[test]
    fn test_discover_editor_with_env_var() {
        // Test with EDITOR set
        env::set_var("EDITOR", "vim");
        assert_eq!(discover_editor(), "vim");
        env::remove_var("EDITOR");

        // Test with empty EDITOR
        env::set_var("EDITOR", "");
        assert_ne!(discover_editor(), "");
        env::remove_var("EDITOR");
    }

    #[test]
    fn test_editor_available() {
        // Should be able to check availability of editors
        // Note: In some environments, no editors may be available, which is fine
        let _ = editor_available("nano");
        let _ = editor_available("vi");
        let _ = editor_available("vim");
    }

    #[test]
    fn test_process_template() {
        let hint_with_newline = format!("\n{}", EDITOR_HINT);
        let hint_without_newline = EDITOR_HINT.to_string();

        // Test with leading newline
        let input1 = format!("Some content{}", hint_with_newline);
        assert_eq!(process_template(input1), "Some content");

        // Test without leading newline
        let input2 = format!("Some content{}", hint_without_newline);
        assert_eq!(process_template(input2), "Some content");

        // Test CRLF normalization
        let input3 = "Line 1\r\nLine 2\r\n".to_string();
        assert_eq!(process_template(input3), "Line 1\nLine 2\n");

        // Test both hint removal and CRLF normalization
        let input4 = format!("Task content\r\nMore content{}", hint_with_newline);
        assert_eq!(process_template(input4), "Task content\nMore content");
    }

    #[test]
    fn test_process_template_empty_content() {
        // Empty content should remain empty
        assert_eq!(process_template(String::new()), "");

        // Only hint should be removed, leaving empty content
        let input = EDITOR_HINT.to_string();
        assert_eq!(process_template(input), "");

        // Hint with leading newline should also leave empty content
        let input = format!("\n{}", EDITOR_HINT);
        assert_eq!(process_template(input), "");
    }

    #[test]
    fn test_empty_validation() {
        // Create a temporary file with only the hint
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", EDITOR_HINT).unwrap();
        temp_file.flush().unwrap();

        let content = fs::read_to_string(temp_file.path()).unwrap();
        let processed = process_template(content);
        assert_eq!(processed.trim(), "");

        // This should fail validation
        assert!(processed.trim().is_empty());
    }
}
