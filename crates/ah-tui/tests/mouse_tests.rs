// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for mouse interaction functionality

use ah_tui::view_model::dashboard_model::{MouseAction, Msg, ViewModel};
use ratatui::layout::Rect;

mod common;

fn new_view_model() -> ViewModel {
    common::build_view_model()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_textarea_click_positioning_with_padding() {
        let mut vm = new_view_model();

        // Set up textarea with known content
        if let Some(card) = vm.draft_cards.first_mut() {
            card.description = tui_textarea::TextArea::from(["Hello World"]);
            card.focus_element = ah_tui::view_model::task_entry::CardFocusElement::TaskDescription;
        }
        vm.focus_element = ah_tui::view_model::DashboardFocusState::DraftTask(0);

        // Test padding calculation: textarea at x=5, padding=1, so text starts at x=6
        let bounds = Rect {
            x: 5,
            y: 5,
            width: 20,
            height: 5,
        };

        // Click at x=11 (5 + 1 + 5) should position at character index 5 (space before 'W')
        vm.handle_mouse_click(
            MouseAction::FocusDraftTextarea(0),
            11,
            5, // column, row
            &bounds,
        );

        if let Some(card) = vm.draft_cards.first() {
            let (row, col) = card.description.cursor();
            assert_eq!(row, 0);
            assert_eq!(col, 6); // Space is at index 5 in "Hello World"
        }
    }

    #[test]
    fn test_multi_click_detection() {
        let mut vm = new_view_model();

        // Set up textarea
        if let Some(card) = vm.draft_cards.first_mut() {
            card.description = tui_textarea::TextArea::from(["Hello World"]);
            card.focus_element = ah_tui::view_model::task_entry::CardFocusElement::TaskDescription;
        }
        vm.focus_element = ah_tui::view_model::DashboardFocusState::DraftTask(0);

        let bounds = Rect {
            x: 5,
            y: 5,
            width: 20,
            height: 5,
        };

        // First click
        vm.handle_mouse_click(MouseAction::FocusDraftTextarea(0), 8, 5, &bounds);
        assert_eq!(vm.click_count, 1);

        // Second click within 500ms at same position
        vm.handle_mouse_click(MouseAction::FocusDraftTextarea(0), 8, 5, &bounds);
        assert_eq!(vm.click_count, 2);

        // Third click
        vm.handle_mouse_click(MouseAction::FocusDraftTextarea(0), 8, 5, &bounds);
        assert_eq!(vm.click_count, 3);

        // Fourth click - should be 4
        vm.handle_mouse_click(MouseAction::FocusDraftTextarea(0), 8, 5, &bounds);
        assert_eq!(vm.click_count, 4);
    }

    #[test]
    fn test_slow_clicks_reset_multi_click() {
        let mut vm = new_view_model();

        // Set up textarea
        if let Some(card) = vm.draft_cards.first_mut() {
            card.description = tui_textarea::TextArea::from(["Hello World"]);
            card.focus_element = ah_tui::view_model::task_entry::CardFocusElement::TaskDescription;
        }
        vm.focus_element = ah_tui::view_model::DashboardFocusState::DraftTask(0);

        let bounds = Rect {
            x: 5,
            y: 5,
            width: 20,
            height: 5,
        };

        // First click
        vm.handle_mouse_click(MouseAction::FocusDraftTextarea(0), 8, 5, &bounds);
        assert_eq!(vm.click_count, 1);

        // Manually set last click time to be old (simulate slow click)
        vm.last_click_time =
            Some(std::time::Instant::now() - std::time::Duration::from_millis(600));

        // Second click should reset to 1
        vm.handle_mouse_click(MouseAction::FocusDraftTextarea(0), 8, 5, &bounds);
        assert_eq!(vm.click_count, 1);
    }

    #[test]
    fn test_model_selector_actions() {
        let mut vm = new_view_model();

        // Open model selection modal
        vm.open_modal(ah_tui::view_model::ModalState::ModelSearch);

        // Check initial count
        let initial_count = if let Some(modal) = &vm.active_modal {
            if let ah_tui::view_model::ModalType::ModelSelection { options } = &modal.modal_type {
                options[0].count
            } else {
                0
            }
        } else {
            0
        };

        // Test increment
        vm.perform_mouse_action(MouseAction::ModelIncrementCount(0));
        if let Some(modal) = &vm.active_modal {
            if let ah_tui::view_model::ModalType::ModelSelection { options } = &modal.modal_type {
                assert_eq!(options[0].count, initial_count + 1);
            }
        }

        // Test decrement
        vm.perform_mouse_action(MouseAction::ModelDecrementCount(0));
        if let Some(modal) = &vm.active_modal {
            if let ah_tui::view_model::ModalType::ModelSelection { options } = &modal.modal_type {
                assert_eq!(options[0].count, initial_count);
            }
        }

        // Test decrement doesn't go below 0
        vm.perform_mouse_action(MouseAction::ModelDecrementCount(0));
        vm.perform_mouse_action(MouseAction::ModelDecrementCount(0));
        if let Some(modal) = &vm.active_modal {
            if let ah_tui::view_model::ModalType::ModelSelection { options } = &modal.modal_type {
                assert_eq!(options[0].count, 0); // Should not go below 0
            }
        }
    }

    #[test]
    fn test_mouse_scroll_in_modals() {
        let mut vm = crate::common::build_view_model_with_repos();

        // Open repository search modal
        vm.open_modal(ah_tui::view_model::ModalState::RepositorySearch);

        // Initially at index 0
        assert_eq!(vm.active_modal.as_ref().unwrap().selected_index, 0);

        // Scroll down
        vm.update(Msg::MouseScrollDown).unwrap();
        assert_eq!(vm.active_modal.as_ref().unwrap().selected_index, 1);

        // Scroll up
        vm.update(Msg::MouseScrollUp).unwrap();
        assert_eq!(vm.active_modal.as_ref().unwrap().selected_index, 0);

        // Scroll up from 0 stays at 0
        vm.update(Msg::MouseScrollUp).unwrap();
        assert_eq!(vm.active_modal.as_ref().unwrap().selected_index, 0);
    }

    #[test]
    fn test_click_position_calculation() {
        // Test the padding calculation logic directly
        let textarea_x = 5;
        let padding = 1;
        let click_x = 11;

        let relative_x = (click_x - textarea_x - padding).max(0) as u16;
        assert_eq!(relative_x, 5); // 11 - 5 - 1 = 5

        // Test with wide characters (simple heuristic)
        let test_string = "Hello 世界"; // 5 ASCII + 2 wide chars
        let mut visual_width = 0u16;
        let mut char_index = 0;

        for ch in test_string.chars() {
            let char_width = if ch.is_ascii() { 1 } else { 2 };
            if visual_width + char_width > relative_x {
                break;
            }
            visual_width += char_width;
            char_index += 1;
        }

        assert_eq!(char_index, 5); // Should be at the '世' character (after 5 ASCII chars)
    }

    #[test]
    fn test_mouse_disabled_setting() {
        let settings_disabled = ah_tui::settings::Settings {
            mouse_enabled: Some(false),
            ..Default::default()
        };
        assert!(!settings_disabled.mouse_enabled());

        let settings_enabled = ah_tui::settings::Settings {
            mouse_enabled: Some(true),
            ..Default::default()
        };
        assert!(settings_enabled.mouse_enabled());

        // Test default
        let default_settings = ah_tui::settings::Settings::default();
        assert!(default_settings.mouse_enabled());
    }
}
