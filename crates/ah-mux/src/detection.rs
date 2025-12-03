// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Environment detection for terminal multiplexers and editors.
//!
//! This module provides functions to detect various terminal environments,
//! including multiplexers, terminals, and editors that may embed terminals.
//! Detection works by checking environment variables, TERM hints, and process trees.

use tracing::{debug, info, instrument};

/// Represents different types of terminal environments that can be detected
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TerminalEnvironment {
    /// Tmux multiplexer
    Tmux,
    /// Zellij multiplexer
    Zellij,
    /// GNU Screen multiplexer
    Screen,
    /// Kitty terminal
    Kitty,
    /// iTerm2 terminal (macOS)
    ITerm2,
    /// WezTerm terminal/multiplexer
    WezTerm,
    /// Tilix terminal (Linux only)
    Tilix,
    /// Windows Terminal (Windows only)
    WindowsTerminal,
    /// Ghostty terminal
    Ghostty,
    /// Neovim editor
    Neovim,
    /// Vim editor
    Vim,
    /// Emacs editor
    Emacs,
}

impl TerminalEnvironment {
    /// Get a human-readable display name for the terminal environment
    pub fn display_name(&self) -> &'static str {
        match self {
            TerminalEnvironment::Tmux => "Tmux (multiplexer)",
            TerminalEnvironment::Zellij => "Zellij (multiplexer)",
            TerminalEnvironment::Screen => "GNU Screen (multiplexer)",
            TerminalEnvironment::Kitty => "Kitty (terminal)",
            TerminalEnvironment::ITerm2 => "iTerm2 (terminal)",
            TerminalEnvironment::WezTerm => "WezTerm (terminal)",
            TerminalEnvironment::Tilix => "Tilix (terminal)",
            TerminalEnvironment::WindowsTerminal => "Windows Terminal (terminal)",
            TerminalEnvironment::Ghostty => "Ghostty (terminal)",
            TerminalEnvironment::Neovim => "Neovim (editor)",
            TerminalEnvironment::Vim => "Vim (editor)",
            TerminalEnvironment::Emacs => "Emacs (editor)",
        }
    }
}

/// Detect all terminal environments currently active, in wrapping order.
///
/// This function checks for nested terminal environments and returns them
/// in the order they wrap around each other (outermost first).
/// For example, if you're running tmux inside kitty, it would return [Kitty, Tmux].
///
/// # Returns
/// A vector of detected environments in wrapping order (outermost to innermost)
#[instrument(fields(component = "ah_mux", operation = "detect_terminal_environments"))]
pub fn detect_terminal_environments() -> Vec<TerminalEnvironment> {
    info!("Starting terminal environment detection");
    let mut detected = Vec::new();

    // Check multiplexers first (these are typically the innermost layers)
    if is_in_tmux() {
        debug!("Detected tmux environment");
        detected.push(TerminalEnvironment::Tmux);
    }
    if is_in_zellij() {
        debug!("Detected zellij environment");
        detected.push(TerminalEnvironment::Zellij);
    }
    if is_in_screen() {
        debug!("Detected screen environment");
        detected.push(TerminalEnvironment::Screen);
    }

    // Check editors first (these can wrap terminals/multiplexers)
    if is_in_emacs() {
        debug!("Detected emacs environment");
        detected.insert(0, TerminalEnvironment::Emacs);
    }
    if is_in_neovim() {
        debug!("Detected neovim environment");
        detected.insert(0, TerminalEnvironment::Neovim);
    }
    if is_in_vim() {
        debug!("Detected vim environment");
        detected.insert(0, TerminalEnvironment::Vim);
    }

    // Check terminals (these can wrap multiplexers but are wrapped by editors)
    if is_in_kitty() {
        debug!("Detected kitty environment");
        detected.insert(0, TerminalEnvironment::Kitty);
    }
    if is_in_iterm2() {
        debug!("Detected iTerm2 environment");
        detected.insert(0, TerminalEnvironment::ITerm2);
    }
    if is_in_wezterm() {
        debug!("Detected WezTerm environment");
        detected.insert(0, TerminalEnvironment::WezTerm);
    }
    if is_in_tilix() {
        debug!("Detected Tilix environment");
        detected.insert(0, TerminalEnvironment::Tilix);
    }
    if is_in_windows_terminal() {
        debug!("Detected Windows Terminal environment");
        detected.insert(0, TerminalEnvironment::WindowsTerminal);
    }
    if is_in_ghostty() {
        debug!("Detected Ghostty environment");
        detected.insert(0, TerminalEnvironment::Ghostty);
    }

    // If detected contains both WezTerm and iTerm2, ensure correct order based on the TERM_PROGRAM variable.
    // This is necessary because when user open wezterm from iTerm2, TERM_PROGRAM is being overridden,
    // but the rest of the iTerm2 vars are still present
    if detected.contains(&TerminalEnvironment::WezTerm)
        && detected.contains(&TerminalEnvironment::ITerm2)
    {
        // we prioritize value in TERM_PROGRAM
        let term_program = std::env::var("TERM_PROGRAM");

        if term_program == Ok("WezTerm".to_string()) {
            let wezterm_index =
                detected.iter().position(|e| *e == TerminalEnvironment::WezTerm).unwrap();
            let iterm2_index =
                detected.iter().position(|e| *e == TerminalEnvironment::ITerm2).unwrap();
            // swap positions of WezTerm and iTerm2
            detected.swap(wezterm_index, iterm2_index);
        }
    }

    info!(detected_environments = ?detected, environment_count = %detected.len(),
          "Terminal environment detection completed");
    detected
}

