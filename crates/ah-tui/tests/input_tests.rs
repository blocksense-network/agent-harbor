// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use ah_tui::settings::{KeyboardOperation, KeymapConfig};
use ah_tui::view_model::input::{InputResult, InputState, InputStateStack, operations};

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
        Some('a'),
    );
    let ctrl_n_matcher = KeyMatcher::new(
        KeyCode::Char('n'),
        KeyModifiers::CONTROL,
        KeyModifiers::NONE,
        Some('n'),
    );

    ah_tui::Settings {
        keymap: Some(KeymapConfig {
            move_to_next_line: Some(vec![down_matcher]),
            move_to_previous_line: Some(vec![up_matcher]),
            dismiss_overlay: Some(vec![esc_matcher]),
            select_all: Some(vec![ctrl_a_matcher]),
            new_draft: Some(vec![ctrl_n_matcher]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn dummy_key_event() -> KeyEvent {
    KeyEvent::new(KeyCode::Null, KeyModifiers::NONE)
}

/// Test helper to create mock settings where Ctrl+N is bound to both NewDraft and DismissOverlay
/// This allows testing the same key doing different things in different contexts
fn create_ambiguous_mock_settings() -> ah_tui::Settings {
    use ah_tui::settings::KeyMatcher;

    // Create KeyMatchers - note that both operations use the same key (Ctrl+N)
    let ctrl_n_matcher = KeyMatcher::new(
        KeyCode::Char('n'),
        KeyModifiers::CONTROL,
        KeyModifiers::NONE,
        Some('n'),
    );
    let esc_matcher = KeyMatcher::new(KeyCode::Esc, KeyModifiers::NONE, KeyModifiers::NONE, None);

    ah_tui::Settings {
        keymap: Some(KeymapConfig {
            dismiss_overlay: Some(vec![esc_matcher]),
            new_draft: Some(vec![ctrl_n_matcher.clone()]),
            // We'll use the same Ctrl+N for a different operation in different states
            select_all: Some(vec![ctrl_n_matcher]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod basic_functionality {
    use super::*;

    #[test]
    fn empty_stack_delegates_to_default_state() {
        let mut stack = InputStateStack::new();
        let settings = create_mock_settings();

        // Default state should handle no operations
        assert!(!stack.current().handles_operation(&KeyboardOperation::MoveToNextLine));

        // Key event should not be handled
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert!(!stack.handle_key_event(&key_event, &settings));
    }

    #[test]
    fn single_state_push_and_pop() {
        let mut stack = InputStateStack::new();

        let input_state = InputState::new(operations::NAVIGATION, |operation, _| {
            assert_eq!(operation, KeyboardOperation::MoveToNextLine);
            InputResult::Handled // Indicate operation was handled
        });

        // Push state and verify it's current
        stack.push(input_state);
        assert!(stack.current().handles_operation(&KeyboardOperation::MoveToNextLine));
        assert!(!stack.current().handles_operation(&KeyboardOperation::SelectAll));

        // Handle key event
        let settings = create_mock_settings();
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert!(stack.handle_key_event(&key_event, &settings));

        // Pop state and verify default behavior restored
        stack.pop();
        assert!(!stack.current().handles_operation(&KeyboardOperation::MoveToNextLine));
        assert!(!stack.handle_key_event(&key_event, &settings));
    }

    #[test]
    fn default_state_has_no_operations() {
        let default_state = InputState::default();
        assert!(!default_state.handles_operation(&KeyboardOperation::MoveToNextLine));
        assert_eq!(default_state.supported_operations().count(), 0);
        assert_eq!(default_state.prominent_operations().len(), 0);
    }

    #[test]
    fn clear_resets_to_default() {
        let mut stack = InputStateStack::new();

        let input_state = InputState::new(operations::NAVIGATION, |_, _| InputResult::Handled);

        stack.push(input_state);
        assert!(stack.current().handles_operation(&KeyboardOperation::MoveToNextLine));

        stack.clear();
        assert!(!stack.current().handles_operation(&KeyboardOperation::MoveToNextLine));
    }
}

#[cfg(test)]
mod stack_mechanics {
    use super::*;

    #[test]
    fn multiple_states_lifo_ordering() {
        let mut stack = InputStateStack::new();

        let state1 = InputState::new(&[KeyboardOperation::MoveToNextLine], |op, _| {
            assert_eq!(op, KeyboardOperation::MoveToNextLine);
            InputResult::Handled
        });

        let state2 = InputState::new(&[KeyboardOperation::MoveToPreviousLine], |op, _| {
            assert_eq!(op, KeyboardOperation::MoveToPreviousLine);
            InputResult::Handled
        });

        let state3 = InputState::new(&[KeyboardOperation::DismissOverlay], |op, _| {
            assert_eq!(op, KeyboardOperation::DismissOverlay);
            InputResult::Handled
        });

        // Push states in order
        stack.push(state1);
        stack.push(state2);
        stack.push(state3);

        // Current should be state3
        assert!(stack.current().handles_operation(&KeyboardOperation::DismissOverlay));
        assert!(!stack.current().handles_operation(&KeyboardOperation::MoveToNextLine));

        // Pop and verify LIFO
        stack.pop();
        assert!(stack.current().handles_operation(&KeyboardOperation::MoveToPreviousLine));
        assert!(!stack.current().handles_operation(&KeyboardOperation::DismissOverlay));

        stack.pop();
        assert!(stack.current().handles_operation(&KeyboardOperation::MoveToNextLine));
        assert!(!stack.current().handles_operation(&KeyboardOperation::MoveToPreviousLine));

        stack.pop();
        assert!(!stack.current().handles_operation(&KeyboardOperation::MoveToNextLine));
    }

    #[test]
    fn duplicate_states_maintain_independence() {
        let mut stack = InputStateStack::new();

        let state1 = InputState::new(operations::NAVIGATION, |op, _| {
            assert_eq!(op, KeyboardOperation::MoveToNextLine);
            InputResult::Handled
        });

        let state2 = InputState::new(
            operations::NAVIGATION, // Same operations slice
            |op, _| {
                assert_eq!(op, KeyboardOperation::MoveToNextLine);
                InputResult::Handled
            },
        );

        stack.push(state1);
        stack.push(state2);

        let settings = create_mock_settings();
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);

        // Both states handle the same operation
        assert!(stack.handle_key_event(&key_event, &settings));

        stack.pop();
        assert!(stack.handle_key_event(&key_event, &settings));
    }

    #[test]
    fn same_key_different_behaviors_in_different_states() {
        let mut stack = InputStateStack::new();
        let behaviors_executed = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

        // Both states handle the same operation (NewDraft) but with different behaviors
        let behaviors_executed_clone1 = behaviors_executed.clone();
        let state1 = InputState::new(&[KeyboardOperation::NewDraft], move |op, _| {
            assert_eq!(op, KeyboardOperation::NewDraft);
            behaviors_executed_clone1.borrow_mut().push("state1_new_draft");
            InputResult::Handled
        });

        let behaviors_executed_clone2 = behaviors_executed.clone();
        let state2 = InputState::new(
            &[KeyboardOperation::NewDraft], // Same operation as state1
            move |op, _| {
                assert_eq!(op, KeyboardOperation::NewDraft);
                behaviors_executed_clone2.borrow_mut().push("state2_new_draft"); // Different behavior
                InputResult::Handled
            },
        );

        let settings = create_mock_settings();
        let ctrl_n_event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL); // Maps to NewDraft

        // Push state1 - Ctrl+N triggers state1's behavior
        stack.push(state1);
        assert!(stack.handle_key_event(&ctrl_n_event, &settings));
        assert_eq!(*behaviors_executed.borrow(), vec!["state1_new_draft"]);

        // Push state2 on top - same key, same operation, but different behavior
        stack.push(state2);
        behaviors_executed.borrow_mut().clear();
        assert!(stack.handle_key_event(&ctrl_n_event, &settings));
        assert_eq!(*behaviors_executed.borrow(), vec!["state2_new_draft"]);

        // Pop state2 - back to state1's behavior
        stack.pop();
        behaviors_executed.borrow_mut().clear();
        assert!(stack.handle_key_event(&ctrl_n_event, &settings));
        assert_eq!(*behaviors_executed.borrow(), vec!["state1_new_draft"]);
    }

    #[test]
    fn multiple_states_handle_same_operation_top_most_takes_precedence() {
        let mut stack = InputStateStack::new();
        let handlers_called = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

        // Create three states, all handling the same operation but with different handlers
        let handlers_called_clone1 = handlers_called.clone();
        let bottom_state = InputState::new(&[KeyboardOperation::MoveToNextLine], move |_, _| {
            handlers_called_clone1.borrow_mut().push("bottom_handler");
            InputResult::Handled
        });

        let handlers_called_clone2 = handlers_called.clone();
        let middle_state = InputState::new(&[KeyboardOperation::MoveToNextLine], move |_, _| {
            handlers_called_clone2.borrow_mut().push("middle_handler");
            InputResult::Handled
        });

        let handlers_called_clone3 = handlers_called.clone();
        let top_state = InputState::new(&[KeyboardOperation::MoveToNextLine], move |_, _| {
            handlers_called_clone3.borrow_mut().push("top_handler");
            InputResult::Handled
        });

        let settings = create_mock_settings();
        let down_key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE); // Maps to MoveToNextLine

        // Push all three states - top_state should be active
        stack.push(bottom_state);
        stack.push(middle_state);
        stack.push(top_state);

        // All three states handle MoveToNextLine, but only the top-most (top_state) should execute
        assert!(stack.handle_key_event(&down_key, &settings));
        assert_eq!(*handlers_called.borrow(), vec!["top_handler"]);

        // Pop top state - now middle_state should be active
        handlers_called.borrow_mut().clear();
        stack.pop();
        assert!(stack.handle_key_event(&down_key, &settings));
        assert_eq!(*handlers_called.borrow(), vec!["middle_handler"]);

        // Pop middle state - now bottom_state should be active
        handlers_called.borrow_mut().clear();
        stack.pop();
        assert!(stack.handle_key_event(&down_key, &settings));
        assert_eq!(*handlers_called.borrow(), vec!["bottom_handler"]);

        // Pop bottom state - now default state (no handler)
        handlers_called.borrow_mut().clear();
        stack.pop();
        assert!(!stack.handle_key_event(&down_key, &settings)); // No handler called
        assert_eq!(handlers_called.borrow().len(), 0);
    }

    #[test]
    fn operation_falls_through_to_lower_states() {
        let mut stack = InputStateStack::new();
        let operations_handled = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

        // State1 handles NewDraft, State2 handles DismissOverlay
        let operations_handled_clone1 = operations_handled.clone();
        let state1 = InputState::new(&[KeyboardOperation::NewDraft], move |op, _| {
            operations_handled_clone1.borrow_mut().push(("state1", op));
            InputResult::Handled
        });

        let operations_handled_clone2 = operations_handled.clone();
        let state2 = InputState::new(
            &[KeyboardOperation::DismissOverlay], // Different operation
            move |op, _| {
                operations_handled_clone2.borrow_mut().push(("state2", op));
                InputResult::Handled
            },
        );

        let settings = create_mock_settings();
        let ctrl_n_event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL); // Maps to NewDraft
        let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE); // Maps to DismissOverlay

        // Push both states
        stack.push(state1);
        stack.push(state2);

        // Ctrl+N maps to NewDraft, only state1 (bottom) handles it, but search still works
        assert!(stack.handle_key_event(&ctrl_n_event, &settings));
        assert_eq!(
            *operations_handled.borrow(),
            vec![("state1", KeyboardOperation::NewDraft)]
        );

        // Esc maps to DismissOverlay, state2 (top) handles it
        operations_handled.borrow_mut().clear();
        assert!(stack.handle_key_event(&esc_event, &settings));
        assert_eq!(
            *operations_handled.borrow(),
            vec![("state2", KeyboardOperation::DismissOverlay)]
        );
    }

    #[test]
    fn handler_can_decline_operation_fallback_to_lower_states() {
        let mut stack = InputStateStack::new();
        let operations_handled = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

        // Top state handles NewDraft but declines it (returns false)
        let operations_handled_clone1 = operations_handled.clone();
        let top_state = InputState::new(&[KeyboardOperation::NewDraft], move |op, _| {
            operations_handled_clone1.borrow_mut().push(("top_declined", op));
            InputResult::NotHandled // Decline to handle, allow fallback
        });

        // Bottom state also handles NewDraft and accepts it
        let operations_handled_clone2 = operations_handled.clone();
        let bottom_state = InputState::new(&[KeyboardOperation::NewDraft], move |op, _| {
            operations_handled_clone2.borrow_mut().push(("bottom_accepted", op));
            InputResult::Handled // Accept the operation
        });

        let settings = create_mock_settings();
        let ctrl_n_event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL); // Maps to NewDraft

        // Push states: bottom first, then top
        stack.push(bottom_state);
        stack.push(top_state);

        // Ctrl+N should be handled by top state first (which declines),
        // then fall back to bottom state (which accepts)
        assert!(stack.handle_key_event(&ctrl_n_event, &settings));
        assert_eq!(
            *operations_handled.borrow(),
            vec![
                ("top_declined", KeyboardOperation::NewDraft),
                ("bottom_accepted", KeyboardOperation::NewDraft)
            ]
        );
    }

    #[test]
    fn all_handlers_decline_operation_returns_false() {
        let mut stack = InputStateStack::new();
        let operations_handled = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

        // Both states handle NewDraft but both decline it
        let operations_handled_clone1 = operations_handled.clone();
        let state1 = InputState::new(&[KeyboardOperation::NewDraft], move |op, _| {
            operations_handled_clone1.borrow_mut().push(("state1_declined", op));
            InputResult::NotHandled // Decline
        });

        let operations_handled_clone2 = operations_handled.clone();
        let state2 = InputState::new(&[KeyboardOperation::NewDraft], move |op, _| {
            operations_handled_clone2.borrow_mut().push(("state2_declined", op));
            InputResult::NotHandled // Decline
        });

        let settings = create_mock_settings();
        let ctrl_n_event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL); // Maps to NewDraft

        // Push both states
        stack.push(state1);
        stack.push(state2);

        // Both states decline, so overall result should be false
        assert!(!stack.handle_key_event(&ctrl_n_event, &settings));
        assert_eq!(
            *operations_handled.borrow(),
            vec![
                ("state2_declined", KeyboardOperation::NewDraft),
                ("state1_declined", KeyboardOperation::NewDraft)
            ]
        );
    }
}

#[cfg(test)]
mod memory_management_patterns {
    use super::*;

    // Miniature view state types to demonstrate memory management patterns

    /// Simple owned state - can be moved into closures
    #[derive(Debug, Clone)]
    struct SimpleViewState {
        item_count: usize,
        selected_index: usize,
    }

    impl SimpleViewState {
        fn new(item_count: usize) -> Self {
            Self {
                item_count,
                selected_index: 0,
            }
        }

        fn move_selection_down(&mut self) {
            if self.selected_index < self.item_count.saturating_sub(1) {
                self.selected_index += 1;
            }
        }

        fn move_selection_up(&mut self) {
            self.selected_index = self.selected_index.saturating_sub(1);
        }

        fn get_selected_index(&self) -> usize {
            self.selected_index
        }
    }

    /// Complex view model that needs to be shared between multiple input states
    #[derive(Debug)]
    struct TaskListViewModel {
        tasks: Vec<String>,
        selected_task_index: usize,
        filter_text: String,
        is_editing: bool,
    }

    impl TaskListViewModel {
        fn new() -> Self {
            Self {
                tasks: vec![
                    "Task 1".to_string(),
                    "Task 2".to_string(),
                    "Task 3".to_string(),
                ],
                selected_task_index: 0,
                filter_text: String::new(),
                is_editing: false,
            }
        }

        fn navigate_down(&mut self) {
            if self.selected_task_index < self.tasks.len().saturating_sub(1) {
                self.selected_task_index += 1;
            }
        }

        fn navigate_up(&mut self) {
            self.selected_task_index = self.selected_task_index.saturating_sub(1);
        }

        fn start_editing(&mut self) {
            self.is_editing = true;
        }

        fn stop_editing(&mut self) {
            self.is_editing = false;
        }

        fn get_selected_task(&self) -> Option<&str> {
            self.tasks.get(self.selected_task_index).map(|s| s.as_str())
        }

        fn is_editing(&self) -> bool {
            self.is_editing
        }
    }

    #[test]
    fn owned_state_in_closures_works_but_not_practical() {
        // This pattern works technically but is not practical for real applications
        // because you can't share state between multiple input handlers
        let mut stack = InputStateStack::new();

        // Each input state gets its own copy of the view state
        // This defeats the purpose of having coordinated state
        let view_state = SimpleViewState::new(5);

        let navigation_state = InputState::new(operations::NAVIGATION, {
            let mut owned_state = view_state.clone(); // ❌ Cloning defeats single source of truth
            move |op, _| match op {
                KeyboardOperation::MoveToNextLine => {
                    owned_state.move_selection_down();
                    InputResult::Handled
                }
                KeyboardOperation::MoveToPreviousLine => {
                    owned_state.move_selection_up();
                    InputResult::Handled
                }
                _ => InputResult::NotHandled,
            }
        });

        stack.push(navigation_state);
        let settings = create_mock_settings();

        // Navigation works, but state is isolated
        assert!(
            stack.handle_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &settings)
        );

        // ❌ Can't access the original view_state - it's been moved/cloned
        // ❌ Multiple input handlers can't coordinate on the same state
        // ❌ This pattern doesn't work for real TUI applications

        // Conclusion: Rc<RefCell<>> is required for realistic state management
    }

    #[test]
    fn borrowed_state_lifetime_issues() {
        // Pattern 2: Attempting to borrow state (this demonstrates why it doesn't work)
        let mut stack = InputStateStack::new();

        let view_state = SimpleViewState::new(5);

        // This would not compile because the closure borrows view_state
        // but InputState requires 'static lifetime
        /*
        let navigation_state = InputState::new(
            operations::NAVIGATION,
            |op, _| {  // error: closure may outlive the current function
                match op {
                    KeyboardOperation::MoveToNextLine => {
                        view_state.move_selection_down(); // borrows view_state
                        InputResult::Handled
                    }
                    _ => InputResult::NotHandled,
                }
            }
        );
        */

        // Instead, we need to use Rc<RefCell<>> for shared mutable access
    }

    #[test]
    fn shared_mutable_state_with_rc_refcell() {
        // Pattern 3: Shared mutable state with Rc<RefCell<>>
        // This is the most common and practical pattern for real applications
        let mut stack = InputStateStack::new();

        // Create shared view model - this is how you'd typically structure a TUI component
        let view_model = std::rc::Rc::new(std::cell::RefCell::new(SimpleViewState::new(5)));
        assert_eq!(std::rc::Rc::strong_count(&view_model), 1);

        // Create two input states that share the same view model
        let view_model_clone1 = view_model.clone();
        let navigation_state = InputState::new(operations::NAVIGATION, move |op, _| {
            let mut vm = view_model_clone1.borrow_mut();
            match op {
                KeyboardOperation::MoveToNextLine => {
                    vm.move_selection_down();
                    InputResult::Handled
                }
                KeyboardOperation::MoveToPreviousLine => {
                    vm.move_selection_up();
                    InputResult::Handled
                }
                _ => InputResult::NotHandled,
            }
        });
        assert_eq!(std::rc::Rc::strong_count(&view_model), 2);

        let view_model_clone2 = view_model.clone();
        let action_state = InputState::new(&[KeyboardOperation::SelectAll], move |op, _| {
            let mut vm = view_model_clone2.borrow_mut();
            match op {
                KeyboardOperation::SelectAll => {
                    vm.selected_index = vm.item_count.saturating_sub(1); // Select last
                    InputResult::Handled
                }
                _ => InputResult::NotHandled,
            }
        });
        assert_eq!(std::rc::Rc::strong_count(&view_model), 3);

        // Push both states
        stack.push(navigation_state);
        stack.push(action_state);

        let settings = create_mock_settings();

        // Both states can mutate the same underlying view model
        assert!(
            stack.handle_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &settings)
        );
        assert_eq!(view_model.borrow().get_selected_index(), 1);

        // Rc<RefCell<>> allows safe interior mutability for single-threaded TUI applications
        // This is the standard pattern for sharing state between input handlers
    }

    #[test]
    fn complex_view_model_with_multiple_input_states() {
        // Pattern 4: Complex view model shared between multiple input states
        // This simulates a realistic TUI component with different interaction modes
        let mut stack = InputStateStack::new();

        let view_model = std::rc::Rc::new(std::cell::RefCell::new(TaskListViewModel::new()));

        // Normal browsing mode
        let vm_clone1 = view_model.clone();
        let browse_state = InputState::new(operations::NAVIGATION, move |op, _| {
            let mut vm = vm_clone1.borrow_mut();
            match op {
                KeyboardOperation::MoveToNextLine => {
                    vm.navigate_down();
                    InputResult::Handled
                }
                KeyboardOperation::MoveToPreviousLine => {
                    vm.navigate_up();
                    InputResult::Handled
                }
                KeyboardOperation::DismissOverlay => {
                    vm.stop_editing();
                    InputResult::Handled
                }
                _ => InputResult::NotHandled,
            }
        });

        // Editing mode (overlay on top of browsing)
        let vm_clone2 = view_model.clone();
        let edit_state = InputState::new(
            &[
                KeyboardOperation::NewDraft,
                KeyboardOperation::DismissOverlay,
            ],
            move |op, _| {
                let mut vm = vm_clone2.borrow_mut();
                match op {
                    KeyboardOperation::NewDraft => {
                        vm.start_editing();
                        InputResult::Handled
                    }
                    KeyboardOperation::DismissOverlay => {
                        vm.stop_editing();
                        InputResult::Handled
                    }
                    _ => InputResult::NotHandled, // Let navigation fall through to browse_state
                }
            },
        );

        let settings = create_mock_settings();

        // Start in browse mode
        stack.push(browse_state);
        assert!(
            stack.handle_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &settings)
        );
        assert_eq!(view_model.borrow().get_selected_task(), Some("Task 2"));

        // Enter edit mode (push overlay)
        stack.push(edit_state);
        assert!(stack.handle_key_event(
            &KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
            &settings
        ));
        assert!(view_model.borrow().is_editing());

        // Navigation should fall through to browse state when edit state doesn't handle it
        assert!(
            stack.handle_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &settings)
        );
        assert_eq!(view_model.borrow().get_selected_task(), Some("Task 3"));

        // ESC should be handled by edit state (dismiss overlay)
        assert!(
            stack.handle_key_event(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &settings)
        );
        assert!(!view_model.borrow().is_editing());
    }

    #[test]
    fn demonstrating_rc_arc_difference() {
        // Pattern 5: When Rc<RefCell<>> is sufficient vs when you need Arc<Mutex<>>
        let view_model = std::rc::Rc::new(std::cell::RefCell::new(SimpleViewState::new(3)));

        // Rc<RefCell<>> works fine for single-threaded scenarios
        // This is what you'd use in a typical TUI application
        let vm_clone = view_model.clone();
        std::thread::spawn(move || {
            // This would fail at runtime if we tried to use Rc across threads
            // let _ = vm_clone.borrow_mut(); // panic: Rc<RefCell<T>> not Send
        });

        // For multi-threaded scenarios (rare in TUI), you'd use Arc<Mutex<>>
        let thread_safe_model = std::sync::Arc::new(std::sync::Mutex::new(SimpleViewState::new(3)));

        let model_clone = thread_safe_model.clone();
        let handle = std::thread::spawn(move || {
            let mut model = model_clone.lock().unwrap();
            model.move_selection_down();
            model.get_selected_index()
        });

        let result = handle.join().unwrap();
        assert_eq!(result, 1);

        // But for TUI applications, Rc<RefCell<>> is simpler and more appropriate
    }

    #[test]
    fn memory_cleanup_when_states_are_dropped() {
        // Pattern 6: Demonstrating that Rc<RefCell<>> handles cleanup properly
        let view_model = std::rc::Rc::new(std::cell::RefCell::new(SimpleViewState::new(3)));
        assert_eq!(std::rc::Rc::strong_count(&view_model), 1);

        {
            let mut stack = InputStateStack::new();

            // Create input states that hold references to the view model
            let vm_clone1 = view_model.clone();
            let state1 = InputState::new(&[KeyboardOperation::MoveToNextLine], move |op, _| {
                vm_clone1.borrow_mut().move_selection_down();
                InputResult::Handled
            });
            assert_eq!(std::rc::Rc::strong_count(&view_model), 2);

            let vm_clone2 = view_model.clone();
            let state2 = InputState::new(&[KeyboardOperation::MoveToPreviousLine], move |op, _| {
                vm_clone2.borrow_mut().move_selection_up();
                InputResult::Handled
            });
            assert_eq!(std::rc::Rc::strong_count(&view_model), 3);

            stack.push(state1);
            stack.push(state2);

            // States are still holding references
            assert_eq!(std::rc::Rc::strong_count(&view_model), 3);

            // Drop the stack
        }

        // References should be cleaned up
        assert_eq!(std::rc::Rc::strong_count(&view_model), 1);
    }

    #[test]
    fn efficiency_improvement_state_specific_vs_global_mapping() {
        // This test demonstrates why the new InputState::handle_key_event is more efficient
        // than the old global key_event_to_operation approach

        let mut stack = InputStateStack::new();
        let settings = create_mock_settings();

        // Create a state that only handles navigation
        let navigation_state = InputState::new(
            operations::NAVIGATION, // Only 4 operations
            |op, _| {
                if matches!(
                    op,
                    KeyboardOperation::MoveToNextLine
                        | KeyboardOperation::MoveToPreviousLine
                        | KeyboardOperation::MoveToNextField
                        | KeyboardOperation::MoveToPreviousField
                ) {
                    InputResult::Handled
                } else {
                    InputResult::NotHandled
                }
            },
        );

        stack.push(navigation_state);

        let down_key = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);

        // Test that navigation works
        assert!(stack.handle_key_event(&down_key, &settings));

        // The old key_event_to_operation would check ~50 operations globally
        // The new InputState::handle_key_event only checks the 4 operations this state handles

        // Verify that a key not handled by this state returns false
        let unmapped_key = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE);
        assert!(!stack.handle_key_event(&unmapped_key, &settings));
    }

    #[test]
    fn bubbling_operations_to_lower_states() {
        // Test that operations can be bubbled to lower states in the stack
        let mut stack = InputStateStack::new();
        let settings = create_mock_settings();

        // Top state (modal) - handles ESC and bubbles NewDraft to lower states
        let top_operations_handled = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let top_clone = top_operations_handled.clone();
        let modal_state = InputState::new(
            &[
                KeyboardOperation::DismissOverlay,
                KeyboardOperation::NewDraft,
            ],
            move |op, _| {
                top_clone.borrow_mut().push(("modal", op));
                match op {
                    KeyboardOperation::DismissOverlay => InputResult::Handled,
                    KeyboardOperation::NewDraft => {
                        // Bubble NewDraft to lower states instead of handling it
                        InputResult::Bubble(KeyboardOperation::SelectAll)
                    }
                    _ => InputResult::NotHandled,
                }
            },
        );

        // Bottom state (main) - handles SelectAll (which was bubbled from NewDraft)
        let bottom_operations_handled = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let bottom_clone = bottom_operations_handled.clone();
        let main_state = InputState::new(&[KeyboardOperation::SelectAll], move |op, _| {
            bottom_clone.borrow_mut().push(("main", op));
            match op {
                KeyboardOperation::SelectAll => InputResult::Handled,
                _ => InputResult::NotHandled,
            }
        });

        // Push states (main first, then modal on top)
        stack.push(main_state);
        stack.push(modal_state);

        // Test ESC - handled by modal
        let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert!(stack.handle_key_event(&esc_key, &settings));
        assert_eq!(
            *top_operations_handled.borrow(),
            vec![("modal", KeyboardOperation::DismissOverlay)]
        );
        assert_eq!(
            *bottom_operations_handled.borrow(),
            Vec::<(&str, KeyboardOperation)>::new()
        );

        // Reset
        top_operations_handled.borrow_mut().clear();

        // Test Ctrl+N (NewDraft) - should bubble to SelectAll and be handled by main state
        let ctrl_n_key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL);
        assert!(stack.handle_key_event(&ctrl_n_key, &settings));
        assert_eq!(
            *top_operations_handled.borrow(),
            vec![("modal", KeyboardOperation::NewDraft)]
        );
        assert_eq!(
            *bottom_operations_handled.borrow(),
            vec![("main", KeyboardOperation::SelectAll)]
        );
    }

    #[test]
    fn cloned_stack_uses_default_states() {
        let mut stack = InputStateStack::new();

        let input_state = InputState::new(operations::NAVIGATION, |_, _| InputResult::Handled);

        stack.push(input_state);
        assert!(stack.current().handles_operation(&KeyboardOperation::MoveToNextLine));

        let cloned_stack = stack.clone();
        // Cloned stack should have empty states (no handlers)
        assert!(!cloned_stack.current().handles_operation(&KeyboardOperation::MoveToNextLine));

        // Original stack still works
        let settings = create_mock_settings();
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert!(stack.handle_key_event(&key_event, &settings));
    }

    #[test]
    fn cloned_input_state_loses_handler() {
        let original = InputState::new(operations::NAVIGATION, |_, _| InputResult::Handled);

        assert!(original.handles_operation(&KeyboardOperation::MoveToNextLine));

        let mut cloned = original.clone();
        assert!(cloned.handles_operation(&KeyboardOperation::MoveToNextLine)); // Operations preserved
        assert_eq!(
            cloned.supported_operations().count(),
            original.supported_operations().count()
        );

        // But handler is gone (no operation should execute)
        let key_event = dummy_key_event();
        assert_eq!(
            cloned.execute_operation(KeyboardOperation::MoveToNextLine, &key_event),
            InputResult::NotHandled
        );
    }
}

