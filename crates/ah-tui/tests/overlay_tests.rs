// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Modal navigation and ESC dismissal behaviour tests for the ViewModel

use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ah_core::TaskManager;
use ah_core::WorkspaceFilesEnumerator;
use ah_repo::VcsRepo;
use ah_rest_mock_client::MockRestClient;
use ah_tui::settings::{KeyboardOperation, Settings};
use ah_tui::view_model::autocomplete::{InlineAutocomplete, Item, Provider, Trigger};
use ah_tui::view_model::{FocusElement, ModalState, ViewModel};
use ah_workflows::{WorkflowConfig, WorkflowProcessor, WorkspaceWorkflowsEnumerator};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone)]
struct TestProvider {
    trigger: Trigger,
    items: Arc<Vec<Item>>,
}

impl TestProvider {
    fn new(trigger: Trigger, labels: &[&str]) -> Self {
        let items = labels
            .iter()
            .enumerate()
            .map(|(idx, label)| Item {
                id: format!("item-{}", idx),
                trigger,
                label: label.to_string(),
                detail: None,
                replacement: format!("{}{}", trigger.as_char(), label),
            })
            .collect();

        Self {
            trigger,
            items: Arc::new(items),
        }
    }
}

impl Provider for TestProvider {
    fn trigger(&self) -> Trigger {
        self.trigger
    }

    fn items(&self) -> Arc<Vec<Item>> {
        Arc::clone(&self.items)
    }
}

fn create_test_log(test_name: &str) -> (std::fs::File, std::path::PathBuf) {
    let mut dir = std::env::temp_dir();
    dir.push("ah_tui_vm_logs");
    std::fs::create_dir_all(&dir).expect("create log directory");

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("valid time");
    let file_name = format!(
        "{}_{}_{}.log",
        test_name,
        std::process::id(),
        timestamp.as_nanos()
    );
    dir.push(file_name);
    let file = std::fs::File::create(&dir).expect("create log file");
    (file, dir)
}

fn build_view_model() -> ViewModel {
    let workspace_files: Arc<dyn WorkspaceFilesEnumerator> =
        Arc::new(VcsRepo::new(std::path::Path::new(".").to_path_buf()).unwrap());
    let workspace_workflows = Arc::new(WorkflowProcessor::new(WorkflowConfig::default()));
    let task_manager = Arc::new(MockRestClient::new());
    let settings = Settings::default();

    ViewModel::new(workspace_files, workspace_workflows, task_manager, settings)
}

fn prepare_autocomplete(vm: &mut ViewModel, trigger: Trigger, labels: &[&str]) {
    vm.autocomplete =
        InlineAutocomplete::with_providers(vec![
            Arc::new(TestProvider::new(trigger, labels)) as Arc<dyn Provider>
        ]);

    let card = vm.draft_cards.get_mut(0).expect("draft card");
    card.description = tui_textarea::TextArea::default();
    card.description.input(KeyEvent::new(
        KeyCode::Char(trigger.as_char()),
        KeyModifiers::empty(),
    ));
    card.description.input(KeyEvent::new(
        KeyCode::Char(labels[0].chars().next().unwrap()),
        KeyModifiers::empty(),
    ));

    vm.autocomplete.notify_text_input();
    vm.autocomplete.after_textarea_change(&card.description, &mut vm.needs_redraw);

    std::thread::sleep(Duration::from_millis(90));
    vm.autocomplete.on_tick();
    std::thread::sleep(Duration::from_millis(10));
    vm.autocomplete.poll_results();
}