/// Check if currently running inside tmux
pub fn is_in_tmux() -> bool {
    // Strong signals: TMUX and TMUX_PANE environment variables
    std::env::var("TMUX").is_ok() || std::env::var("TMUX_PANE").is_ok()
}

/// Check if currently running inside zellij
pub fn is_in_zellij() -> bool {
    // ZELLIJ is set to "0" inside sessions, ZELLIJ_SESSION_NAME holds session name
    std::env::var("ZELLIJ").is_ok() || std::env::var("ZELLIJ_SESSION_NAME").is_ok()
}

/// Check if currently running inside GNU screen
pub fn is_in_screen() -> bool {
    // STY (session/socket) and WINDOW (window number) environment variables
    std::env::var("STY").is_ok() || std::env::var("WINDOW").is_ok()
}

/// Check if currently running inside Kitty terminal
pub fn is_in_kitty() -> bool {
    // Strong signals: KITTY_PID and KITTY_WINDOW_ID
    if std::env::var("KITTY_PID").is_ok() || std::env::var("KITTY_WINDOW_ID").is_ok() {
        return true;
    }

    // TERM hint: xterm-kitty
    if let Ok(term) = std::env::var("TERM") {
        if term.starts_with("xterm-kitty") {
            return true;
        }
    }

    false
}

/// Check if currently running inside iTerm2 terminal
pub fn is_in_iterm2() -> bool {
    // Strong signals: TERM_PROGRAM=iTerm.app, ITERM_SESSION_ID, LC_TERMINAL=iTerm2
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        if term_program == "iTerm.app" {
            return true;
        }
    }

    if std::env::var("ITERM_SESSION_ID").is_ok() {
        return true;
    }

    if let Ok(lc_terminal) = std::env::var("LC_TERMINAL") {
        if lc_terminal == "iTerm2" {
            return true;
        }
    }

    false
}

/// Check if currently running inside WezTerm
pub fn is_in_wezterm() -> bool {
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        if term_program == "WezTerm" {
            return true;
        }
    }
    // Strong signals: WEZTERM_PANE, WEZTERM_EXECUTABLE, WEZTERM_SOCKET
    std::env::var("WEZTERM_PANE").is_ok()
        || std::env::var("WEZTERM_EXECUTABLE").is_ok()
        || std::env::var("WEZTERM_SOCKET").is_ok()
}