#[cfg(test)]
mod handler_behavior {
    use super::*;

    #[test]
    fn handler_receives_correct_operation() {
        let mut stack = InputStateStack::new();

        let input_state = InputState::with_prominent_operations(
            operations::STANDARD_NAVIGATION,
            operations::prominent::ACTIONS,
            |operation, _| {
                // Just verify we get a valid operation
                match operation {
                    KeyboardOperation::MoveToNextLine
                    | KeyboardOperation::MoveToPreviousLine
                    | KeyboardOperation::DismissOverlay
                    | KeyboardOperation::SelectAll
                    | KeyboardOperation::NewDraft => {} // Valid
                    _ => panic!("Unexpected operation: {:?}", operation),
                }
                InputResult::Handled
            },
        );

        stack.push(input_state);
        let settings = create_mock_settings();

        // Test different operations
        let test_cases = vec![
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        ];

        for key_event in test_cases {
            assert!(stack.handle_key_event(&key_event, &settings));
        }
    }

    #[test]
    fn handler_executes_without_panicking() {
        let mut stack = InputStateStack::new();

        let input_state = InputState::new(operations::NAVIGATION, |operation, _| {
            // Handler that does work but doesn't capture external state
            match operation {
                KeyboardOperation::MoveToNextLine => {}
                _ => {}
            }
            InputResult::Handled
        });

        stack.push(input_state);
        let settings = create_mock_settings();

        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert!(stack.handle_key_event(&key_event, &settings));
    }

