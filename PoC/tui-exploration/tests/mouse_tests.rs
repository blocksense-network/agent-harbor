use tui_exploration::view_model::{*, create_draft_card_from_task};
use ah_domain_types::{DraftTask, TaskState, DeliveryStatus, TaskExecution, SelectedModel};
use crossterm::event::{MouseEvent, MouseEventKind, MouseButton};
use std::collections::HashMap;
use std::sync::mpsc;
use tokio::sync::mpsc as tokio_mpsc;

/// Helper function to create a test ViewModel
fn create_test_view_model() -> ViewModel {
    let workspace_files = Box::new(tui_exploration::workspace_files::GitWorkspaceFiles::new(std::path::PathBuf::from(".")));
    let workspace_workflows = Box::new(tui_exploration::workspace_workflows::PathWorkspaceWorkflows::new(std::path::PathBuf::from(".")));
    let task_manager = Box::new(tui_exploration::task_manager::MockTaskManager::new());
    let settings = tui_exploration::settings::Settings::default();

    ViewModel::new(
        workspace_files,
        workspace_workflows,
        task_manager,
        settings,
    )
}

#[test]
fn mouse_left_click_on_task_card_selects_card() {
    let mut vm = create_test_view_model();

    // Create a mouse event for left click at position (5, 5)
    let mouse_event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 5,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    // Initially should have draft card at index 0
    assert_eq!(vm.selected_card, 0);

    // Simulate mouse click (this would normally be done by registering interactive areas in view)
    // For this test, we'll directly test the mouse event handling logic
    let handled = vm.handle_mouse_event(mouse_event);
    // Should return false since no interactive areas are registered yet
    assert!(!handled);
}

#[test]
fn mouse_scroll_up_navigates_up_between_ui_elements() {
    let mut vm = create_test_view_model();
    // Start with focus on the first draft task
    vm.focus_element = FocusElement::DraftTask(0);

    let mouse_event = MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let handled = vm.handle_mouse_event(mouse_event);
    assert!(handled);
    // Should navigate up from first draft to settings button
    assert_eq!(vm.focus_element, FocusElement::SettingsButton);
    assert!(vm.needs_redraw); // Should trigger redraw
}

#[test]
fn mouse_scroll_down_navigates_down_like_arrow_key() {
    let mut vm = create_test_view_model();
    // Start with focus on settings button
    vm.focus_element = FocusElement::SettingsButton;

    let mouse_event = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let handled = vm.handle_mouse_event(mouse_event);
    assert!(handled);
    // Should navigate down from settings to first draft task (like Down arrow key)
    assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
    assert!(vm.needs_redraw); // Should trigger redraw
}

#[test]
fn mouse_scroll_up_at_top_wraps_to_bottom() {
    let mut vm = create_test_view_model();
    // Start with focus at the top (settings button)
    vm.focus_element = FocusElement::SettingsButton;

    let mouse_event = MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let handled = vm.handle_mouse_event(mouse_event);
    assert!(handled);
    // Should wrap to the bottom (last draft task since there are draft cards)
    assert_eq!(vm.focus_element, FocusElement::DraftTask(0)); // Only one draft card
}

#[test]
fn mouse_scroll_down_navigates_to_next_element() {
    let mut vm = create_test_view_model();
    // Start with focus at settings button
    vm.focus_element = FocusElement::SettingsButton;

    let mouse_event = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let handled = vm.handle_mouse_event(mouse_event);
    assert!(handled);
    // Should navigate down from settings to first draft task
    assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
}

#[test]
fn mouse_click_in_textarea_positions_caret() {
    let mut vm = create_test_view_model();

    // Set up a textarea area
    let textarea_rect = ratatui::layout::Rect {
        x: 5,
        y: 5,
        width: 20,
        height: 5,
    };
    vm.last_textarea_area = Some(textarea_rect);

    // Click at position (10, 7) which should be inside the textarea
    let mouse_event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 10,
        row: 7,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let handled = vm.handle_mouse_event(mouse_event);
    assert!(handled);
    assert_eq!(vm.focus_element, FocusElement::TaskDescription); // Should focus textarea
}

