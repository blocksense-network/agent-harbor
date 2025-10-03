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

/// Golden snapshot tests using expectrl + vt100 + insta
use expectrl::spawn;
use std::io::{Read, Write};
use std::time::Duration;
use vt100::Parser;

/// Capture a snapshot of the TUI screen using vt100 terminal emulation
fn snapshot_screen() -> anyhow::Result<String> {
    // Get the path to the built ah-tui binary (assuming we're running from the project root)
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

    // Spawn the TUI binary in local mode (no remote server)
    let mut p = spawn(binary_path.to_string_lossy().as_ref())?;

    // Set a timeout for the initial screen to render
    p.set_echo(false, None)?;

    // Create a vt100 parser for 80x24 terminal
    let mut parser = Parser::new(24, 80, 0);

    // Read initial output with a reasonable timeout
    let mut buf = [0u8; 8192];
    let mut total_read = 0;
    let start = std::time::Instant::now();

    // Read for up to 3 seconds or until we get some content
    while start.elapsed() < Duration::from_secs(3) && total_read < 2000 {
        match p.read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => {
                parser.process(&buf[..n]);
                total_read += n;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Brief pause before trying again
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }

    // Send Ctrl+C to exit the application
    p.send_control('c')?;
    p.flush()?;

    // Wait a bit for the application to exit
    std::thread::sleep(Duration::from_millis(100));

    // Get the formatted screen contents (with ANSI escape sequences)
    let screen_contents = parser.screen().contents_formatted();
    let mut screen_str = String::from_utf8_lossy(&screen_contents).to_string();

    // Normalize non-deterministic content for consistent snapshots
    // Normalize timestamps (e.g., "2h ago", "30s ago", etc.)
    screen_str = regex::Regex::new(r"\d+[smhd] ago")
        .unwrap()
        .replace_all(&screen_str, "[TIME_AGO]")
        .to_string();

    // Normalize cursor visibility (can vary between runs)
    screen_str = screen_str.replace("\x1b[?25h", "").replace("\x1b[?25l", "");

    Ok(screen_str)
}

#[test]
fn test_tui_initial_screen_snapshot() {
    // Only run this snapshot test if TUI_SNAPSHOTS environment variable is set
    // This test captures the actual TUI binary output which can vary due to timing,
    // terminal behavior, and application state. It's primarily for visual verification.
    if std::env::var("TUI_SNAPSHOTS").is_ok() {
        // Use insta to snapshot the initial TUI screen
        // This will create/update the golden file when explicitly requested
        insta::assert_snapshot!("initial_screen", snapshot_screen().unwrap());
    } else {
        // Skip the snapshot test by default to avoid flaky CI
        eprintln!("Skipping TUI snapshot test (set TUI_SNAPSHOTS=1 to enable)");
    }
}