    #[test]
    fn state_without_handler_returns_false() {
        let mut cloned_state =
            InputState::new(operations::NAVIGATION, |_, _| InputResult::Handled).clone(); // Clone removes handler

        assert!(cloned_state.handles_operation(&KeyboardOperation::MoveToNextLine));
        let key_event = dummy_key_event();
        assert_eq!(
            cloned_state.execute_operation(KeyboardOperation::MoveToNextLine, &key_event),
            InputResult::NotHandled
        );
    }
}

#[cfg(test)]
mod settings_integration {
    use super::*;

    #[test]
    fn key_event_mapping_works() {
        let mut stack = InputStateStack::new();

        let input_state = InputState::new(operations::STANDARD_NAVIGATION, |operation, _| {
            match operation {
                KeyboardOperation::MoveToNextLine
                | KeyboardOperation::MoveToPreviousLine
                | KeyboardOperation::DismissOverlay
                | KeyboardOperation::SelectAll
                | KeyboardOperation::NewDraft => {} // Valid operations
                _ => panic!("Unexpected operation: {:?}", operation),
            }
            InputResult::Handled
        });

        stack.push(input_state);
        let settings = create_mock_settings();

        // Test various key mappings
        let test_cases = vec![
            (KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), true),
            (KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), true),
            (KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE), false), // Unmapped key
        ];

        for (key_event, should_handle) in test_cases {
            let result = stack.handle_key_event(&key_event, &settings);
            assert_eq!(
                result, should_handle,
                "Failed for key event: {:?}",
                key_event
            );
        }
    }

    #[test]
    fn unmapped_key_returns_false() {
        let mut stack = InputStateStack::new();
        let input_state = InputState::new(operations::NAVIGATION, |_, _| {
            panic!("Should not be called")
        });

        stack.push(input_state);
        let settings = create_mock_settings();

        // Test keys that don't map to any operation
        let unmapped_keys = vec![
            KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT), // Different modifiers
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT), // Different modifiers
        ];

        for key_event in unmapped_keys {
            assert!(
                !stack.handle_key_event(&key_event, &settings),
                "Key event should not be handled: {:?}",
                key_event
            );
        }
    }

    #[test]
    fn operation_not_in_state_returns_false() {
        let mut stack = InputStateStack::new();
        let input_state = InputState::new(
            &[KeyboardOperation::MoveToNextLine], // Only handles one operation
            |_, _| panic!("Should not be called"),
        );

        stack.push(input_state);
        let settings = create_mock_settings();

        // Try a key that maps to an operation not handled by this state
        let key_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE); // Maps to DismissOverlay
        assert!(!stack.handle_key_event(&key_event, &settings));
    }
}

