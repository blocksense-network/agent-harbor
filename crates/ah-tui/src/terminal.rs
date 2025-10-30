// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Terminal Management - Shared terminal setup and cleanup procedures
//!
//! This module provides shared functionality for setting up and cleaning up
//! the terminal for TUI applications, including raw mode, alternate screen,
//! keyboard enhancements, mouse capture, and signal handlers.

use crossterm::{
    ExecutableCommand,
    event::{
        DisableMouseCapture, EnableMouseCapture, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    io::{self, Stdout},
    panic,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

// External dependencies
use ctrlc;

// Global flag to ensure cleanup only happens once
static CLEANUP_DONE: AtomicBool = AtomicBool::new(false);

// Track what we modified so we can restore properly
static RAW_MODE_ENABLED: AtomicBool = AtomicBool::new(false);
static ALTERNATE_SCREEN_ACTIVE: AtomicBool = AtomicBool::new(false);
static KB_FLAGS_PUSHED: AtomicBool = AtomicBool::new(false);
static MOUSE_CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);

/// Terminal setup configuration
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    /// Enable raw mode
    pub raw_mode: bool,
    /// Enter alternate screen
    pub alternate_screen: bool,
    /// Enable enhanced keyboard support
    pub keyboard_enhancement: bool,
    /// Enable mouse capture
    pub mouse_capture: bool,
    /// Install signal handlers for graceful shutdown
    pub install_signal_handlers: bool,
    /// Running flag to control application lifecycle (used by signal handlers)
    pub running_flag: Option<Arc<AtomicBool>>,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            raw_mode: true,
            alternate_screen: true,
            keyboard_enhancement: true,
            mouse_capture: true,
            install_signal_handlers: true,
            running_flag: None,
        }
    }
}

impl TerminalConfig {
    /// Configuration for minimal terminal setup (raw mode + alternate screen only)
    pub fn minimal() -> Self {
        Self {
            raw_mode: true,
            alternate_screen: true,
            keyboard_enhancement: false,
            mouse_capture: false,
            install_signal_handlers: false,
            running_flag: None,
        }
    }

    /// Set the running flag for signal handlers
    pub fn with_running_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.running_flag = Some(flag);
        self
    }

    /// Disable signal handler installation
    pub fn without_signal_handlers(mut self) -> Self {
        self.install_signal_handlers = false;
        self
    }
}

/// Setup terminal for TUI with the specified configuration
pub fn setup_terminal(config: TerminalConfig) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout();

    if config.raw_mode {
        crossterm::terminal::enable_raw_mode()?;
        RAW_MODE_ENABLED.store(true, Ordering::SeqCst);
    }

    if config.alternate_screen {
        stdout.execute(EnterAlternateScreen)?;
        ALTERNATE_SCREEN_ACTIVE.store(true, Ordering::SeqCst);
    }

    // Set initial cursor style to bar (for insert mode)
    stdout.execute(crossterm::cursor::SetCursorStyle::SteadyBar)?;

    if config.keyboard_enhancement {
        // Setup enhanced keyboard support for better input handling
        stdout.execute(PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS,
        ))?;
        KB_FLAGS_PUSHED.store(true, Ordering::SeqCst);
    }

    if config.mouse_capture {
        stdout.execute(EnableMouseCapture)?;
        MOUSE_CAPTURE_ENABLED.store(true, Ordering::SeqCst);
    }

    // Install signal handlers if requested
    if config.install_signal_handlers {
        // Install signal handler for graceful shutdown
        if let Some(running_flag) = &config.running_flag {
            let r = running_flag.clone();
            ctrlc::set_handler(move || {
                cleanup_terminal();
                r.store(false, Ordering::SeqCst);
            })
            .expect("Error setting Ctrl-C handler");
        } else {
            ctrlc::set_handler(|| {
                cleanup_terminal();
            })
            .expect("Error setting Ctrl-C handler");
        }

        // Install panic hook for cleanup on panic
        let default_panic = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            cleanup_terminal();
            default_panic(panic_info);
        }));
    }

    Ok(())
}

