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
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event,
        KeyEventKind, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
        KeyboardEnhancementFlags,
    },
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
    fs::OpenOptions,
};
use tracing::{trace};
use tracing_subscriber;
use futures::StreamExt;
use crossbeam_channel as chan;

use tui_exploration::{
    view_model::ViewModel,
    view,
    TaskManager,
    ViewCache,
    settings::Settings,
    workspace_files::GitWorkspaceFiles,
    workspace_workflows::PathWorkspaceWorkflows,
    TaskState,
    ModalState,
};

use ah_rest_mock_client::MockRestClient;
use ah_tui::Theme;
use image::GenericImageView;

/// Initialize logo rendering components (Picker and StatefulProtocol)
fn initialize_logo_rendering(bg_color: ratatui::style::Color) -> (Option<ratatui_image::picker::Picker>, Option<ratatui_image::protocol::StatefulProtocol>) {
    use ratatui_image::picker::Picker;
    use ratatui_image::protocol::StatefulProtocol;
    use image::{DynamicImage, ImageReader};

    // Try to create a picker that detects terminal graphics capabilities
    let picker = match Picker::from_query_stdio() {
        Ok(picker) => Some(picker),
        Err(_) => {
            // If we can't detect terminal capabilities, try with default font size
            // This allows for basic image processing
            Some(Picker::from_fontsize((8, 16)))
        }
    };

    // Try to load and encode the logo image
    let logo_protocol = if let Some(ref picker) = picker {
        let cell_width = Some(picker.font_size().0);
        // Try to load the PNG logo
        match ImageReader::open("../../assets/agent-harbor-logo.png") {
            Ok(reader) => match reader.decode() {
                Ok(img) => {
                    // Compose the transparent logo onto the themed background before encoding.
                    let composed = precompose_on_background(img, bg_color);
                    let prepared = pad_to_cell_width(composed, bg_color, cell_width);
                    Some(picker.new_resize_protocol(prepared) as StatefulProtocol)
                }
                Err(_) => None,
            },
            Err(_) => None,
        }
    } else {
        None
    };

    (picker, logo_protocol)
}

/// Convert a Ratatui color into raw RGB components (default to black for non-RGB variants).
fn color_to_rgb_components(color: ratatui::style::Color) -> (u8, u8, u8) {
    match color {
        ratatui::style::Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    }
}

/// Blend the transparent regions of the logo onto the TUI background color before rendering.
fn precompose_on_background(image: image::DynamicImage, bg_color: ratatui::style::Color) -> image::DynamicImage {
    let (r, g, b) = color_to_rgb_components(bg_color);
    let rgba_logo = image.to_rgba8();
    let (width, height) = rgba_logo.dimensions();
    let mut background = image::RgbaImage::from_pixel(width, height, image::Rgba([r, g, b, 255]));
    image::imageops::overlay(&mut background, &rgba_logo, 0, 0);
    image::DynamicImage::ImageRgba8(background)
}

/// Pad the image width so it fills complete terminal cells, avoiding partially transparent columns.
fn pad_to_cell_width(
    image: image::DynamicImage,
    bg_color: ratatui::style::Color,
    cell_width: Option<u16>,
) -> image::DynamicImage {
    let cell_width = match cell_width {
        Some(width) if width > 0 => width as u32,
        _ => return image,
    };

    let (width, height) = image.dimensions();
    let remainder = width % cell_width;
    if remainder == 0 {
        return image;
    }

    let pad_width = cell_width - remainder;
    let (r, g, b) = color_to_rgb_components(bg_color);
    let mut canvas = image::RgbaImage::from_pixel(width + pad_width, height, image::Rgba([r, g, b, 255]));
    image::imageops::overlay(&mut canvas, &image.to_rgba8(), 0, 0);
    image::DynamicImage::ImageRgba8(canvas)
}