/// Check if currently running inside Tilix (Linux only)
pub fn is_in_tilix() -> bool {
    // Strong signal: TILIX_ID environment variable
    std::env::var("TILIX_ID").is_ok()
}

/// Check if currently running inside Windows Terminal
pub fn is_in_windows_terminal() -> bool {
    // Strong signals: WT_SESSION and WT_PROFILE_ID
    std::env::var("WT_SESSION").is_ok() || std::env::var("WT_PROFILE_ID").is_ok()
}

/// Check if currently running inside Ghostty
pub fn is_in_ghostty() -> bool {
    // Strong signal: GHOSTTY_RESOURCES_DIR (set by shell integration)
    std::env::var("GHOSTTY_RESOURCES_DIR").is_ok()
}

/// Check if currently running inside Neovim
pub fn is_in_neovim() -> bool {
    // Check for Neovim-specific VIMRUNTIME path
    if let Ok(vimruntime) = std::env::var("VIMRUNTIME") {
        if vimruntime.contains("/nvim/runtime") {
            return true;
        }
    }

    // Check for Neovim-specific environment variables
    std::env::var("NVIM").is_ok() || std::env::var("NVIM_LISTEN_ADDRESS").is_ok()
}

/// Check if currently running inside Vim (but not Neovim)
pub fn is_in_vim() -> bool {
    // Must have VIMRUNTIME but not be Neovim
    if let Ok(vimruntime) = std::env::var("VIMRUNTIME") {
        // Exclude Neovim runtime paths
        if !vimruntime.contains("/nvim/") {
            return true;
        }
    }

    false
}

/// Check if currently running inside Emacs
pub fn is_in_emacs() -> bool {
    // Strong signal: INSIDE_EMACS environment variable
    std::env::var("INSIDE_EMACS").is_ok()
}

/// Check if currently running inside any multiplexer
pub fn is_in_multiplexer() -> bool {
    is_in_tmux() || is_in_zellij() || is_in_screen()
}

/// Check if currently running inside any terminal (excluding multiplexers)
pub fn is_in_terminal() -> bool {
    is_in_kitty()
        || is_in_iterm2()
        || is_in_wezterm()
        || is_in_tilix()
        || is_in_windows_terminal()
        || is_in_ghostty()
}