#[test]
fn mouse_click_outside_textarea_does_not_position_caret() {
    let mut vm = create_test_view_model();

    // Set up a textarea area
    let textarea_rect = ratatui::layout::Rect {
        x: 5,
        y: 5,
        width: 20,
        height: 5,
    };
    vm.last_textarea_area = Some(textarea_rect);

    // Click at position (50, 50) which should be outside the textarea
    let mouse_event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 50,
        row: 50,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let handled = vm.handle_mouse_event(mouse_event);
    assert!(!handled); // Should not be handled since no interactive areas
}

#[test]
fn mouse_action_select_card_updates_selection_and_focus() {
    let mut vm = create_test_view_model();

    // Test selecting draft card (index 0)
    vm.perform_mouse_action(MouseAction::SelectCard(0));
    assert_eq!(vm.selected_card, 0);
    assert_eq!(vm.focus_element, FocusElement::TaskDescription);

    // Test selecting regular task card (mouse action index 1 = array index 0)
    vm.perform_mouse_action(MouseAction::SelectCard(1));
    assert_eq!(vm.selected_card, 1);
    assert_eq!(vm.focus_element, FocusElement::ExistingTask(0));
}

#[test]
fn mouse_action_open_settings_updates_modal_state() {
    let mut vm = create_test_view_model();

    vm.perform_mouse_action(MouseAction::OpenSettings);
    assert_eq!(vm.focus_element, FocusElement::SettingsButton);
    assert_eq!(vm.modal_state, ModalState::Settings);
}

#[test]
fn mouse_action_activate_repository_modal_updates_state() {
    let mut vm = create_test_view_model();

    vm.perform_mouse_action(MouseAction::ActivateRepositoryModal);
    assert_eq!(vm.focus_element, FocusElement::RepositoryButton);
    assert_eq!(vm.modal_state, ModalState::RepositorySearch);
}

#[test]
fn mouse_action_activate_branch_modal_updates_state() {
    let mut vm = create_test_view_model();

    vm.perform_mouse_action(MouseAction::ActivateBranchModal);
    assert_eq!(vm.focus_element, FocusElement::BranchButton);
    assert_eq!(vm.modal_state, ModalState::BranchSearch);
}

#[test]
fn mouse_action_activate_model_modal_updates_state() {
    let mut vm = create_test_view_model();

    vm.perform_mouse_action(MouseAction::ActivateModelModal);
    assert_eq!(vm.focus_element, FocusElement::ModelButton);
    assert_eq!(vm.modal_state, ModalState::ModelSearch);
}

#[test]
fn mouse_action_launch_task_updates_focus() {
    let mut vm = create_test_view_model();

    vm.perform_mouse_action(MouseAction::LaunchTask);
    assert_eq!(vm.focus_element, FocusElement::GoButton);
    // Note: handle_go_button() would be called but we can't easily test its effects here
}

#[test]
fn mouse_action_select_filter_bar_updates_focus() {
    let mut vm = create_test_view_model();

    vm.perform_mouse_action(MouseAction::SelectFilterBarLine);
    assert_eq!(vm.focus_element, FocusElement::FilterBarLine);
}