#[cfg(test)]
mod robustness_and_error_conditions {
    use super::*;

    #[test]
    fn multiple_push_pop_cycles() {
        let mut stack = InputStateStack::new();
        let settings = create_mock_settings();
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);

        // Multiple push/pop cycles - verify push/pop don't panic and key events
        // are handled when state is active
        for _ in 0..3 {
            // Create a fresh state each time (clone would remove the handler)
            let input_state = InputState::new(operations::NAVIGATION, |_, _| InputResult::Handled);

            stack.push(input_state);
            let result = stack.handle_key_event(&key_event, &settings);
            assert!(result, "Key event should be handled when state is pushed");
            stack.pop();
            // After pop, default state doesn't handle navigation - this is expected
        }
    }

    #[test]
    fn pop_on_empty_stack_is_safe() {
        let mut stack = InputStateStack::new();
        assert_eq!(stack.states().len(), 0);

        // Pop on empty stack should not panic
        stack.pop();
        stack.pop();
        stack.pop();

        assert_eq!(stack.states().len(), 0);
    }

    #[test]
    fn empty_operation_slices_work() {
        static EMPTY_OPERATIONS: &[KeyboardOperation] = &[];

        let input_state = InputState::with_prominent_operations(
            EMPTY_OPERATIONS,
            &[KeyboardOperation::DismissOverlay], // Prominent operations can be non-empty
            |_, _| InputResult::Handled,
        );

        assert!(!input_state.handles_operation(&KeyboardOperation::DismissOverlay));
        assert_eq!(input_state.supported_operations().count(), 0);
        assert_eq!(input_state.prominent_operations().len(), 1);
    }

    #[test]
    fn large_operation_set_linear_search() {
        // Test with all operations to ensure linear search works
        let all_operations = &[
            KeyboardOperation::MoveToNextLine,
            KeyboardOperation::MoveToPreviousLine,
            KeyboardOperation::MoveToNextField,
            KeyboardOperation::MoveToPreviousField,
            KeyboardOperation::DismissOverlay,
            KeyboardOperation::SelectAll,
            KeyboardOperation::NewDraft,
            KeyboardOperation::MoveToBeginningOfLine,
            KeyboardOperation::MoveToEndOfLine,
        ];

        let input_state = InputState::new(all_operations, |_, _| InputResult::Handled);

        // All operations should be handled
        for operation in all_operations {
            assert!(input_state.handles_operation(operation));
        }

        // Non-included operation should not be handled
        assert!(!input_state.handles_operation(&KeyboardOperation::IncrementalSearchForward));
    }
}

