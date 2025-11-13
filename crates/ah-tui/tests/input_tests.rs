// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use ah_tui::settings::{KeyboardOperation, KeymapConfig};
use ah_tui::view_model::input::{InputMinorMode, InputResult, minor_modes, operations};
use ah_tui::view_model::session_viewer_model::SESSION_VIEWER_MODE;

/// Test helper to create a mock settings object with minimal key bindings
fn create_mock_settings() -> ah_tui::Settings {
    use ah_tui::settings::KeyMatcher;

    // Create KeyMatchers for our test bindings
    let down_matcher = KeyMatcher::new(KeyCode::Down, KeyModifiers::NONE, KeyModifiers::NONE, None);
    let up_matcher = KeyMatcher::new(KeyCode::Up, KeyModifiers::NONE, KeyModifiers::NONE, None);
    let esc_matcher = KeyMatcher::new(KeyCode::Esc, KeyModifiers::NONE, KeyModifiers::NONE, None);
    let ctrl_a_matcher = KeyMatcher::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL,
        KeyModifiers::NONE,
        None,
    );

    // Create keymap with bindings
    let mut keymap = KeymapConfig::default();
    keymap.move_to_next_line = Some(vec![down_matcher]);
    keymap.move_to_previous_line = Some(vec![up_matcher]);
    keymap.dismiss_overlay = Some(vec![esc_matcher]);
    keymap.select_all = Some(vec![ctrl_a_matcher]);

    ah_tui::Settings {
        keymap: Some(keymap),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_minor_mode_creation() {
        let mode = &SESSION_VIEWER_MODE;
        assert!(mode.handles_operation(&KeyboardOperation::MoveToNextLine));
        assert!(mode.handles_operation(&KeyboardOperation::MoveToPreviousLine));
        assert!(!mode.handles_operation(&KeyboardOperation::SelectAll));
    }

    #[test]
    fn test_input_minor_mode_with_prominent_operations() {
        let mode = &minor_modes::SELECTION_PROMINENT_MODE;
        assert!(mode.handles_operation(&KeyboardOperation::SelectAll));
        assert!(mode.handles_operation(&KeyboardOperation::DismissOverlay));
        assert!(mode.handles_operation(&KeyboardOperation::NewDraft));
        assert_eq!(mode.prominent_operations().len(), 3);
        assert!(mode.prominent_operations().contains(&KeyboardOperation::DismissOverlay));
        assert!(mode.prominent_operations().contains(&KeyboardOperation::SelectAll));
        assert!(mode.prominent_operations().contains(&KeyboardOperation::NewDraft));
    }

    #[test]
    fn test_resolve_key_to_operation() {
        let settings = create_mock_settings();

        // Test navigation operations
        let down_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let up_event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);

        assert_eq!(
            SESSION_VIEWER_MODE.resolve_key_to_operation(&down_event, &settings),
            Some(KeyboardOperation::MoveToNextLine)
        );
        assert_eq!(
            SESSION_VIEWER_MODE.resolve_key_to_operation(&up_event, &settings),
            Some(KeyboardOperation::MoveToPreviousLine)
        );

        // Test that operations not in the mode are not resolved
        assert_eq!(
            minor_modes::SELECTION_MODE.resolve_key_to_operation(&down_event, &settings),
            None
        );
    }

    #[test]
    fn test_resolve_key_to_operation_with_selection() {
        let settings = create_mock_settings();

        let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let ctrl_a_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);

        assert_eq!(
            minor_modes::SELECTION_MODE.resolve_key_to_operation(&esc_event, &settings),
            Some(KeyboardOperation::DismissOverlay)
        );
        assert_eq!(
            minor_modes::SELECTION_MODE.resolve_key_to_operation(&ctrl_a_event, &settings),
            Some(KeyboardOperation::SelectAll)
        );
    }

    #[test]
    fn test_clone_and_debug() {
        let mode = &minor_modes::NAVIGATION_PROMINENT_MODE;

        // Test that static constants work and have expected properties
        assert!(mode.handles_operation(&KeyboardOperation::MoveToNextLine));
        assert!(mode.handles_operation(&KeyboardOperation::MoveToPreviousLine));
        assert_eq!(mode.prominent_operations().len(), 4); // Basic navigation operations

        // Test Debug on the static constant
        let debug_str = format!("{:?}", mode);
        assert!(debug_str.contains("InputMinorMode"));
    }

    #[test]
    fn test_default_mode() {
        let mode = InputMinorMode::default();
        assert_eq!(mode.supported_operations().count(), 0);
        assert_eq!(mode.prominent_operations().len(), 0);
    }

    #[test]
    fn test_operations_constants() {
        // Test that our operation constants contain expected operations
        assert!(operations::NAVIGATION.contains(&KeyboardOperation::MoveToNextLine));
        assert!(operations::NAVIGATION.contains(&KeyboardOperation::MoveToPreviousLine));
        assert!(operations::SELECTION.contains(&KeyboardOperation::DismissOverlay));
        assert!(operations::SELECTION.contains(&KeyboardOperation::SelectAll));
        assert!(operations::TEXT_EDITING.contains(&KeyboardOperation::DeleteCharacterForward));
    }

    #[test]
    fn test_resolve_unbound_key() {
        let settings = create_mock_settings();

        // Test a key that has no binding
        let unbound_event = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE);

        assert_eq!(
            SESSION_VIEWER_MODE.resolve_key_to_operation(&unbound_event, &settings),
            None
        );
    }

    #[test]
    fn test_resolve_key_outside_mode() {
        let settings = create_mock_settings();

        // ESC is bound but not in SESSION_VIEWER_MODE mode
        let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);

        assert_eq!(
            SESSION_VIEWER_MODE.resolve_key_to_operation(&esc_event, &settings),
            None
        );

        // But it should work in SELECTION mode
        assert_eq!(
            minor_modes::SELECTION_MODE.resolve_key_to_operation(&esc_event, &settings),
            Some(KeyboardOperation::DismissOverlay)
        );
    }
}
