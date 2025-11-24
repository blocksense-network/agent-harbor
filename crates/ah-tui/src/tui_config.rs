// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! TUI-specific configuration types

use serde::{Deserialize, Serialize};

/// TUI-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct TuiConfig {
    /// Terminal multiplexer choice
    pub terminal_multiplexer: Option<String>,
    /// Default editor command
    pub editor: Option<String>,
    /// TUI symbol style (unicode/nerdfont/ascii)
    pub tui_font_style: Option<String>,
    /// TUI font name for advanced terminal font customization
    pub tui_font: Option<String>,
    /// Number of activity rows for active task cards (defaults to 3)
    pub active_sessions_activity_rows: Option<usize>,
    /// Selection dialog style (modal/inline/default)
    pub selection_dialog_style: Option<String>,
    /// Enable workspace terms menu (autocomplete popup)
    pub workspace_terms_menu: Option<bool>,
    /// Keyboard shortcut mappings
    pub keymap: Option<TuiKeymapConfig>,
    /// UI theme selection
    pub theme: Option<String>,
    /// High contrast mode toggle
    pub high_contrast: Option<bool>,
    /// Activity lines count per card
    pub activity_lines_count: Option<usize>,
    /// Word wrap settings
    pub word_wrap: Option<bool>,
    /// Native vs normalized output mode
    pub native_output: Option<bool>,
    /// Default multiplexer selection (tmux/zellij/screen/auto)
    pub default_multiplexer: Option<String>,
    /// Autocomplete behavior settings
    pub autocomplete_behavior: Option<String>,
    /// Scroll behavior settings
    pub scroll_behavior: Option<String>,
    /// Mouse interaction preferences
    pub mouse_interaction: Option<bool>,
}

/// Keyboard keymap configuration with all TUI operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TuiKeymapConfig {
    /// Meta key for key bindings (alt/option)
    pub meta_key: Option<String>,

    /// Cursor movement operations
    pub move_to_beginning_of_line: Option<String>,
    pub move_to_end_of_line: Option<String>,
    pub move_forward_one_character: Option<String>,
    pub move_backward_one_character: Option<String>,
    pub move_to_next_line: Option<String>,
    pub move_to_previous_line: Option<String>,
    pub move_forward_one_word: Option<String>,
    pub move_backward_one_word: Option<String>,
    pub move_to_beginning_of_sentence: Option<String>,
    pub move_to_end_of_sentence: Option<String>,
    pub scroll_down_one_screen: Option<String>,
    pub scroll_up_one_screen: Option<String>,
    pub recenter_screen_on_cursor: Option<String>,
    pub move_to_beginning_of_document: Option<String>,
    pub move_to_end_of_document: Option<String>,
    pub move_to_beginning_of_paragraph: Option<String>,
    pub move_to_end_of_paragraph: Option<String>,
    pub go_to_line_number: Option<String>,
    pub move_to_matching_parenthesis: Option<String>,

    /// Editing and deletion operations
    pub delete_character_forward: Option<String>,
    pub delete_character_backward: Option<String>,
    pub delete_word_forward: Option<String>,
    pub delete_word_backward: Option<String>,
    pub delete_to_end_of_line: Option<String>,
    pub cut: Option<String>,
    pub copy: Option<String>,
    pub paste: Option<String>,
    pub cycle_through_clipboard: Option<String>,
    pub transpose_characters: Option<String>,
    pub transpose_words: Option<String>,
    pub undo: Option<String>,
    pub redo: Option<String>,
    pub open_new_line: Option<String>,
    pub indent_or_complete: Option<String>,
    pub move_to_next_field: Option<String>,
    pub move_to_previous_field: Option<String>,
    pub dismiss_overlay: Option<String>,
    pub increment_value: Option<String>,
    pub decrement_value: Option<String>,
    pub delete_to_beginning_of_line: Option<String>,
    pub toggle_insert_mode: Option<String>,

    /// Text transformation operations
    pub uppercase_word: Option<String>,
    pub lowercase_word: Option<String>,
    pub capitalize_word: Option<String>,
    pub justify_paragraph: Option<String>,
    pub join_lines: Option<String>,

    /// Formatting operations (Markdown style)
    pub bold: Option<String>,
    pub italic: Option<String>,
    pub underline: Option<String>,

    /// Code editing operations
    pub toggle_comment: Option<String>,
    pub duplicate_line_selection: Option<String>,
    pub move_line_up: Option<String>,
    pub move_line_down: Option<String>,
    pub indent_region: Option<String>,
    pub dedent_region: Option<String>,

    /// Search and replace operations
    pub incremental_search_forward: Option<String>,
    pub incremental_search_backward: Option<String>,
    pub find_and_replace: Option<String>,
    pub find_and_replace_with_regex: Option<String>,
    pub find_next: Option<String>,
    pub find_previous: Option<String>,

    /// Mark and region operations
    pub set_mark: Option<String>,
    pub select_all: Option<String>,
    pub select_word_under_cursor: Option<String>,
    pub extend_selection: Option<String>,

    /// Application actions
    pub draft_new_task: Option<String>,
    pub show_launch_options: Option<String>,
    pub launch_and_focus: Option<String>,
    pub launch_in_split_view: Option<String>,
    pub launch_in_split_view_and_focus: Option<String>,
    pub launch_in_horizontal_split: Option<String>,
    pub launch_in_vertical_split: Option<String>,
    pub activate_current_item: Option<String>,
    pub delete_current_task: Option<String>,

    /// Session viewer task entry operations
    pub move_to_next_snapshot: Option<String>,
    pub move_to_previous_snapshot: Option<String>,
}
