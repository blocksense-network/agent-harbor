//! MVVM Architecture TUI Application
//!
//! This is a new implementation of the TUI using proper MVVM architecture
//! with clean separation between Model (business logic), ViewModel (UI logic),
//! and View (rendering).

use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use crossterm::{
    event::{self, Event},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use std::{
    io::stdout,
    time::Duration,
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    thread,
    panic,
};
use crossbeam_channel as chan;

use tui_exploration::{
    model::Model,
    view_model::ViewModel,
    view,
    workspace_files::GitWorkspaceFiles,
    workspace_workflows::PathWorkspaceWorkflows,
};

/// Arguments for the MVVM application
#[derive(Debug, Clone)]
struct Args {
    enable_raw_mode: bool,
}

/// Parse command line arguments
fn parse_args() -> Args {
    let mut enable_raw_mode = true;

    let args: Vec<String> = std::env::args().collect();
    for arg in &args[1..] {
        match arg.as_str() {
            "--no-raw-mode" => enable_raw_mode = false,
            _ => eprintln!("Unknown argument: {}", arg),
        }
    }

    Args { enable_raw_mode }
}

/// Setup terminal for TUI
fn setup_terminal(enable_raw_mode: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = stdout();

    if enable_raw_mode {
        crossterm::terminal::enable_raw_mode()?;
    }

    stdout.execute(EnterAlternateScreen)?;
    Ok(())
}

/// Cleanup terminal after TUI
fn cleanup_terminal() {
    let mut stdout = stdout();

    // Try to cleanup as much as possible even if some operations fail
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = stdout.execute(LeaveAlternateScreen);
}

/// Cleanup and exit with code
fn cleanup_and_exit(code: i32) -> ! {
    cleanup_terminal();
    std::process::exit(code);
}

/// Main application function using MVVM architecture
async fn run_app_mvvm(
    running: &Arc<AtomicBool>,
    enable_raw_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    setup_terminal(enable_raw_mode)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Initialize MVVM components
    let mut model = Model::default();

    // Create service instances
    let workspace_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let workspace_files = Box::new(GitWorkspaceFiles::new(workspace_dir.clone()));
    let workspace_workflows = Box::new(PathWorkspaceWorkflows::new(workspace_dir));

    let mut view_model = ViewModel::new(&model, workspace_files, workspace_workflows);

    // Create channels for event handling
    let (tx_ev, rx_ev) = chan::unbounded::<Event>();
    let (tx_tick, rx_tick) = chan::unbounded::<()>();

    // Event reader thread
    thread::spawn(move || {
        while let Ok(ev) = event::read() {
            let _ = tx_ev.send(ev);
        }
    });

    // Tick thread for periodic updates (~60 FPS)
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(16));
            let _ = tx_tick.send(());
        }
    });

    // Main event loop
    loop {
        // Check if we should exit
        if !running.load(Ordering::SeqCst) {
            break;
        }

        // Handle events (non-blocking)
        let timeout = Duration::from_millis(10);
        match rx_ev.recv_timeout(timeout) {
            Ok(event) => {
                match event {
                    Event::Key(key) => {
                        // Handle the key event in ViewModel
                        let domain_msgs = view_model.handle_key_event(key, &model);
                        // Process domain messages in Model
                        let mut ui_msgs = Vec::new();
                        for msg in domain_msgs {
                            let additional_msgs = model.update_domain(msg);
                            ui_msgs.extend(additional_msgs);
                        }
                        // Handle any UI state changes from Model
                        for _msg in ui_msgs {
                            // Handle UI state changes if needed
                        }
                    }
                    Event::Mouse(_mouse_event) => {
                        // Handle mouse events if needed (TODO)
                    }
                    Event::Resize(_width, _height) => {
                        // Handle resize events if needed
                        let _ = terminal.autoresize();
                    }
                    _ => {}
                }
            }
            Err(_) => {
                // No event, continue to next iteration
            }
        }

        // Handle tick events (for now, just consume them)
        while rx_tick.try_recv().is_ok() {
            // TODO: Handle periodic updates like activity simulation
        }

        // Update footer with current state
        view_model.update_footer(&model);

        // Render the UI
        terminal.draw(|frame| {
            view::render(frame, &view_model);
        })?;
    }

    Ok(())
}

/// Main entry point for the MVVM TUI application
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();

    // Install signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        cleanup_terminal();
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Install panic hook for cleanup on panic
    let default_panic = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        cleanup_terminal();
        default_panic(panic_info);
    }));

    // Run the app
    let result = run_app_mvvm(&running, args.enable_raw_mode).await;

    // Ensure cleanup happens
    cleanup_terminal();

    result
}
