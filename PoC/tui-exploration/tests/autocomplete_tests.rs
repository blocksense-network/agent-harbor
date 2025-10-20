//! Autocomplete-specific ViewModel tests ensuring keyboard navigation and caret interactions

use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ah_rest_mock_client::MockRestClient;
use ah_tui::view_model::autocomplete::{InlineAutocomplete, Item, Provider, Trigger};
use ah_tui::view_model::FocusElement;
use ah_workflows::{WorkflowConfig, WorkflowProcessor};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_exploration::settings::{KeyboardOperation, Settings};
use tui_exploration::view_model::ViewModel;
use tui_exploration::workspace_files::GitWorkspaceFiles;

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

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("valid time");
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
    let workspace_files = Box::new(GitWorkspaceFiles::new(std::path::PathBuf::from(".")));
    let workspace_workflows = Box::new(WorkflowProcessor::new(WorkflowConfig::default()));
    let task_manager = Box::new(MockRestClient::new());
    let settings = Settings::default();

    ViewModel::new(workspace_files, workspace_workflows, task_manager, settings)
}

fn prepare_autocomplete(vm: &mut ViewModel, trigger: Trigger, labels: &[&str]) {
    vm.autocomplete = InlineAutocomplete::with_providers(vec![
        Arc::new(TestProvider::new(trigger, labels)) as Arc<dyn Provider>,
    ]);

    let card = vm.draft_cards.get_mut(0).expect("draft card");
    card.description = tui_textarea::TextArea::default();
    card.description
        .input(KeyEvent::new(KeyCode::Char(trigger.as_char()), KeyModifiers::empty()));
    card.description
        .input(KeyEvent::new(KeyCode::Char(labels[0].chars().next().unwrap()), KeyModifiers::empty()));

    vm.autocomplete.notify_text_input();
    vm.autocomplete.after_textarea_change(&card.description);

    std::thread::sleep(Duration::from_millis(90));
    vm.autocomplete.on_tick();
    std::thread::sleep(Duration::from_millis(10));
    vm.autocomplete.poll_results();
}

#[test]
fn autocomplete_navigation_wraps_for_keyboard_operations() {
    let (mut log, log_path) = create_test_log("autocomplete_wrap");
    let log_hint = log_path.display().to_string();

    let mut vm = build_view_model();
    vm.focus_element = FocusElement::TaskDescription;
    prepare_autocomplete(&mut vm, Trigger::Slash, &["alpha", "beta", "gamma"]);

    writeln!(log, "Initial menu: {:?}", vm.autocomplete.menu_state())
        .expect("write log");

    let state = vm
        .autocomplete
        .menu_state()
        .expect("menu should be open after preparation");
    assert_eq!(state.selected_index, 0, "initial selection should be first item (log: {log_hint})");

    let dummy_key = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToNextLine, &dummy_key),
        "MoveToNextLine should be handled by autocomplete (log: {log_hint})"
    );
    let state = vm
        .autocomplete
        .menu_state()
        .expect("menu should remain open after moving next");
    writeln!(log, "After MoveToNextLine -> selected {}", state.selected_index)
        .expect("write log");
    assert_eq!(state.selected_index, 1, "selection should advance (log: {log_hint})");

    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToNextField, &dummy_key),
        "MoveToNextField should also advance selection (log: {log_hint})"
    );
    let state = vm
        .autocomplete
        .menu_state()
        .expect("menu remains open after MoveToNextField");
    writeln!(log, "After MoveToNextField -> selected {}", state.selected_index)
        .expect("write log");
    assert_eq!(state.selected_index, 2, "selection should move to third item (log: {log_hint})");

    // Next move should wrap to first entry
    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToNextLine, &dummy_key),
        "MoveToNextLine should wrap selection (log: {log_hint})"
    );
    let state = vm
        .autocomplete
        .menu_state()
        .expect("menu remains open after wrapping");
    writeln!(log, "After wrap -> selected {}", state.selected_index).expect("write log");
    assert_eq!(state.selected_index, 0, "selection should wrap to first item (log: {log_hint})");

    // Previous movement should also wrap backwards
    assert!(
        vm.handle_keyboard_operation(KeyboardOperation::MoveToPreviousField, &dummy_key),
        "MoveToPreviousField should move selection backwards (log: {log_hint})"
    );
    let state = vm
        .autocomplete
        .menu_state()
        .expect("menu remains open after previous movement");
    writeln!(log, "After MoveToPreviousField -> selected {}", state.selected_index)
        .expect("write log");
    assert_eq!(state.selected_index, 2, "selection should wrap to last item (log: {log_hint})");
}

#[test]
fn caret_movement_closes_autocomplete_menu() {
    let (mut log, log_path) = create_test_log("autocomplete_caret");
    let log_hint = log_path.display().to_string();

    let mut vm = build_view_model();
    vm.focus_element = FocusElement::TaskDescription;
    prepare_autocomplete(&mut vm, Trigger::Slash, &["alpha", "beta"]);

    assert!(vm.autocomplete.is_open(), "menu should be open before moving caret (log: {log_hint})");

    let card = vm.draft_cards.get_mut(0).expect("draft card");
    use tui_textarea::CursorMove;
    card.description.move_cursor(CursorMove::Head);
    vm.autocomplete.after_textarea_change(&card.description);

    writeln!(log, "After moving caret to head, menu open: {}", vm.autocomplete.is_open())
        .expect("write log");

    assert!(
        !vm.autocomplete.is_open(),
        "moving caret off token should close menu (log: {log_hint})"
    );
}