// Simple render function for performance testing
fn simple_render(frame: &mut ratatui::Frame<'_>, view_model: &mut ViewModel) {
    use ratatui::{widgets::Paragraph, style::Style};

    let area = frame.area();
    let text = format!(
        "Simple Render - Draft cards: {}, Tasks: {}, Focus: {:?}",
        view_model.draft_cards.len(),
        view_model.task_cards.len(),
        view_model.focus_element
    );

    let paragraph = Paragraph::new(text).style(Style::default());
    frame.render_widget(paragraph, area);
}

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
        RAW_MODE_ENABLED.store(true, Ordering::SeqCst);
    }

    stdout.execute(EnterAlternateScreen)?;
    ALTERNATE_SCREEN_ACTIVE.store(true, Ordering::SeqCst);

    // Setup enhanced keyboard and mouse support for better image rendering
    stdout.execute(PushKeyboardEnhancementFlags(
        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
            | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
    ))?;
    KB_FLAGS_PUSHED.store(true, Ordering::SeqCst);

    stdout.execute(EnableMouseCapture)?;
    MOUSE_CAPTURE_ENABLED.store(true, Ordering::SeqCst);

    Ok(())
}

/// Cleanup terminal after TUI
fn cleanup_terminal() {
    if CLEANUP_DONE.swap(true, Ordering::SeqCst) {
        return; // Already cleaned up
    }

    let mut stdout = stdout();

    // Pop keyboard enhancement flags first (must be done while still in raw mode/alternate screen)
    if KB_FLAGS_PUSHED.load(Ordering::SeqCst) {
        let _ = stdout.execute(PopKeyboardEnhancementFlags);
        KB_FLAGS_PUSHED.store(false, Ordering::SeqCst);
    }

    if MOUSE_CAPTURE_ENABLED.load(Ordering::SeqCst) {
        let _ = stdout.execute(DisableMouseCapture);
        MOUSE_CAPTURE_ENABLED.store(false, Ordering::SeqCst);
    }

    // Disable raw mode next
    if RAW_MODE_ENABLED.load(Ordering::SeqCst) {
        let _ = crossterm::terminal::disable_raw_mode();
        RAW_MODE_ENABLED.store(false, Ordering::SeqCst);
    }

    // Leave alternate screen last
    if ALTERNATE_SCREEN_ACTIVE.load(Ordering::SeqCst) {
        let _ = stdout.execute(LeaveAlternateScreen);
        ALTERNATE_SCREEN_ACTIVE.store(false, Ordering::SeqCst);
    }
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

    // Create service instances
    let workspace_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let workspace_files = Box::new(GitWorkspaceFiles::new(workspace_dir.clone()));
    let workspace_workflows = Box::new(PathWorkspaceWorkflows::new(workspace_dir));
    let task_manager = Box::new(MockRestClient::with_mock_data());

    let settings = Settings::default();
    let mut view_model = ViewModel::new(workspace_files, workspace_workflows, task_manager, settings);

    // Initialize view cache with image rendering components
    let theme = Theme::default();
    let (picker, logo_protocol) = initialize_logo_rendering(theme.bg);
    let mut view_cache = ViewCache::new();
    view_cache.picker = picker;
    view_cache.logo_protocol = logo_protocol;

    // Load initial mock tasks to populate the UI
    view_model.load_initial_tasks().await?;

    // Create channels for event handling
    let (tx_ev, rx_ev) = chan::unbounded::<Event>();

    // Use coalescing tick channel that never builds a backlog
    let rx_tick = chan::tick(Duration::from_millis(16));

    // Event reader thread
    thread::spawn(move || {
        while let Ok(ev) = event::read() {
            // Play a bell sound immediately when event is received from channel
            // This provides instant feedback regardless of processing delays
            let _ = tx_ev.send(ev);
        }
    });

    // Main event loop
    loop {
        // Check if we should exit due to interrupt signal
        if !running.load(Ordering::SeqCst) {
            break;
        }

        // Use biased select to prefer input events over ticks
        chan::select_biased! {
            recv(rx_ev) -> msg => {
                let event = match msg {
                    Ok(e) => e,
                    Err(_) => break,
                };

                match event {
                    Event::Key(key) => {
                        // Key logging for debugging (trace level, disabled by default)
                        trace!(
                            key_code = ?key.code,
                            modifiers = ?key.modifiers,
                            key_kind = ?key.kind,
                            focus_element = ?view_model.focus_element,
                            "Key event received"
                        );

                        // Handle ESC key directly to exit (like main.rs)
                        if key.code == crossterm::event::KeyCode::Esc {
                            break;
                        }
                        // Send key event to ViewModel via message system
                        let msg = tui_exploration::view_model::Msg::Key(key);
                        if let Err(error) = view_model.update(msg) {
                            eprintln!("Error handling key event: {}", error);
                        }
                    }
                    Event::Mouse(mouse_event) => {
                        // Send mouse event to ViewModel
                        let msg = tui_exploration::view_model::Msg::Mouse(mouse_event);
                        let _ = view_model.update(msg);
                    }
                    Event::Resize(_width, _height) => {
                        // Handle resize events if needed
                        let _ = terminal.autoresize();
                        view_model.needs_redraw = true; // Force redraw on resize
                    }
                    _ => {}
                }

                // Process any pending task events
                view_model.process_pending_task_events();

                // After handling an event, update footer and redraw if needed
                view_model.update_footer();
                if view_model.needs_redraw {
                    terminal.draw(|frame| {
                        let size = frame.area();
                        // Use full render for production
                        // simple_render(frame, &mut view_model);
                        view::render(frame, &mut view_model, &mut view_cache);

                        // Render modals on top of main UI
                        render_modals(frame, &view_model, size, &theme);
                    })?;
                    view_model.needs_redraw = false;
                }
            }
            recv(rx_tick) -> _ => {
                // Handle tick events (activity simulation)
                let msg = tui_exploration::view_model::Msg::Tick;
                let _ = view_model.update(msg);

                // Only redraw if tick actually changed something
                if view_model.needs_redraw {
                    view_model.update_footer();
                    terminal.draw(|frame| {
                        let size = frame.area();
                        // Use full render for production
                        // simple_render(frame, &mut view_model);
                        view::render(frame, &mut view_model, &mut view_cache);

                        // Render modals on top of main UI
                        render_modals(frame, &view_model, size, &theme);
                    })?;
                    view_model.needs_redraw = false;
                }
            }
        }
    }

    Ok(())
}