#[test]
fn modal_navigation_wraps_with_keyboard_operations() {
    let (mut log, log_path) = create_test_log("modal_navigation");
    let log_hint = log_path.display().to_string();

    let mut vm = build_view_model();
    vm.open_modal(ModalState::RepositorySearch);

    let next_key = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());

    let total_options = vm.active_modal.as_ref().expect("modal available").filtered_options.len();

    for step in 0..(total_options.saturating_sub(1)) {
        assert!(
            vm.handle_keyboard_operation(KeyboardOperation::MoveToNextLine, &next_key),
            "MoveToNextLine should be handled inside modal (log: {log_hint})"
        );
        let modal = vm.active_modal.as_ref().expect("modal still open");
        writeln!(log, "Step {} -> selected {}", step, modal.selected_index).expect("write log");
    }

    let modal = vm.active_modal.as_ref().expect("modal still open");
    assert_eq!(
        modal.selected_index,
        total_options.saturating_sub(1),
        "selection should reach last option before wrapping (log: {log_hint})"
    );

    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToNextLine, &next_key),
        "additional MoveToNextLine should wrap (log: {log_hint})"
    );
    let modal = vm.active_modal.as_ref().expect("modal still open");
    assert_eq!(
        modal.selected_index, 0,
        "selection should wrap to start (log: {log_hint})"
    );

    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToPreviousField, &next_key),
        "MoveToPreviousField should wrap backwards (log: {log_hint})"
    );
    let modal = vm.active_modal.as_ref().expect("modal still open");
    let len = modal.filtered_options.len();
    assert_eq!(
        modal.selected_index,
        len - 1,
        "previous field wraps to last option (log: {log_hint})"
    );
}

#[test]
fn dismiss_overlay_behaviour_follows_priority_and_exit_rules() {
    let (mut log, log_path) = create_test_log("dismiss_overlay");
    let log_hint = log_path.display().to_string();

    let mut vm = build_view_model();
    vm.focus_element = FocusElement::TaskDescription;
    vm.open_modal(ModalState::Settings);

    let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::DismissOverlay, &esc_key),
        "ESC should dismiss settings modal (log: {log_hint})"
    );
    writeln!(
        log,
        "Modal after dismiss: {:?}, exit armed: {}",
        vm.modal_state, vm.exit_confirmation_armed
    )
    .expect("write log");

    assert_eq!(
        vm.modal_state,
        ModalState::None,
        "modal should close (log: {log_hint})"
    );
    assert!(
        !vm.exit_confirmation_armed,
        "closing modal should not arm exit (log: {log_hint})"
    );

    prepare_autocomplete(&mut vm, Trigger::Slash, &["alpha"]);
    assert!(
        vm.autocomplete.is_open(),
        "menu should be open before ESC (log: {log_hint})"
    );
    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::DismissOverlay, &esc_key),
        "ESC should close autocomplete first (log: {log_hint})"
    );
    assert!(
        !vm.autocomplete.is_open(),
        "autocomplete should close on ESC (log: {log_hint})"
    );
    assert!(
        !vm.exit_confirmation_armed,
        "closing autocomplete should not arm exit (log: {log_hint})"
    );

    // First ESC should arm exit state
    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::DismissOverlay, &esc_key),
        "ESC should arm exit when nothing else is open (log: {log_hint})"
    );
    assert!(
        vm.exit_confirmation_armed,
        "exit should be armed after first ESC (log: {log_hint})"
    );
    assert!(
        !vm.exit_requested,
        "no exit yet after first ESC (log: {log_hint})"
    );

    // Any other key should discharge the confirmation
    assert!(
        vm.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::empty())),
        "Tab should still be handled (log: {log_hint})"
    );
    assert!(
        !vm.exit_confirmation_armed,
        "non-ESC key should clear exit confirmation (log: {log_hint})"
    );

    // Arm again and confirm second ESC requests exit
    vm.handle_keyboard_operation(KeyboardOperation::DismissOverlay, &esc_key);
    assert!(
        vm.exit_confirmation_armed,
        "exit re-armed (log: {log_hint})"
    );
    vm.handle_keyboard_operation(KeyboardOperation::DismissOverlay, &esc_key);
    writeln!(
        log,
        "After second ESC -> armed {} requested {}",
        vm.exit_confirmation_armed, vm.exit_requested
    )
    .expect("write log");
    assert!(
        vm.exit_requested,
        "second ESC should request exit (log: {log_hint})"
    );
    assert!(
        vm.take_exit_request(),
        "take_exit_request returns true (log: {log_hint})"
    );
    assert!(
        !vm.exit_requested,
        "exit flag cleared after take_exit_request (log: {log_hint})"
    );
}