#[cfg(test)]
mod regression_guards {
    use super::*;

    #[test]
    fn prominent_operations_subset_of_supported() {
        let input_state = InputState::with_prominent_operations(
            operations::STANDARD_NAVIGATION,
            operations::prominent::ACTIONS,
            |_, _| InputResult::Handled,
        );

        // All prominent operations should be in the supported operations
        for prominent_op in input_state.prominent_operations() {
            assert!(
                input_state.handles_operation(prominent_op),
                "Prominent operation {:?} not in supported operations",
                prominent_op
            );
        }
    }

    #[test]
    fn supported_operations_iteration_matches_slice() {
        let input_state =
            InputState::new(operations::STANDARD_NAVIGATION, |_, _| InputResult::Handled);

        let collected: Vec<_> = input_state.supported_operations().collect();
        assert_eq!(collected.len(), operations::STANDARD_NAVIGATION.len());

        for (i, operation) in collected.into_iter().enumerate() {
            assert_eq!(operation, &operations::STANDARD_NAVIGATION[i]);
        }
    }

    #[test]
    fn debug_output_shows_handler_presence() {
        let with_handler = InputState::new(operations::NAVIGATION, |_, _| InputResult::Handled);
        let without_handler = with_handler.clone();

        let debug_str = format!("{:?}", with_handler);
        assert!(debug_str.contains("has_handler"));
        assert!(debug_str.contains("true"));

        let debug_str_no_handler = format!("{:?}", without_handler);
        assert!(debug_str_no_handler.contains("has_handler"));
        assert!(debug_str_no_handler.contains("false"));
    }