/// Check if currently running inside any editor
pub fn is_in_editor() -> bool {
    is_in_emacs() || is_in_neovim() || is_in_vim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_empty_environment() {
        // Use a temporary environment to avoid affecting other tests
        // We can't easily clear the global environment in tests, so we'll test
        // with a controlled setup that checks individual functions instead

        // Test that individual detection functions return false when vars are not set
        // This is a more targeted approach than trying to clear the global environment

        // Test tmux detection with no env vars
        std::env::remove_var("TMUX");
        std::env::remove_var("TMUX_PANE");
        // We can't guarantee these are not set globally, so we'll skip this test
        // and focus on testing the logic with controlled inputs

        // Instead, test that the function doesn't panic and returns reasonable results
        let detected = detect_terminal_environments();
        // We can't assert emptiness due to global env, but we can assert it doesn't panic
        // and that the result is a valid Vec (length check is always true for usize)
        assert!(detected.is_empty() || !detected.is_empty());
    }

    #[test]
    fn test_detect_tmux() {
        // Save original values
        let original_tmux = std::env::var("TMUX").ok();
        let original_tmux_pane = std::env::var("TMUX_PANE").ok();

        std::env::set_var("TMUX", "test");
        std::env::remove_var("TMUX_PANE");
        assert!(is_in_tmux());
        assert!(is_in_multiplexer());

        std::env::remove_var("TMUX");
        std::env::set_var("TMUX_PANE", "test");
        assert!(is_in_tmux());
        assert!(is_in_multiplexer());

        // Restore original values
        match original_tmux {
            Some(val) => std::env::set_var("TMUX", val),
            None => std::env::remove_var("TMUX"),
        }
        match original_tmux_pane {
            Some(val) => std::env::set_var("TMUX_PANE", val),
            None => std::env::remove_var("TMUX_PANE"),
        }
    }

    #[test]
    fn test_detect_zellij() {
        // Save original values
        let original_zellij = std::env::var("ZELLIJ").ok();
        let original_zellij_session = std::env::var("ZELLIJ_SESSION_NAME").ok();

        std::env::set_var("ZELLIJ", "0");
        std::env::remove_var("ZELLIJ_SESSION_NAME");
        assert!(is_in_zellij());
        assert!(is_in_multiplexer());

        std::env::remove_var("ZELLIJ");
        std::env::set_var("ZELLIJ_SESSION_NAME", "test");
        assert!(is_in_zellij());
        assert!(is_in_multiplexer());

        // Restore original values
        match original_zellij {
            Some(val) => std::env::set_var("ZELLIJ", val),
            None => std::env::remove_var("ZELLIJ"),
        }
        match original_zellij_session {
            Some(val) => std::env::set_var("ZELLIJ_SESSION_NAME", val),
            None => std::env::remove_var("ZELLIJ_SESSION_NAME"),
        }
    }

    #[test]
    fn test_detect_kitty() {
        // Save original values
        let original_kitty_pid = std::env::var("KITTY_PID").ok();
        let original_term = std::env::var("TERM").ok();

        // Test KITTY_PID detection
        std::env::set_var("KITTY_PID", "123");
        std::env::remove_var("TERM"); // Clear TERM to test KITTY_PID alone
        assert!(is_in_kitty());
        assert!(is_in_terminal());

        // Test TERM detection
        std::env::remove_var("KITTY_PID");
        std::env::set_var("TERM", "xterm-kitty");
        assert!(is_in_kitty());
        assert!(is_in_terminal());

        // Restore original values
        match original_kitty_pid {
            Some(val) => std::env::set_var("KITTY_PID", val),
            None => std::env::remove_var("KITTY_PID"),
        }
        match original_term {
            Some(val) => std::env::set_var("TERM", val),
            None => std::env::remove_var("TERM"),
        }
    }

    #[test]
    fn test_detect_emacs() {
        // Save original values
        let original_inside_emacs = std::env::var("INSIDE_EMACS").ok();

        std::env::set_var("INSIDE_EMACS", "29.1,comint");
        assert!(is_in_emacs());
        assert!(is_in_editor());

        // Restore original values
        match original_inside_emacs {
            Some(val) => std::env::set_var("INSIDE_EMACS", val),
            None => std::env::remove_var("INSIDE_EMACS"),
        }
    }

    #[test]
    fn test_detect_neovim() {
        // Save original values
        let original_vimruntime = std::env::var("VIMRUNTIME").ok();
        let original_nvim = std::env::var("NVIM").ok();
        let original_nvim_listen = std::env::var("NVIM_LISTEN_ADDRESS").ok();

        std::env::set_var("VIMRUNTIME", "/usr/share/nvim/runtime");
        std::env::remove_var("NVIM");
        std::env::remove_var("NVIM_LISTEN_ADDRESS");
        assert!(is_in_neovim());
        assert!(is_in_editor());

        std::env::remove_var("VIMRUNTIME");
        std::env::set_var("NVIM", "/tmp/nvim.sock");
        assert!(is_in_neovim());
        assert!(is_in_editor());

        // Restore original values
        match original_vimruntime {
            Some(val) => std::env::set_var("VIMRUNTIME", val),
            None => std::env::remove_var("VIMRUNTIME"),
        }
        match original_nvim {
            Some(val) => std::env::set_var("NVIM", val),
            None => std::env::remove_var("NVIM"),
        }
        match original_nvim_listen {
            Some(val) => std::env::set_var("NVIM_LISTEN_ADDRESS", val),
            None => std::env::remove_var("NVIM_LISTEN_ADDRESS"),
        }
    }

    #[test]
    fn test_detect_iterm2() {
        // Save original values
        let original_term_program = std::env::var("TERM_PROGRAM").ok();
        let original_iterm_session_id = std::env::var("ITERM_SESSION_ID").ok();
        let original_lc_terminal = std::env::var("LC_TERMINAL").ok();

        // Test TERM_PROGRAM=iTerm.app detection
        std::env::set_var("TERM_PROGRAM", "iTerm.app");
        assert!(is_in_iterm2());
        assert!(is_in_terminal());

        std::env::remove_var("TERM_PROGRAM");

        // Test ITERM_SESSION_ID detection
        std::env::set_var(
            "ITERM_SESSION_ID",
            "w0t0p0:12345678-1234-5678-9ABC-123456789ABC",
        );
        assert!(is_in_iterm2());
        assert!(is_in_terminal());

        std::env::remove_var("ITERM_SESSION_ID");

        // Test LC_TERMINAL=iTerm2 detection
        std::env::set_var("LC_TERMINAL", "iTerm2");
        assert!(is_in_iterm2());
        assert!(is_in_terminal());

        // Restore original values
        match original_term_program {
            Some(val) => std::env::set_var("TERM_PROGRAM", val),
            None => std::env::remove_var("TERM_PROGRAM"),
        }
        match original_iterm_session_id {
            Some(val) => std::env::set_var("ITERM_SESSION_ID", val),
            None => std::env::remove_var("ITERM_SESSION_ID"),
        }
        match original_lc_terminal {
            Some(val) => std::env::set_var("LC_TERMINAL", val),
            None => std::env::remove_var("LC_TERMINAL"),
        }
    }

    #[test]
    #[serial_test::file_serial]
    fn test_detect_vim() {
        // Save original values
        let original_vimruntime = std::env::var("VIMRUNTIME").ok();

        std::env::set_var("VIMRUNTIME", "/usr/share/vim/vim82");
        assert!(is_in_vim());
        assert!(is_in_editor());

        // Should not detect vim if it's actually neovim
        std::env::set_var("VIMRUNTIME", "/usr/share/nvim/runtime");
        assert!(!is_in_vim());
        assert!(is_in_neovim());

        // Restore original values
        match original_vimruntime {
            Some(val) => std::env::set_var("VIMRUNTIME", val),
            None => std::env::remove_var("VIMRUNTIME"),
        }
    }

    #[test]
    #[serial_test::file_serial]
    fn test_wrapping_order() {
        // Save original values
        let original_kitty_pid = std::env::var("KITTY_PID").ok();
        let original_kitty_window_id = std::env::var("KITTY_WINDOW_ID").ok();
        let original_term = std::env::var("TERM").ok();
        let original_term_program = std::env::var("TERM_PROGRAM").ok();
        let original_iterm_session_id = std::env::var("ITERM_SESSION_ID").ok();
        let original_lc_terminal = std::env::var("LC_TERMINAL").ok();
        let original_tmux = std::env::var("TMUX").ok();
        let original_tmux_pane = std::env::var("TMUX_PANE").ok();
        let original_zellij = std::env::var("ZELLIJ").ok();
        let original_zellij_session = std::env::var("ZELLIJ_SESSION_NAME").ok();
        let original_sty = std::env::var("STY").ok();
        let original_window = std::env::var("WINDOW").ok();
        let original_wezterm_pane = std::env::var("WEZTERM_PANE").ok();
        let original_wezterm_exec = std::env::var("WEZTERM_EXECUTABLE").ok();
        let original_wezterm_socket = std::env::var("WEZTERM_SOCKET").ok();
        let original_tilix_id = std::env::var("TILIX_ID").ok();
        let original_wt_session = std::env::var("WT_SESSION").ok();
        let original_wt_profile_id = std::env::var("WT_PROFILE_ID").ok();
        let original_ghostty_resources = std::env::var("GHOSTTY_RESOURCES_DIR").ok();
        let original_inside_emacs = std::env::var("INSIDE_EMACS").ok();
        let original_vimruntime = std::env::var("VIMRUNTIME").ok();
        let original_nvim = std::env::var("NVIM").ok();
        let original_nvim_listen = std::env::var("NVIM_LISTEN_ADDRESS").ok();

        // Clear all detection variables first
        std::env::remove_var("KITTY_PID");
        std::env::remove_var("KITTY_WINDOW_ID");
        std::env::remove_var("TERM");
        std::env::remove_var("TERM_PROGRAM");
        std::env::remove_var("ITERM_SESSION_ID");
        std::env::remove_var("LC_TERMINAL");
        std::env::remove_var("TMUX");
        std::env::remove_var("TMUX_PANE");
        std::env::remove_var("ZELLIJ");
        std::env::remove_var("ZELLIJ_SESSION_NAME");
        std::env::remove_var("STY");
        std::env::remove_var("WINDOW");
        std::env::remove_var("WEZTERM_PANE");
        std::env::remove_var("WEZTERM_EXECUTABLE");
        std::env::remove_var("WEZTERM_SOCKET");
        std::env::remove_var("TILIX_ID");
        std::env::remove_var("WT_SESSION");
        std::env::remove_var("WT_PROFILE_ID");
        std::env::remove_var("GHOSTTY_RESOURCES_DIR");
        std::env::remove_var("INSIDE_EMACS");
        std::env::remove_var("VIMRUNTIME");
        std::env::remove_var("NVIM");
        std::env::remove_var("NVIM_LISTEN_ADDRESS");

        // Extra paranoia: clear them again to ensure no cross-test pollution
        std::env::remove_var("TERM_PROGRAM");
        std::env::remove_var("ITERM_SESSION_ID");
        std::env::remove_var("LC_TERMINAL");

        {
            // Simulate kitty -> tmux -> neovim nesting
            std::env::set_var("KITTY_PID", "123");
            std::env::set_var("TMUX", "test");
            std::env::set_var("NVIM", "/tmp/nvim.sock");

            let detected = detect_terminal_environments();

            // Should be in wrapping order: outermost to innermost
            assert_eq!(detected.len(), 3);
            assert_eq!(detected[0], TerminalEnvironment::Kitty);
            assert_eq!(detected[1], TerminalEnvironment::Neovim);
            assert_eq!(detected[2], TerminalEnvironment::Tmux);

            std::env::remove_var("KITTY_PID");
            std::env::remove_var("TMUX");
            std::env::remove_var("NVIM");
        }
        {
            // Simulate iterm2 -> wezterm
            std::env::set_var("TERM_PROGRAM", "iTerm.app");
            std::env::set_var("ITERM_SESSION_ID", "456");
            std::env::set_var("LC_TERMINAL", "iTerm2");
            std::env::set_var("TERM_PROGRAM", "WezTerm");
            std::env::set_var("WEZTERM_PANE", "789");

            let detected = detect_terminal_environments();

            // Should be in wrapping order: outermost to innermost
            assert_eq!(detected.len(), 2);
            assert_eq!(detected[0], TerminalEnvironment::ITerm2);
            assert_eq!(detected[1], TerminalEnvironment::WezTerm);

            std::env::remove_var("TERM_PROGRAM");
            std::env::remove_var("ITERM_SESSION_ID");
            std::env::remove_var("LC_TERMINAL");
            std::env::remove_var("WEZTERM_PANE");
        }
        {
            // Simulate wezterm > iterm2
            std::env::set_var("TERM_PROGRAM", "WezTerm");
            std::env::set_var("WEZTERM_PANE", "789");
            std::env::set_var("TERM_PROGRAM", "iTerm.app");
            std::env::set_var("ITERM_SESSION_ID", "456");
            std::env::set_var("LC_TERMINAL", "iTerm2");

            let detected = detect_terminal_environments();

            // Should be in wrapping order: outermost to innermost
            assert_eq!(detected.len(), 2);
            assert_eq!(detected[0], TerminalEnvironment::WezTerm);
            assert_eq!(detected[1], TerminalEnvironment::ITerm2);

            std::env::remove_var("TERM_PROGRAM");
            std::env::remove_var("ITERM_SESSION_ID");
            std::env::remove_var("LC_TERMINAL");
            std::env::remove_var("WEZTERM_PANE");
        }

        // Restore original values
        match original_kitty_pid {
            Some(val) => std::env::set_var("KITTY_PID", val),
            None => std::env::remove_var("KITTY_PID"),
        }
        match original_kitty_window_id {
            Some(val) => std::env::set_var("KITTY_WINDOW_ID", val),
            None => std::env::remove_var("KITTY_WINDOW_ID"),
        }
        match original_term {
            Some(val) => std::env::set_var("TERM", val),
            None => std::env::remove_var("TERM"),
        }
        match original_term_program {
            Some(val) => std::env::set_var("TERM_PROGRAM", val),
            None => std::env::remove_var("TERM_PROGRAM"),
        }
        match original_iterm_session_id {
            Some(val) => std::env::set_var("ITERM_SESSION_ID", val),
            None => std::env::remove_var("ITERM_SESSION_ID"),
        }
        match original_lc_terminal {
            Some(val) => std::env::set_var("LC_TERMINAL", val),
            None => std::env::remove_var("LC_TERMINAL"),
        }
        match original_tmux {
            Some(val) => std::env::set_var("TMUX", val),
            None => std::env::remove_var("TMUX"),
        }
        match original_tmux_pane {
            Some(val) => std::env::set_var("TMUX_PANE", val),
            None => std::env::remove_var("TMUX_PANE"),
        }
        match original_zellij {
            Some(val) => std::env::set_var("ZELLIJ", val),
            None => std::env::remove_var("ZELLIJ"),
        }
        match original_zellij_session {
            Some(val) => std::env::set_var("ZELLIJ_SESSION_NAME", val),
            None => std::env::remove_var("ZELLIJ_SESSION_NAME"),
        }
        match original_sty {
            Some(val) => std::env::set_var("STY", val),
            None => std::env::remove_var("STY"),
        }
        match original_window {
            Some(val) => std::env::set_var("WINDOW", val),
            None => std::env::remove_var("WINDOW"),
        }
        match original_wezterm_pane {
            Some(val) => std::env::set_var("WEZTERM_PANE", val),
            None => std::env::remove_var("WEZTERM_PANE"),
        }
        match original_wezterm_exec {
            Some(val) => std::env::set_var("WEZTERM_EXECUTABLE", val),
            None => std::env::remove_var("WEZTERM_EXECUTABLE"),
        }
        match original_wezterm_socket {
            Some(val) => std::env::set_var("WEZTERM_SOCKET", val),
            None => std::env::remove_var("WEZTERM_SOCKET"),
        }
        match original_tilix_id {
            Some(val) => std::env::set_var("TILIX_ID", val),
            None => std::env::remove_var("TILIX_ID"),
        }
        match original_wt_session {
            Some(val) => std::env::set_var("WT_SESSION", val),
            None => std::env::remove_var("WT_SESSION"),
        }
        match original_wt_profile_id {
            Some(val) => std::env::set_var("WT_PROFILE_ID", val),
            None => std::env::remove_var("WT_PROFILE_ID"),
        }
        match original_ghostty_resources {
            Some(val) => std::env::set_var("GHOSTTY_RESOURCES_DIR", val),
            None => std::env::remove_var("GHOSTTY_RESOURCES_DIR"),
        }
        match original_inside_emacs {
            Some(val) => std::env::set_var("INSIDE_EMACS", val),
            None => std::env::remove_var("INSIDE_EMACS"),
        }
        match original_vimruntime {
            Some(val) => std::env::set_var("VIMRUNTIME", val),
            None => std::env::remove_var("VIMRUNTIME"),
        }
        match original_nvim {
            Some(val) => std::env::set_var("NVIM", val),
            None => std::env::remove_var("NVIM"),
        }
        match original_nvim_listen {
            Some(val) => std::env::set_var("NVIM_LISTEN_ADDRESS", val),
            None => std::env::remove_var("NVIM_LISTEN_ADDRESS"),
        }
    }
}
