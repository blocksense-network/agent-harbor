//! Test runtime for deterministic scenario execution
//!
//! This module provides the core testing infrastructure that can execute
//! scenarios step-by-step in a deterministic manner using fake time.

use ah_client_api::ClientApi;
use ah_test_scenarios::{Scenario, Step};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
use std::sync::Arc;
use tokio::time;

use crate::golden::GoldenManager;

use crate::{
    create_test_terminal,
    model::Model,
    msg::{Msg, NetMsg, RestSuccessMsg},
    ui,
    viewmodel::ViewModel,
};

/// Test runtime that executes scenarios deterministically
pub struct TestRuntime<C: ClientApi> {
    terminal: Terminal<TestBackend>,
    model: Model<C>,
    current_view_model: ViewModel,
    golden_manager: GoldenManager,
}

impl<C: ClientApi> TestRuntime<C> {
    /// Create a new test runtime
    pub fn new(client: Arc<C>, width: u16, height: u16, update_goldens: bool) -> Self {
        let terminal = create_test_terminal(width, height);
        let model = Model::new(client);

        // Get initial view model
        let current_view_model = ViewModel::from_state(&model.state);

        Self {
            terminal,
            model,
            current_view_model,
            golden_manager: GoldenManager::new(update_goldens),
        }
    }

    /// Execute a single step from the scenario
    ///
    /// This is the core deterministic execution method that handles
    /// exactly one message and renders once.
    pub async fn execute_step(&mut self, step: &Step, scenario_name: &str) -> Result<(), String> {
        match step {
            Step::AdvanceMs { ms } => {
                // Advance fake time
                time::advance(time::Duration::from_millis(*ms as u64)).await;
                self.model.update(Msg::Tick).await;
            }
            Step::Key { key } => {
                // Convert string key to crossterm KeyEvent
                let key_event = self.parse_key(key)?;
                self.model.update(Msg::Key(key_event)).await;
            }
            Step::Sse { event } => {
                // Inject SSE event
                self.model.update(Msg::Net(NetMsg::Sse(event.clone()))).await;
            }
            Step::AssertVm { focus, selected } => {
                // Run ViewModel assertion
                self.assert_view_model(focus, *selected)?;
            }
            Step::Snapshot { name } => {
                // Take golden and compare with golden file
                let buffer_content = self.buffer_content();
                self.golden_manager.compare_golden(&scenario_name, name, &buffer_content)?;
                self.render()?; // Still render for consistency
            }
        }

        // Update view model after state change
        self.current_view_model = ViewModel::from_state(&self.model.state);

        // Always render after each step for consistency
        self.render()?;

        Ok(())
    }

    /// Load initial data (projects, repos, agents)
    pub async fn load_initial_data(&mut self) -> Result<(), String> {
        self.model.load_initial_data().await?;
        // Add the new task at the end like AppState::default() does
        self.model.state.tasks.push(crate::task::Task::new());
        self.current_view_model = ViewModel::from_state(&self.model.state);
        self.render()?;
        Ok(())
    }

    /// Get the current ViewModel for assertions
    pub fn view_model(&self) -> &ViewModel {
        &self.current_view_model
    }

    /// Get the terminal buffer as a string for snapshot comparisons
    pub fn buffer_content(&self) -> String {
        let buffer = self.terminal.backend().buffer();
        let area = buffer.area();
        let mut result = String::new();

        for y in 0..area.height {
            for x in 0..area.width {
                let cell = buffer.get(x, y);
                result.push_str(cell.symbol());
            }
            if y < area.height - 1 {
                result.push('\n');
            }
        }

        result
    }

    /// Assert ViewModel state
    fn assert_view_model(
        &self,
        expected_focus: &str,
        expected_selected: Option<usize>,
    ) -> Result<(), String> {
        let vm = &self.current_view_model;

        // Check focus
        let actual_focus = "tasks"; // In the new design, we only have tasks
        if actual_focus != expected_focus {
            return Err(format!(
                "Focus assertion failed: expected '{}', got '{}'",
                expected_focus, actual_focus
            ));
        }

        // Check selection if specified
        if let Some(expected_idx) = expected_selected {
            let actual_selected = match expected_focus {
                "tasks" => vm.selected_task_index as i64,
                _ => {
                    return Err(format!(
                        "Cannot check selection for focus '{}'",
                        expected_focus
                    ));
                }
            };

            if actual_selected != expected_idx as i64 {
                return Err(format!(
                    "Selection assertion failed for {}: expected {}, got {}",
                    expected_focus, expected_idx, actual_selected
                ));
            }
        }

        Ok(())
    }

    /// Parse a key string into a KeyEvent
    fn parse_key(&self, key: &str) -> Result<KeyEvent, String> {
        match key {
            "Tab" => Ok(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            "Down" => Ok(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            "Up" => Ok(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            "Enter" => Ok(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            "Esc" => Ok(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            "Backspace" => Ok(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            // Add more key mappings as needed
            other => {
                // Try to parse as a single character
                if other.len() == 1 {
                    let c = other.chars().next().unwrap();
                    Ok(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                } else {
                    Err(format!("Unsupported key: {}", other))
                }
            }
        }
    }

    /// Render the current state to the test terminal
    fn render(&mut self) -> Result<(), String> {
        self.terminal
            .draw(|f| {
                let area = f.size();
                let mut project_state = ratatui::widgets::ListState::default();
                let mut branch_state = ratatui::widgets::ListState::default();
                let mut agent_state = ratatui::widgets::ListState::default();

                ui::draw_task_dashboard(f, area, &self.current_view_model, None, None);
            })
            .map_err(|e| format!("Render error: {}", e))?;

        Ok(())
    }
}

/// Execute a complete scenario
pub async fn execute_scenario<C: ClientApi>(
    client: Arc<C>,
    scenario: &Scenario,
    update_goldens: bool,
) -> Result<(), String> {
    // Set up fake time
    time::pause();

    let (width, height) = scenario
        .terminal
        .as_ref()
        .map(|t| (t.width.unwrap_or(80), t.height.unwrap_or(24)))
        .unwrap_or((80, 24));

    let mut runtime = TestRuntime::new(client, width, height, update_goldens);

    // Load initial data
    runtime.load_initial_data().await?;

    // Execute each step
    for step in &scenario.steps {
        runtime.execute_step(step, &scenario.name).await?;
    }

    Ok(())
}