    #[test]
    fn operation_constants_are_static_slices() {
        // Verify that operation constants are indeed static slices
        let nav_ops = operations::NAVIGATION;
        let actions = operations::prominent::ACTIONS;

        assert_eq!(nav_ops.len(), 4); // Up, Down, Tab, Shift+Tab
        assert_eq!(actions.len(), 3); // Esc, SelectAll, NewDraft

        // Verify specific operations are present
        assert!(nav_ops.contains(&KeyboardOperation::MoveToNextLine));
        assert!(nav_ops.contains(&KeyboardOperation::MoveToPreviousLine));
        assert!(actions.contains(&KeyboardOperation::DismissOverlay));
        assert!(actions.contains(&KeyboardOperation::SelectAll));
    }

    #[test]
    fn stack_states_accessor() {
        let mut stack = InputStateStack::new();
        assert_eq!(stack.states().len(), 0);

        let state1 = InputState::new(operations::NAVIGATION, |_, _| InputResult::Handled);
        let state2 = InputState::new(operations::SELECTION, |_, _| InputResult::Handled);

        stack.push(state1);
        assert_eq!(stack.states().len(), 1);

        stack.push(state2);
        assert_eq!(stack.states().len(), 2);

        stack.pop();
        assert_eq!(stack.states().len(), 1);

        stack.clear();
        assert_eq!(stack.states().len(), 0);
    }
}

