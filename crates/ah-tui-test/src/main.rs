// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! tui-test runner CLI

use ah_rest_client_mock::MockClient;
use ah_test_scenarios::{Scenario, Step};
// TODO: Update imports for new TUI architecture
// use ah_tui::{TestRuntime, execute_scenario, task::TaskState};
use clap::{Parser, Subcommand};
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{self, Clear, ClearType};
use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "tui-test")]
#[command(about = "TUI testing runner - run automated tests or interactive video player")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the TUI against a scenario file and validate assertions
    Run {
        /// Path to JSON/YAML scenario
        scenario_path: String,
        /// Start from step index
        #[arg(long)]
        start_step: Option<usize>,
        /// Stop after step index (inclusive)
        #[arg(long)]
        until_step: Option<usize>,
        /// Update golden files instead of asserting
        #[arg(long)]
        update_goldens: bool,
        /// Override terminal width
        #[arg(long)]
        terminal_width: Option<u16>,
        /// Override terminal height
        #[arg(long)]
        terminal_height: Option<u16>,
        /// Seed for randomized components
        #[arg(long)]
        seed: Option<u64>,
        /// Write JSON report to path
        #[arg(long)]
        report: Option<String>,
    },
    /// Interactive player for stepping through a scenario (video player style)
    Play {
        /// Path to JSON/YAML scenario
        scenario_path: String,
        /// Start from step index
        #[arg(long)]
        start_step: Option<usize>,
        /// Jump to step index
        #[arg(long)]
        jump: Option<usize>,
        /// Run without opening a real TTY (uses TestBackend)
        #[arg(long)]
        headless: bool,
        /// Record per-step VM state and buffer
        #[arg(long)]
        trace: bool,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            scenario_path,
            update_goldens,
            ..
        } => {
            let data = std::fs::read_to_string(&scenario_path)?;
            let scenario = Scenario::from_str(&data)?;

            // Create mock client
            let client = Arc::new(MockClient::from_scenario_name(&scenario.name));

            // Execute the scenario
            match execute_scenario(client, &scenario, update_goldens).await {
                Ok(()) => {
                    println!("✅ Scenario '{}' executed successfully", scenario.name);
                }
                Err(e) => {
                    eprintln!("❌ Scenario '{}' failed: {}", scenario.name, e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Play {
            scenario_path,
            start_step,
            jump,
            headless: _,
            trace,
        } => {
            let data = std::fs::read_to_string(&scenario_path)?;
            let scenario = Scenario::from_str(&data)?;

            // Create mock client
            let client = Arc::new(MockClient::from_scenario_name(&scenario.name));

            // Initialize test runtime
            let (width, height) = scenario
                .terminal
                .as_ref()
                .map(|t| (t.width.unwrap_or(80), t.height.unwrap_or(24)))
                .unwrap_or((80, 24));

            let mut runtime = TestRuntime::new(client.clone(), width, height, false); // Interactive mode doesn't update goldens

            // Set up interactive session
            let mut current_step = start_step.unwrap_or(0);
            if let Some(jump_to) = jump {
                current_step = jump_to.min(scenario.steps.len());
            }

            println!("🎬 Interactive Scenario Player");
            println!("==============================");
            println!("Scenario: {}", scenario.name);
            println!("Terminal: {}x{}", width, height);
            println!("Steps: {}", scenario.steps.len());
            println!();

            // Freeze time for deterministic execution
            tokio::time::pause();

            // Interactive loop
            run_interactive_player(&scenario, &mut runtime, current_step, trace).await?;
        }
    }
    Ok(())
}

/// Redraw the entire screen (TUI + status + shortcuts)
fn redraw_screen(
    scenario: &Scenario,
    runtime: &TestRuntime<MockClient>,
    current_step: usize,
    executed_steps: usize,
) {
    // Clear screen
    let mut stdout = io::stdout();
    let _ = execute!(stdout, Clear(ClearType::All), cursor::MoveTo(0, 0));
    show_tui_display(runtime);

    // Show compact status
    show_compact_status(scenario, runtime, current_step, executed_steps);

    // Show keyboard shortcuts on single line at bottom
    show_keyboard_shortcuts();
}

/// Cleanup terminal state on exit
fn cleanup_terminal() {
    let mut stdout = io::stdout();
    let _ = terminal::disable_raw_mode();
    let _ = execute!(
        stdout,
        Clear(ClearType::All),
        cursor::MoveTo(0, 0),
        cursor::Show
    );
    let _ = io::stdout().flush();
}

/// Run the interactive scenario player
async fn run_interactive_player(
    scenario: &Scenario,
    runtime: &mut TestRuntime<MockClient>,
    mut current_step: usize,
    trace: bool,
) -> anyhow::Result<()> {
    let mut executed_steps = 0; // Track how many steps we've actually executed

    // Setup terminal for raw mode
    terminal::enable_raw_mode().map_err(|e| {
        cleanup_terminal();
        e
    })?;

    let result: anyhow::Result<()> = async {
        // Initial display
        redraw_screen(scenario, runtime, current_step, executed_steps);

        loop {
            // Read key press (blocking)
            let key_event = match event::read()? {
                Event::Key(key) => key,
                _ => continue, // Ignore non-key events
            };

            // Clear the status lines before processing command feedback
            let mut stdout = io::stdout();
            // Clear lines 24-26 where status and shortcuts appear
            for line in 24..=26 {
                let _ = execute!(
                    stdout,
                    cursor::MoveTo(0, line),
                    terminal::Clear(terminal::ClearType::CurrentLine)
                );
            }
            let _ = execute!(stdout, cursor::MoveTo(0, 24)); // Position cursor for command output
            io::stdout().flush()?;

            match key_event.code {
                // Forward navigation
                KeyCode::Char('n') | KeyCode::Right => {
                    if current_step < scenario.steps.len() {
                        // Execute the current step
                        match runtime
                            .execute_step(&scenario.steps[current_step], &scenario.name)
                            .await
                        {
                            Ok(()) => {
                                executed_steps += 1;
                                current_step += 1;
                            }
                            Err(e) => {
                                // Show error briefly, then continue
                                println!("❌ Step {} failed: {}", current_step + 1, e);
                                std::thread::sleep(std::time::Duration::from_secs(2));
                            }
                        }
                        // Redraw after state change
                        redraw_screen(scenario, runtime, current_step, executed_steps);
                    }
                }
                // Backward navigation
                KeyCode::Char('b') | KeyCode::Left => {
                    if executed_steps > 0 {
                        // Reset and replay up to previous step
                        executed_steps = executed_steps.saturating_sub(1);
                        current_step = executed_steps;
                        if let Err(e) = reset_and_replay(scenario, runtime, executed_steps).await {
                            println!("❌ Failed to reset: {}", e);
                            std::thread::sleep(std::time::Duration::from_secs(2));
                        }
                        // Redraw after state change
                        redraw_screen(scenario, runtime, current_step, executed_steps);
                    }
                }
                // Fast backward (10 steps)
                KeyCode::Up => {
                    if executed_steps > 0 {
                        let target_step = executed_steps.saturating_sub(10);
                        executed_steps = target_step;
                        current_step = target_step;
                        if let Err(e) = reset_and_replay(scenario, runtime, target_step).await {
                            println!("❌ Failed to jump: {}", e);
                            std::thread::sleep(std::time::Duration::from_secs(2));
                        }
                        // Redraw after state change
                        redraw_screen(scenario, runtime, current_step, executed_steps);
                    }
                }
                // Fast forward (10 steps)
                KeyCode::Down => {
                    let target_step = (executed_steps + 10).min(scenario.steps.len());
                    if target_step > executed_steps {
                        // Execute multiple steps at once
                        for step_idx in executed_steps..target_step {
                            if let Err(e) = runtime
                                .execute_step(&scenario.steps[step_idx], &scenario.name)
                                .await
                            {
                                println!("❌ Step {} failed: {}", step_idx + 1, e);
                                std::thread::sleep(std::time::Duration::from_secs(2));
                                break;
                            }
                        }
                        executed_steps = target_step;
                        current_step = target_step;
                        // Redraw after state change
                        redraw_screen(scenario, runtime, current_step, executed_steps);
                    }
                }
                // Reset to beginning
                KeyCode::Char('r') => {
                    executed_steps = 0;
                    current_step = 0;
                    if let Err(e) = reset_and_replay(scenario, runtime, 0).await {
                        println!("❌ Failed to reset: {}", e);
                        std::thread::sleep(std::time::Duration::from_secs(2));
                    }
                    // Redraw after state change
                    redraw_screen(scenario, runtime, current_step, executed_steps);
                }
                // Show detailed buffer info
                KeyCode::Char('s') => {
                    // Clear screen and show detailed buffer info
                    let mut stdout = io::stdout();
                    let _ = execute!(stdout, Clear(ClearType::All), cursor::MoveTo(0, 0));
                    show_tui_buffer(runtime);
                    println!("\nPress any key to continue...");
                    let _ = event::read(); // Wait for any key
                }
                // Show detailed ViewModel info
                KeyCode::Char('v') => {
                    // Clear screen and show detailed ViewModel info
                    let mut stdout = io::stdout();
                    let _ = execute!(stdout, Clear(ClearType::All), cursor::MoveTo(0, 0));
                    show_viewmodel_details(runtime);
                    println!("\nPress any key to continue...");
                    let _ = event::read(); // Wait for any key
                }
                // Quit
                KeyCode::Char('q') | KeyCode::Esc => {
                    break;
                }
                // Jump to step (single digit for now)
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    if let Ok(step_num) = c.to_string().parse::<usize>() {
                        if step_num <= scenario.steps.len() {
                            executed_steps = step_num;
                            current_step = step_num;
                            if let Err(e) = reset_and_replay(scenario, runtime, step_num).await {
                                println!("❌ Failed to jump: {}", e);
                                std::thread::sleep(std::time::Duration::from_secs(2));
                            }
                            // Redraw after state change
                            redraw_screen(scenario, runtime, current_step, executed_steps);
                        } else {
                            println!(
                                "❌ Invalid step number: {} (max: {})",
                                step_num,
                                scenario.steps.len()
                            );
                            std::thread::sleep(std::time::Duration::from_secs(2));
                        }
                    }
                }
                // Ignore other keys
                _ => {}
            }
        }
        Ok(())
    }
    .await;

    // Always cleanup terminal, even on error
    cleanup_terminal();

    result
}

/// Reset the runtime and replay steps up to the specified step
async fn reset_and_replay(
    scenario: &Scenario,
    runtime: &mut TestRuntime<MockClient>,
    up_to_step: usize,
) -> anyhow::Result<()> {
    // Create fresh runtime
    let (width, height) = scenario
        .terminal
        .as_ref()
        .map(|t| (t.width.unwrap_or(80), t.height.unwrap_or(24)))
        .unwrap_or((80, 24));

    let client = Arc::new(MockClient::from_scenario_name(&scenario.name));
    *runtime = TestRuntime::new(client, width, height, false); // Replay doesn't update goldens

    // Reload initial data
    runtime
        .load_initial_data()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to load initial data: {}", e))?;

    // Replay steps up to the target
    for i in 0..up_to_step {
        runtime
            .execute_step(&scenario.steps[i], &scenario.name)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to execute step {}: {}", i, e))?;
    }

    Ok(())
}

/// Describe a step in human-readable format
fn describe_step(step: &Step) -> String {
    match step {
        Step::AdvanceMs { ms } => format!("Advance {}ms", ms),
        Step::Key { key } => format!("Press key '{}'", key),
        Step::Sse { .. } => "Receive SSE event".to_string(),
        Step::AssertVm { focus, selected } => {
            format!("Assert focus='{}' selected={:?}", focus, selected)
        }
        Step::Snapshot { name } => format!("Take snapshot '{}'", name),
    }
}

/// Show the current TUI buffer in a simplified text format
fn show_tui_buffer(runtime: &TestRuntime<MockClient>) {
    println!("🖥️  TUI Buffer");
    println!("============");

    let buffer = runtime.buffer_content();
    let lines: Vec<&str> = buffer.split('\n').collect();

    // Show first 20 lines with line numbers
    for (i, line) in lines.iter().enumerate().take(20) {
        if line.trim().is_empty() {
            continue;
        }
        println!("{:2}: {}", i + 1, line);
    }

    if lines.len() > 20 {
        println!("... ({} more lines)", lines.len() - 20);
    }
}

/// Show the current TUI display (always visible in video player mode)
fn show_tui_display(runtime: &TestRuntime<MockClient>) {
    let buffer = runtime.buffer_content();
    let mut stdout = io::stdout();

    // Print each line at the correct position
    for (row, line) in buffer.lines().enumerate() {
        let _ = execute!(
            stdout,
            cursor::MoveTo(0, row as u16),
            crossterm::style::Print(line)
        );
    }

    let _ = io::stdout().flush();
}

/// Show compact status information
fn show_compact_status(
    scenario: &Scenario,
    runtime: &TestRuntime<MockClient>,
    current_step: usize,
    executed_steps: usize,
) {
    let vm = runtime.view_model();
    let mut stdout = io::stdout();

    // Single line status bar at line 24 (after 24-line TUI)
    let next_desc = if current_step < scenario.steps.len() {
        describe_step(&scenario.steps[current_step])
    } else {
        "[END OF SCENARIO]".to_string()
    };

    let task_info = if let Some(task) = vm.selected_task() {
        let status = match &task.state {
            TaskState::Merged { .. } => "Merged",
            TaskState::Completed { .. } => "Completed",
            TaskState::Active { .. } => "Active",
            TaskState::Draft { .. } => "Draft",
            TaskState::New { .. } => "New",
        };
        format!("Task: {} ({})", task.id, status)
    } else {
        "No task selected".to_string()
    };

    let status_line = format!(
        "📊 Step {}/{} | {} | Next: {}",
        current_step,
        scenario.steps.len(),
        task_info,
        next_desc
    );
    let _ = execute!(
        stdout,
        cursor::MoveTo(0, 24),
        crossterm::style::Print(&status_line)
    );

    // Loading/error status on next line if present
    if vm.loading {
        let status_line = "   Loading...";
        let _ = execute!(
            stdout,
            cursor::MoveTo(0, 25),
            crossterm::style::Print(status_line)
        );
    } else if let Some(error) = &vm.error_message {
        let status_line = format!("   Error: {}", error);
        let _ = execute!(
            stdout,
            cursor::MoveTo(0, 25),
            crossterm::style::Print(&status_line)
        );
    }
}

/// Show keyboard shortcuts on a single line
fn show_keyboard_shortcuts() {
    let mut stdout = io::stdout();
    let shortcuts = "🎮 n/→:next | b/←:prev | ↑:back 10 | ↓:forward 10 | r:reset | 0-9:jump | s:show | v:view | q:quit";
    let _ = execute!(
        stdout,
        cursor::MoveTo(0, 26),
        crossterm::style::Print(shortcuts)
    );
}

/// Show detailed ViewModel state
fn show_viewmodel_details(runtime: &TestRuntime<MockClient>) {
    println!("🔍 ViewModel Details");
    println!("===================");

    let vm = runtime.view_model();

    println!("Tasks: {}", vm.tasks.len());
    println!("Selected Task Index: {}", vm.selected_task_index);
    println!("Has Unsaved Draft: {}", vm.has_unsaved_draft);
    println!("Loading: {}", vm.loading);

    if let Some(error) = &vm.error_message {
        println!("Error: {}", error);
    }

    println!();
    println!("Task List:");
    for (i, task) in vm.tasks.iter().enumerate() {
        let marker = if i == vm.selected_task_index {
            "▶"
        } else {
            " "
        };
        let status = match &task.state {
            TaskState::Merged { .. } => "Merged",
            TaskState::Completed { .. } => "Completed",
            TaskState::Active { .. } => "Active",
            TaskState::Draft { .. } => "Draft",
            TaskState::New { .. } => "New",
        };
        println!("  {} {} - {}", marker, task.id, status);
    }
}