/// Cleanup terminal after TUI
pub fn cleanup_terminal() {
    if CLEANUP_DONE.swap(true, Ordering::SeqCst) {
        return; // Already cleaned up
    }

    let mut stdout = io::stdout();

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

/// Quick setup for minimal terminal configuration (raw mode + alternate screen)
pub fn setup_terminal_minimal() -> Result<(), Box<dyn std::error::Error>> {
    setup_terminal(TerminalConfig::minimal())
}

/// Quick cleanup for terminal
pub fn cleanup_terminal_now() {
    cleanup_terminal();
}

pub fn key_event_to_bytes(key: &crossterm::event::KeyEvent) -> Option<Vec<u8>> {
    key_event_to_bytes_with_features(key, false)
}

pub fn key_event_to_bytes_with_features(
    key: &crossterm::event::KeyEvent,
    app_cursor_mode: bool,
) -> Option<Vec<u8>> {
    use crossterm::event::{KeyCode, KeyModifiers};

    // xterm modifier encoding: ;2=Shift ;3=Alt ;4=Shift+Alt ;5=Ctrl ;6=Shift+Ctrl ;7=Alt+Ctrl ;8=Shift+Alt+Ctrl
    fn mod_suffix(mods: KeyModifiers) -> &'static str {
        let shift = mods.contains(KeyModifiers::SHIFT);
        let alt = mods.contains(KeyModifiers::ALT);
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        match (shift, alt, ctrl) {
            (false, false, false) => "",
            (true, false, false) => ";2",
            (false, true, false) => ";3",
            (true, true, false) => ";4",
            (false, false, true) => ";5",
            (true, false, true) => ";6",
            (false, true, true) => ";7",
            (true, true, true) => ";8",
        }
    }

    // Helper to maybe prefix ESC for Alt/meta
    fn maybe_meta(mut bytes: Vec<u8>, mods: KeyModifiers) -> Vec<u8> {
        if mods.contains(KeyModifiers::ALT) {
            let mut v = Vec::with_capacity(1 + bytes.len());
            v.push(0x1B); // ESC
            v.extend(bytes);
            v
        } else {
            bytes
        }
    }

    // Ctrl mappings for printable keys
    fn ctrl_for_char(c: char) -> Option<u8> {
        match c {
            'a'..='z' => Some((c as u8 - b'a') + 1), // ^A..^Z = 1..26
            'A'..='Z' => Some((c as u8 - b'A') + 1),
            ' ' => Some(0), // ^@ / Ctrl-Space
            '@' => Some(0),
            '[' => Some(27),  // ESC
            '\\' => Some(28), // FS
            ']' => Some(29),  // GS
            '^' => Some(30),  // RS
            '_' => Some(31),  // US
            '?' => Some(127), // DEL
            _ => None,
        }
    }

    let mods = key.modifiers;

    match key.code {
        KeyCode::Char(c) => {
            // If Ctrl is down, try to map to a control byte
            if mods.contains(KeyModifiers::CONTROL) {
                if let Some(b) = ctrl_for_char(c) {
                    return Some(maybe_meta(vec![b], mods));
                }
            }
            // Otherwise emit the UTF-8 for the char, possibly Meta-prefixed
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            Some(maybe_meta(s.as_bytes().to_vec(), mods))
        }

        // Enter: CR (carriage return)
        KeyCode::Enter => Some(maybe_meta(vec![b'\r'], mods)),

        // Tab and BackTab
        KeyCode::Tab => {
            if mods.contains(KeyModifiers::SHIFT) {
                // Shift-Tab is CSI Z
                let seq = format!("\x1b[Z");
                Some(maybe_meta(seq.into_bytes(), mods - KeyModifiers::SHIFT))
            } else {
                Some(maybe_meta(vec![b'\t'], mods))
            }
        }

        // Backspace must be DEL (127) so line discipline erases
        KeyCode::Backspace => Some(maybe_meta(vec![127], mods)),

        // Escape
        KeyCode::Esc => Some(vec![0x1B]),

        // Cursor keys (normal or application mode)
        KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
            let letter = match key.code {
                KeyCode::Up => 'A',
                KeyCode::Down => 'B',
                KeyCode::Right => 'C',
                KeyCode::Left => 'D',
                _ => unreachable!(),
            };
            let seq = if app_cursor_mode {
                // Application cursor mode: ESC O <letter>
                format!(
                    "\x1bO{}{}",
                    if mods.contains(KeyModifiers::SHIFT) {
                        "1;2"
                    } else {
                        ""
                    },
                    letter
                )
            } else {
                // Normal cursor mode: CSI [ 1 <mods> <letter>
                format!("\x1b[1{}{}", mod_suffix(mods), letter)
            };
            Some(seq.into_bytes())
        }

        // Home/End
        KeyCode::Home | KeyCode::End => {
            let letter = match key.code {
                KeyCode::Home => 'H',
                KeyCode::End => 'F',
                _ => unreachable!(),
            };
            let seq = format!("\x1b[1{}{}", mod_suffix(mods), letter);
            Some(seq.into_bytes())
        }

        // Insert/Delete/PageUp/PageDown (tilde family)
        KeyCode::Insert | KeyCode::Delete | KeyCode::PageUp | KeyCode::PageDown => {
            let num = match key.code {
                KeyCode::Insert => 2,
                KeyCode::Delete => 3,
                KeyCode::PageUp => 5,
                KeyCode::PageDown => 6,
                _ => unreachable!(),
            };
            let seq = format!(
                "\x1b[{}{}~",
                num,
                if mod_suffix(mods).is_empty() {
                    ""
                } else {
                    &mod_suffix(mods)[1..]
                }
            );
            Some(seq.into_bytes())
        }

        // Function keys → CSI 11~..24~ (common xterm set)
        KeyCode::F(n) => {
            let base = match n {
                1 => 11,
                2 => 12,
                3 => 13,
                4 => 14,
                5 => 15,
                6 => 17,
                7 => 18,
                8 => 19,
                9 => 20,
                10 => 21,
                11 => 23,
                12 => 24,
                _ => return None, // extend if you need higher F-keys
            };
            let mut seq = format!("\x1b[{}", base);
            // tilde family gets modifiers as ;N~
            let ms = mod_suffix(mods);
            if ms.is_empty() {
                seq.push('~');
            } else {
                seq.push_str(&format!("{}~", &ms[1..]));
            }
            Some(seq.into_bytes())
        }

        _ => None,
    }
}