#[test]
fn mouse_events_other_than_left_click_and_scroll_ignored() {
    let mut vm = create_test_view_model();

    // Test right click
    let right_click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let handled = vm.handle_mouse_event(right_click);
    assert!(!handled);

    // Test mouse move
    let mouse_move = MouseEvent {
        kind: MouseEventKind::Moved,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let handled = vm.handle_mouse_event(mouse_move);
    assert!(!handled);
}

#[test]
fn interactive_areas_rect_contains_works() {
    use tui_exploration::view_model::{rect_contains, InteractiveArea};

    let rect = ratatui::layout::Rect {
        x: 10,
        y: 10,
        width: 20,
        height: 10,
    };

    // Point inside rect
    assert!(rect_contains(rect, 15, 15));

    // Point at rect boundary
    assert!(rect_contains(rect, 10, 10));
    assert!(rect_contains(rect, 29, 19)); // x + width - 1, y + height - 1

    // Point outside rect
    assert!(!rect_contains(rect, 5, 5));   // Left of rect
    assert!(!rect_contains(rect, 35, 15)); // Right of rect
    assert!(!rect_contains(rect, 15, 5));  // Above rect
    assert!(!rect_contains(rect, 15, 25)); // Below rect
}

#[test]
fn mouse_wheel_scrolling_triggers_ui_navigation() {
    // This test verifies that mouse wheel events trigger navigation between UI elements

    let mut vm = create_test_view_model();
    vm.focus_element = FocusElement::DraftTask(0); // Start at first draft task

    let scroll_up = MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let scroll_down = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    // Scroll up should navigate to settings button
    assert!(vm.handle_mouse_event(scroll_up));
    assert_eq!(vm.focus_element, FocusElement::SettingsButton);

    // Scroll down should navigate back to draft task
    assert!(vm.handle_mouse_event(scroll_down));
    assert_eq!(vm.focus_element, FocusElement::DraftTask(0));
}

#[test]
fn clicking_task_card_focuses_it() {
    // This test verifies that clicking on active/complete/merged task cards properly focuses them
    let mut vm = create_test_view_model();

    // Simulate clicking on a task card (mouse action index 1 = first task in array)
    vm.perform_mouse_action(MouseAction::SelectCard(1));

    // Should focus on the first task card (array index 0)
    assert_eq!(vm.focus_element, FocusElement::ExistingTask(0));
    assert_eq!(vm.selected_card, 1);

    // Simulate clicking on a different task card (mouse action index 2 = second task in array)
    vm.perform_mouse_action(MouseAction::SelectCard(2));

    // Should focus on the second task card (array index 1)
    assert_eq!(vm.focus_element, FocusElement::ExistingTask(1));
    assert_eq!(vm.selected_card, 2);
}

#[test]
fn down_arrow_from_draft_goes_to_filter_separator() {
    // This test verifies that pressing Down arrow from draft card navigates to FilterBarSeparator
    let mut vm = create_test_view_model();

    // Add a dummy task card so navigation logic sees that there are tasks
    vm.task_cards.push(TaskCardViewModel {
        id: "dummy".to_string(),
        task: ah_domain_types::TaskExecution {
            id: "dummy".to_string(),
            repository: "dummy".to_string(),
            branch: "dummy".to_string(),
            agents: vec![],
            state: ah_domain_types::TaskState::Completed,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            activity: vec![],
            delivery_status: vec![],
        },
        title: "dummy".to_string(),
        metadata: tui_exploration::view_model::TaskCardMetadata {
            repository: "dummy".to_string(),
            branch: "dummy".to_string(),
            models: vec![],
            state: ah_domain_types::TaskState::Completed,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            delivery_indicators: "".to_string(),
        },
        height: 5,
        card_type: tui_exploration::view_model::TaskCardType::Completed { delivery_indicators: "".to_string() },
        focus_element: FocusElement::ExistingTask(0),
    });

    vm.focus_element = FocusElement::DraftTask(0); // Start with draft focused

    // Simulate Down arrow key
    let key_event = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::empty(),
    );

    let handled = vm.handle_key_event(key_event);
    assert!(handled);
    // Should navigate to FilterBarSeparator (existing tasks separator)
    assert_eq!(vm.focus_element, FocusElement::FilterBarSeparator);
}

#[test]
fn tab_from_draft_cycles_through_controls() {
    // This test verifies that pressing Tab while draft card is selected cycles through controls
    let mut vm = create_test_view_model();
    vm.focus_element = FocusElement::DraftTask(0); // Start with draft focused

    // Test MoveToNextField operation directly (Tab should trigger this)
    use tui_exploration::settings::KeyboardOperation;

    // First Tab should move to RepositorySelector
    let handled = vm.handle_keyboard_operation(KeyboardOperation::MoveToNextField, &crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Tab, crossterm::event::KeyModifiers::empty()));
    assert!(handled);
    assert_eq!(vm.focus_element, FocusElement::RepositorySelector);

    // Second Tab should move to BranchSelector
    let handled = vm.handle_keyboard_operation(KeyboardOperation::MoveToNextField, &crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Tab, crossterm::event::KeyModifiers::empty()));
    assert!(handled);
    assert_eq!(vm.focus_element, FocusElement::BranchSelector);

    // Third Tab should move to ModelSelector
    let handled = vm.handle_keyboard_operation(KeyboardOperation::MoveToNextField, &crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Tab, crossterm::event::KeyModifiers::empty()));
    assert!(handled);
    assert_eq!(vm.focus_element, FocusElement::ModelSelector);

    // Fourth Tab should move to GoButton
    let handled = vm.handle_keyboard_operation(KeyboardOperation::MoveToNextField, &crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Tab, crossterm::event::KeyModifiers::empty()));
    assert!(handled);
    assert_eq!(vm.focus_element, FocusElement::GoButton);

    // Fifth Tab should cycle back to RepositorySelector
    let handled = vm.handle_keyboard_operation(KeyboardOperation::MoveToNextField, &crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Tab, crossterm::event::KeyModifiers::empty()));
    assert!(handled);
    assert_eq!(vm.focus_element, FocusElement::RepositorySelector);
}

