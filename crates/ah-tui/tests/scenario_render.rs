//! Scenario-based initial render test

use ah_test_scenarios::{Scenario, ScenarioTerminal};
use ah_tui::{app::AppState, create_test_terminal, ViewModel};
use ratatui::widgets::ListState;

#[test]
fn test_initial_render_from_minimal_scenario() -> anyhow::Result<()> {
    // Minimal scenario JSON (could be moved to a fixture file later)
    let json = r#"{
        "name": "minimal_initial",
        "terminal": { "width": 80, "height": 24 },
        "steps": []
    }"#;

    let scenario = Scenario::from_str(json)?;
    let (w, h) = scenario
        .terminal
        .map(|t| (t.width.unwrap_or(80), t.height.unwrap_or(24)))
        .unwrap_or((80, 24));

    let mut term = create_test_terminal(w, h);

    // Render the current dashboard with default state
    term.draw(|f| {
        let area = f.size();
        let mut project_state = ListState::default();
        let mut branch_state = ListState::default();
        let mut agent_state = ListState::default();

        // Build a default AppState via a lightweight path: reuse draw_dashboard with empty data
        let state = AppState::default();
        let view_model = ViewModel::from_state(&state);
        ah_tui::ui::draw_task_dashboard(f, area, &view_model, None, None);
    })?;

    let buffer = term.backend().buffer();
    let all_text = buffer.content().iter().map(|c| c.symbol()).collect::<String>();

    // Expect the static section titles to be present
    assert!(
        all_text.contains("╔"),
        "Should render header with logo border"
    );
    assert!(all_text.contains("New Task"));
    assert!(all_text.contains("Description"));

    Ok(())
}

/// Golden snapshot tests using tui-testing framework
#[tokio::test]
async fn test_tui_initial_screen_golden() -> anyhow::Result<()> {
    use tui_testing::*;

    // Get the path to the built ah-tui binary
    let binary_path = std::env::current_exe()?
        .parent()
        .unwrap() // target/debug/deps
        .parent()
        .unwrap() // target/debug
        .parent()
        .unwrap() // target
        .parent()
        .unwrap() // project root
        .join("target")
        .join("debug")
        .join(if cfg!(windows) {
            "ah-tui.exe"
        } else {
            "ah-tui"
        });

    // Create a test runner for the ah-tui binary
    let mut runner = TestedTerminalProgram::new(binary_path.to_string_lossy().as_ref())
        .width(80)
        .height(24)
        .spawn()
        .await?;

    // Wait a moment for the TUI to initialize and render
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Read and parse the screen output
    runner.read_and_parse().await?;

    // Send Ctrl+C to exit the application
    runner.send_control('c').await?;

    // Wait for the application to exit
    runner.wait().await?;

    // Get the screen contents
    let screen_contents = runner.screen_contents();

    // Normalize non-deterministic content for consistent snapshots
    let normalized_screen = normalize_screen_content(&screen_contents);

    // Use insta for golden file testing (platform-specific snapshots)
    insta::assert_snapshot!(format!("initial_screen_{}", std::env::consts::OS), normalized_screen);

    Ok(())
}

/// Test TUI interaction scenarios with multiple screenshots
#[tokio::test]
async fn test_tui_interaction_scenario() -> anyhow::Result<()> {
    use tui_testing::*;

    // Get the path to the built ah-tui binary
    let binary_path = std::env::current_exe()?
        .parent()
        .unwrap() // target/debug/deps
        .parent()
        .unwrap() // target/debug
        .parent()
        .unwrap() // target
        .parent()
        .unwrap() // project root
        .join("target")
        .join("debug")
        .join(if cfg!(windows) {
            "ah-tui.exe"
        } else {
            "ah-tui"
        });

    // Create a test runner for the ah-tui binary
    let mut runner = TestedTerminalProgram::new(binary_path.to_string_lossy().as_ref())
        .width(120)
        .height(30)
        .spawn()
        .await?;

    // Wait for initial render
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    runner.read_and_parse().await?;

    // Capture initial screen
    let initial_screen = normalize_screen_content(&runner.screen_contents());
    insta::assert_snapshot!(format!("interaction_initial_{}", std::env::consts::OS), initial_screen);

    // Navigate down through tasks
    runner.send("\x1b[B").await?; // Down arrow
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    runner.read_and_parse().await?;

    // Navigate to the "New Task" section (should be the last item)
    for _ in 0..2 {
        // Navigate down to reach the New Task section
        runner.send("\x1b[B").await?; // Down arrow
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    runner.read_and_parse().await?;

    // Capture screen after navigation
    let navigation_screen = normalize_screen_content(&runner.screen_contents());
    insta::assert_snapshot!(format!("interaction_navigation_{}", std::env::consts::OS), navigation_screen);

    // Try to enter the description field and type something
    runner.send("\r").await?; // Enter (should focus description)
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    runner.read_and_parse().await?;

    runner.send("Test task description").await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    runner.read_and_parse().await?;

    // Capture screen with typed description
    let description_screen = normalize_screen_content(&runner.screen_contents());
    insta::assert_snapshot!(format!("interaction_description_{}", std::env::consts::OS), description_screen);

    // Exit the application
    runner.send_control('c').await?;
    runner.wait().await?;

    Ok(())
}

/// Normalize screen content to remove non-deterministic elements
fn normalize_screen_content(content: &str) -> String {
    let mut normalized = content.to_string();

    // Normalize timestamps (if any)
    normalized = regex::Regex::new(r"\d+[smhd] ago")
        .unwrap()
        .replace_all(&normalized, "[TIME_AGO]")
        .to_string();

    // Normalize cursor visibility escape sequences
    normalized = normalized.replace("\x1b[?25h", "").replace("\x1b[?25l", "");

    // Normalize cursor position escape sequences (can vary)
    normalized = regex::Regex::new(r"\x1b\[\d+;\d+H")
        .unwrap()
        .replace_all(&normalized, "[CURSOR_POS]")
        .to_string();

    // Normalize other potential non-deterministic content
    // Add more normalization rules as needed

    normalized
}