/// Render modals on top of the main UI
fn render_modals(frame: &mut ratatui::Frame, view_model: &ViewModel, area: ratatui::layout::Rect, theme: &ah_tui::Theme) {
    use ah_tui::view::dialogs::{
        render_fuzzy_modal, render_model_selection_modal, render_settings_dialog,
        render_go_to_line_modal, render_find_replace_modal, render_shortcut_help_modal,
    };

    match view_model.modal_state {
        ModalState::None => {
            // No modal to render
        }
        ModalState::RepositorySearch => {
            // Create a placeholder fuzzy search modal for repository selection
            let modal = ah_tui::view::dialogs::FuzzySearchModal {
                input: String::new(),
                options: view_model.available_repositories.clone(),
                selected_index: 0,
            };
            render_fuzzy_modal(frame, &modal, area, theme);
        }
        ModalState::BranchSearch => {
            // Create a placeholder fuzzy search modal for branch selection
            let modal = ah_tui::view::dialogs::FuzzySearchModal {
                input: String::new(),
                options: view_model.available_branches.clone(),
                selected_index: 0,
            };
            render_fuzzy_modal(frame, &modal, area, theme);
        }
        ModalState::ModelSearch => {
            // Create a placeholder fuzzy search modal for model selection
            let modal = ah_tui::view::dialogs::FuzzySearchModal {
                input: String::new(),
                options: view_model.available_models.clone(),
                selected_index: 0,
            };
            render_fuzzy_modal(frame, &modal, area, theme);
        }
        ModalState::Settings => {
            render_settings_dialog(frame, area, theme);
        }
    }
}

// Global flag to ensure cleanup only happens once
static CLEANUP_DONE: AtomicBool = AtomicBool::new(false);

// Track what we modified so we can restore properly
static RAW_MODE_ENABLED: AtomicBool = AtomicBool::new(false);
static ALTERNATE_SCREEN_ACTIVE: AtomicBool = AtomicBool::new(false);
static KB_FLAGS_PUSHED: AtomicBool = AtomicBool::new(false);
static MOUSE_CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting MVVM TUI application...");
    let args = parse_args();
    println!("Parsed args: enable_raw_mode={}", args.enable_raw_mode);

    // Initialize tracing for key logging (disabled by default, enable with RUST_LOG=trace)
    // Output goes to tui-mvvm-trace.log file
    let trace_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("tui-mvvm-trace.log")
        .expect("Failed to open trace log file");

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(trace_file)
        .init();

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