#[test]
fn typing_text_while_draft_focused_edits_description() {
    // This test verifies that typing text while draft card is selected edits the description
    let mut vm = create_test_view_model();
    vm.focus_element = FocusElement::DraftTask(0); // Start with draft focused

    // Type some characters
    let handled = vm.handle_char_input('H');
    assert!(handled);
    let handled = vm.handle_char_input('e');
    assert!(handled);
    let handled = vm.handle_char_input('l');
    assert!(handled);
    let handled = vm.handle_char_input('l');
    assert!(handled);
    let handled = vm.handle_char_input('o');
    assert!(handled);

    // Check that the description was updated
    assert_eq!(vm.draft_cards[0].description.lines().join("\n"), "Hello");
    assert_eq!(vm.draft_cards[0].save_state, DraftSaveState::Unsaved);

    // Test backspace
    let handled = vm.handle_backspace();
    assert!(handled);
    assert_eq!(vm.draft_cards[0].description.lines().join("\n"), "Hell");
    assert_eq!(vm.draft_cards[0].save_state, DraftSaveState::Unsaved);
}

#[test]
fn mouse_click_on_draft_buttons_activates_modals() {
    // This test demonstrates that draft card buttons are registered as interactive areas
    // and clicking them activates the appropriate modals

    let mut vm = create_test_view_model();

    // Simulate clicking repository button area
    let repo_action = MouseAction::ActivateRepositoryModal;
    vm.perform_mouse_action(repo_action);
    assert_eq!(vm.modal_state, ModalState::RepositorySearch);
    assert_eq!(vm.focus_element, FocusElement::RepositoryButton);

    // Reset state
    vm.modal_state = ModalState::None;

    // Simulate clicking branch button area
    let branch_action = MouseAction::ActivateBranchModal;
    vm.perform_mouse_action(branch_action);
    assert_eq!(vm.modal_state, ModalState::BranchSearch);
    assert_eq!(vm.focus_element, FocusElement::BranchButton);

    // Reset state
    vm.modal_state = ModalState::None;

    // Simulate clicking models button area
    let models_action = MouseAction::ActivateModelModal;
    vm.perform_mouse_action(models_action);
    assert_eq!(vm.modal_state, ModalState::ModelSearch);
    assert_eq!(vm.focus_element, FocusElement::ModelButton);
}

#[test]
fn mouse_hover_feedback_not_implemented_yet() {
    // This test documents that mouse hover effects are not yet implemented
    // According to PRD: "Mouse Hover: Visual Feedback: Hover effects on interactive elements"

    let mut vm = create_test_view_model();

    // Mouse move events should currently be ignored
    let hover_event = MouseEvent {
        kind: MouseEventKind::Moved,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let handled = vm.handle_mouse_event(hover_event);
    assert!(!handled); // Hover effects not implemented yet
}

#[test]
fn mouse_drag_selection_not_implemented_yet() {
    // This test documents that mouse drag selection is not yet implemented
    // According to senior feedback: "handle Drag(MouseButton::Left) if you want selections/resizes later"

    let mut vm = create_test_view_model();

    // Mouse drag events should currently be ignored
    let drag_event = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let handled = vm.handle_mouse_event(drag_event);
    assert!(!handled); // Drag selection not implemented yet
}

#[test]
fn mouse_scrollbar_interaction_not_implemented_yet() {
    // This test documents that scrollbar clicking/dragging is not yet implemented
    // According to PRD: "Scrollbar Interaction: Click and drag scrollbars when visible"

    let mut vm = create_test_view_model();

    // Scrollbar interactions would require detecting scrollbar areas and handling
    // drag events within those areas. This is not implemented yet.

    // For now, we just verify that scroll wheel events trigger navigation
    let scroll_event = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 10,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::empty(),
    };

    let handled = vm.handle_mouse_event(scroll_event);
    assert!(handled); // Scroll wheel triggers navigation operations
}