#[cfg(test)]
mod prominent_operations_functionality {
    use super::*;

    #[test]
    fn prominent_operations_stored_correctly() {
        let prominent_ops = &[
            KeyboardOperation::DismissOverlay,
            KeyboardOperation::SelectAll,
        ];
        let input_state = InputState::with_prominent_operations(
            operations::STANDARD_NAVIGATION,
            prominent_ops,
            |_, _| InputResult::Handled,
        );

        assert_eq!(input_state.prominent_operations().len(), 2);
        assert_eq!(
            input_state.prominent_operations()[0],
            KeyboardOperation::DismissOverlay
        );
        assert_eq!(
            input_state.prominent_operations()[1],
            KeyboardOperation::SelectAll
        );
    }

    #[test]
    fn empty_prominent_operations_allowed() {
        let input_state = InputState::with_prominent_operations(
            operations::NAVIGATION,
            &[], // Empty prominent operations
            |_, _| InputResult::Handled,
        );

        assert_eq!(input_state.prominent_operations().len(), 0);
    }

    #[test]
    fn prominent_operations_can_be_larger_than_supported() {
        // This is allowed - prominent operations are just for display
        static SMALL_OPERATIONS: &[KeyboardOperation] = &[KeyboardOperation::MoveToNextLine];
        static LARGE_PROMINENT: &[KeyboardOperation] = &[
            KeyboardOperation::MoveToNextLine,
            KeyboardOperation::DismissOverlay,
            KeyboardOperation::SelectAll,
        ];

        let input_state =
            InputState::with_prominent_operations(SMALL_OPERATIONS, LARGE_PROMINENT, |_, _| {
                InputResult::Handled
            });

        assert_eq!(input_state.supported_operations().count(), 1);
        assert_eq!(input_state.prominent_operations().len(), 3);

        // Only the supported operation should actually work
        assert!(input_state.handles_operation(&KeyboardOperation::MoveToNextLine));
        assert!(!input_state.handles_operation(&KeyboardOperation::DismissOverlay));
    }
}
