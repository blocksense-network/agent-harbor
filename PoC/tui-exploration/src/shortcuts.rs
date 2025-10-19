use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Default shortcut definitions used by the demo application.
pub struct ShortcutDefinition {
    pub key: &'static str,
    pub description: &'static str,
    pub pc: &'static [&'static str],
    pub mac: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortcutPlatform {
    Pc,
    Mac,
}

impl ShortcutPlatform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            ShortcutPlatform::Mac
        } else {
            ShortcutPlatform::Pc
        }
    }
}

#[derive(Debug, Clone)]
pub struct KeyMatcher {
    code: KeyCode,
    required: KeyModifiers,
    optional: KeyModifiers,
    char_lower: Option<char>,
}

impl KeyMatcher {
    pub fn matches(&self, event: &KeyEvent) -> bool {
        if !self.matches_code(&event.code) {
            return false;
        }

        for modifier in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::SHIFT,
            KeyModifiers::SUPER,
        ] {
            let required = self.required.contains(modifier);
            let optional = self.optional.contains(modifier);
            let present = event.modifiers.contains(modifier);
            if required && !present {
                return false;
            }
            if !required && !optional && present {
                return false;
            }
        }

        true
    }

    fn matches_code(&self, code: &KeyCode) -> bool {
        match (&self.code, code) {
            (KeyCode::Char(expected), KeyCode::Char(actual)) => {
                if let Some(lower) = self.char_lower {
                    actual.to_ascii_lowercase() == lower
                } else {
                    actual == expected
                }
            }
            _ => self.code == *code,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShortcutParseError {
    #[error("shortcut must contain a key code, e.g. 'Enter' or 'Ctrl+Enter'")]
    MissingKey,
    #[error("unsupported modifier '{0}'")]
    UnsupportedModifier(String),
    #[error("unsupported key token '{0}'")]
    UnsupportedKey(String),
}

#[derive(Clone, Debug)]
pub struct ShortcutEntry {
    pub values: Vec<String>,
    pub matchers: Vec<KeyMatcher>,
}

impl ShortcutEntry {
    fn new(items: Vec<(String, KeyMatcher)>) -> Self {
        let (values, matchers): (Vec<_>, Vec<_>) = items.into_iter().unzip();
        Self { values, matchers }
    }
}

pub trait ShortcutConfigProvider {
    fn binding_strings(&self, key: &str) -> Option<Vec<String>>;
    fn set_binding_from_text(&mut self, key: &str, value: &str) -> Result<(), ShortcutParseError>;
    fn matches(&self, key: &str, event: &KeyEvent) -> bool;
    fn all_bindings(&self) -> Vec<ShortcutDisplay>;
}

#[derive(Clone, Debug)]
pub struct ShortcutDisplay {
    pub key: &'static str,
    pub description: &'static str,
    pub bindings: Vec<String>,
}

#[derive(Debug)]
pub struct InMemoryShortcutConfig {
    defaults: HashMap<String, ShortcutEntry>,
    overrides: HashMap<String, ShortcutEntry>,
}

impl InMemoryShortcutConfig {
    pub fn new() -> Self {
        let platform = ShortcutPlatform::current();
        let mut defaults = HashMap::new();
        for def in SHORTCUT_DEFINITIONS.iter() {
            let values = match platform {
                ShortcutPlatform::Pc => def.pc,
                ShortcutPlatform::Mac => def.mac,
            };
            let entry = parse_binding_list(values).unwrap_or_else(|err| {
                panic!("Failed to parse default shortcut for '{}': {err}", def.key)
            });
            defaults.insert(def.key.to_string(), entry);
        }

        Self {
            defaults,
            overrides: HashMap::new(),
        }
    }

    fn entry(&self, key: &str) -> Option<&ShortcutEntry> {
        self.overrides.get(key).or_else(|| self.defaults.get(key))
    }

    fn entry_mut(&mut self, key: &str) -> Option<&mut ShortcutEntry> {
        if self.overrides.contains_key(key) {
            self.overrides.get_mut(key)
        } else if self.defaults.contains_key(key) {
            None
        } else {
            None
        }
    }
}

impl ShortcutConfigProvider for InMemoryShortcutConfig {
    fn binding_strings(&self, key: &str) -> Option<Vec<String>> {
        self.entry(key).map(|entry| entry.values.clone())
    }

    fn set_binding_from_text(&mut self, key: &str, value: &str) -> Result<(), ShortcutParseError> {
        let parsed = parse_binding_text(value)?;
        let entry = ShortcutEntry::new(parsed);
        if entry.values.is_empty() {
            self.overrides.remove(key);
            return Ok(());
        }
        if let Some(default) = self.defaults.get(key) {
            if default.values == entry.values {
                self.overrides.remove(key);
                return Ok(());
            }
        }
        self.overrides.insert(key.to_string(), entry);
        Ok(())
    }

    fn matches(&self, key: &str, event: &KeyEvent) -> bool {
        self.entry(key)
            .map(|entry| entry.matchers.iter().any(|m| m.matches(event)))
            .unwrap_or(false)
    }

    fn all_bindings(&self) -> Vec<ShortcutDisplay> {
        SHORTCUT_DEFINITIONS
            .iter()
            .map(|def| ShortcutDisplay {
                key: def.key,
                description: def.description,
                bindings: self.binding_strings(def.key).unwrap_or_else(Vec::new),
            })
            .collect()
    }
}

fn parse_binding_text(value: &str) -> Result<Vec<(String, KeyMatcher)>, ShortcutParseError> {
    let cleaned = value.trim();
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    let chunks: Vec<&str> = if cleaned.contains('|') {
        cleaned.split('|').collect()
    } else {
        cleaned.split(',').collect()
    };
    let parsed: Result<Vec<_>, _> =
        chunks.into_iter().map(|chunk| parse_binding_sequence(chunk.trim())).collect();
    parsed
}

fn parse_binding_list(values: &[&str]) -> Result<ShortcutEntry, ShortcutParseError> {
    let mut items = Vec::new();
    for value in values {
        if value.trim().is_empty() {
            continue;
        }
        items.append(&mut parse_binding_text(value)?);
    }
    Ok(ShortcutEntry::new(items))
}

fn parse_binding_sequence(value: &str) -> Result<(String, KeyMatcher), ShortcutParseError> {
    let mut modifiers = KeyModifiers::empty();
    let mut optional = KeyModifiers::empty();
    let mut key_code: Option<KeyCode> = None;
    let mut char_lower: Option<char> = None;
    let mut modifier_tokens: Vec<String> = Vec::new();

    for token in value.split('+').map(|t| t.trim()).filter(|t| !t.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => {
                modifiers |= KeyModifiers::CONTROL;
                modifier_tokens.push("Ctrl".to_string());
            }
            "alt" | "option" => {
                modifiers |= KeyModifiers::ALT;
                modifier_tokens.push("Alt".to_string());
            }
            "shift" => {
                modifiers |= KeyModifiers::SHIFT;
                modifier_tokens.push("Shift".to_string());
            }
            "cmd" | "command" | "meta" | "super" => {
                modifiers |= KeyModifiers::SUPER;
                modifier_tokens.push("Cmd".to_string());
            }
            other => {
                if key_code.is_some() {
                    return Err(ShortcutParseError::UnsupportedKey(other.to_string()));
                }
                let (code, lower, implies_shift, display) = parse_key_token(other)?;
                key_code = Some(code);
                char_lower = lower;
                if implies_shift && !modifiers.contains(KeyModifiers::SHIFT) {
                    modifiers |= KeyModifiers::SHIFT;
                }
                if matches!(code, KeyCode::Char(_)) && !implies_shift {
                    optional |= KeyModifiers::SHIFT;
                }
                modifier_tokens.push(display.clone());
            }
        }
    }

    let code = key_code.ok_or(ShortcutParseError::MissingKey)?;
    let matcher = KeyMatcher {
        code,
        required: modifiers,
        optional,
        char_lower,
    };

    let repr = modifier_tokens.join("+");
    Ok((repr, matcher))
}

fn parse_key_token(
    token: &str,
) -> Result<(KeyCode, Option<char>, bool, String), ShortcutParseError> {
    let lower = token.to_ascii_lowercase();
    let mut lower_char: Option<char> = None;
    let (code, implies_shift, display) = match lower.as_str() {
        "enter" | "return" => (KeyCode::Enter, false, "Enter".to_string()),
        "tab" => (KeyCode::Tab, false, "Tab".to_string()),
        "esc" | "escape" => (KeyCode::Esc, false, "Esc".to_string()),
        "space" => (KeyCode::Char(' '), false, "Space".to_string()),
        "backspace" => (KeyCode::Backspace, false, "Backspace".to_string()),
        "delete" | "del" => (KeyCode::Delete, false, "Delete".to_string()),
        "up" => (KeyCode::Up, false, "↑".to_string()),
        "down" => (KeyCode::Down, false, "↓".to_string()),
        "left" => (KeyCode::Left, false, "←".to_string()),
        "right" => (KeyCode::Right, false, "→".to_string()),
        "home" => (KeyCode::Home, false, "Home".to_string()),
        "end" => (KeyCode::End, false, "End".to_string()),
        "pageup" | "page-up" => (KeyCode::PageUp, false, "PageUp".to_string()),
        "pagedown" | "page-down" => (KeyCode::PageDown, false, "PageDown".to_string()),
        token if token.len() > 1 && token.to_lowercase().starts_with('f') => {
            // Handle function keys F1-F12
            if let Ok(num) = token[1..].parse::<u8>() {
                if (1..=12).contains(&num) {
                    (KeyCode::F(num), false, format!("F{}", num))
                } else {
                    return Err(ShortcutParseError::UnsupportedKey(token.to_string()));
                }
            } else {
                return Err(ShortcutParseError::UnsupportedKey(token.to_string()));
            }
        }
        _ => {
            let mut chars = token.chars();
            let first = chars
                .next()
                .ok_or_else(|| ShortcutParseError::UnsupportedKey(token.to_string()))?;
            if chars.next().is_some() {
                return Err(ShortcutParseError::UnsupportedKey(token.to_string()));
            }
            let implies_shift = requires_shift_for_char(first);
            let display = if first.is_ascii_alphabetic() {
                first.to_ascii_uppercase().to_string()
            } else {
                first.to_string()
            };
            if first.is_ascii_alphabetic() {
                lower_char = Some(first.to_ascii_lowercase());
            }
            (KeyCode::Char(first), implies_shift, display)
        }
    };

    Ok((code, lower_char, implies_shift, display))
}

fn requires_shift_for_char(c: char) -> bool {
    matches!(
        c,
        '!' | '@'
            | '#'
            | '$'
            | '%'
            | '^'
            | '&'
            | '*'
            | '('
            | ')'
            | '_'
            | '+'
            | '{'
            | '}'
            | '|'
            | ':'
            | '"'
            | '<'
            | '>'
            | '?'
            | '~'
    )
}

pub const SHORTCUT_LAUNCH_TASK: &str = "launch-task";
pub const SHORTCUT_NEW_LINE: &str = "insert-new-line";
pub const SHORTCUT_NEXT_FIELD: &str = "focus-next-field";
pub const SHORTCUT_PREV_FIELD: &str = "focus-previous-field";
pub const SHORTCUT_SHORTCUT_HELP: &str = "shortcut-help";
pub const SHORTCUT_NAVIGATE_CARDS: &str = "navigate-cards";
pub const SHORTCUT_OPEN_SETTINGS: &str = "open-settings";
pub const SHORTCUT_STOP_TASK: &str = "stop-task";
pub const SHORTCUT_QUIT: &str = "quit";

pub const SHORTCUT_MOVE_TO_BEGINNING_OF_LINE: &str = "move-to-beginning-of-line";
pub const SHORTCUT_MOVE_TO_END_OF_LINE: &str = "move-to-end-of-line";
pub const SHORTCUT_MOVE_FORWARD_ONE_CHARACTER: &str = "move-forward-one-character";
pub const SHORTCUT_MOVE_BACKWARD_ONE_CHARACTER: &str = "move-backward-one-character";
pub const SHORTCUT_MOVE_TO_NEXT_LINE: &str = "move-to-next-line";
pub const SHORTCUT_MOVE_TO_PREVIOUS_LINE: &str = "move-to-previous-line";
pub const SHORTCUT_MOVE_FORWARD_ONE_WORD: &str = "move-forward-one-word";
pub const SHORTCUT_MOVE_BACKWARD_ONE_WORD: &str = "move-backward-one-word";
pub const SHORTCUT_MOVE_TO_BEGINNING_OF_SENTENCE: &str = "move-to-beginning-of-sentence";
pub const SHORTCUT_MOVE_TO_END_OF_SENTENCE: &str = "move-to-end-of-sentence";
pub const SHORTCUT_SCROLL_DOWN_ONE_SCREEN: &str = "scroll-down-one-screen";
pub const SHORTCUT_SCROLL_UP_ONE_SCREEN: &str = "scroll-up-one-screen";
pub const SHORTCUT_RECENTER_SCREEN: &str = "recenter-screen-on-cursor";
pub const SHORTCUT_MOVE_TO_BEGINNING_OF_DOCUMENT: &str = "move-to-beginning-of-document";
pub const SHORTCUT_MOVE_TO_END_OF_DOCUMENT: &str = "move-to-end-of-document";
pub const SHORTCUT_MOVE_TO_BEGINNING_OF_PARAGRAPH: &str = "move-to-beginning-of-paragraph";
pub const SHORTCUT_MOVE_TO_END_OF_PARAGRAPH: &str = "move-to-end-of-paragraph";
pub const SHORTCUT_GO_TO_LINE_NUMBER: &str = "go-to-line-number";
pub const SHORTCUT_MOVE_TO_MATCHING_PAREN: &str = "move-to-matching-parenthesis";

pub const SHORTCUT_DELETE_CHARACTER_FORWARD: &str = "delete-character-forward";
pub const SHORTCUT_DELETE_CHARACTER_BACKWARD: &str = "delete-character-backward";
pub const SHORTCUT_DELETE_WORD_FORWARD: &str = "delete-word-forward";
pub const SHORTCUT_DELETE_WORD_BACKWARD: &str = "delete-word-backward";
pub const SHORTCUT_DELETE_TO_END_OF_LINE: &str = "delete-to-end-of-line";
pub const SHORTCUT_CUT: &str = "cut";
pub const SHORTCUT_COPY: &str = "copy";
pub const SHORTCUT_PASTE: &str = "paste";
pub const SHORTCUT_CYCLE_CLIPBOARD: &str = "cycle-through-clipboard";
pub const SHORTCUT_TRANSPOSE_CHARACTERS: &str = "transpose-characters";
pub const SHORTCUT_TRANSPOSE_WORDS: &str = "transpose-words";
pub const SHORTCUT_UNDO: &str = "undo";
pub const SHORTCUT_REDO: &str = "redo";
pub const SHORTCUT_OPEN_NEW_LINE: &str = "open-new-line";
pub const SHORTCUT_INDENT_OR_COMPLETE: &str = "indent-or-complete";
pub const SHORTCUT_DELETE_TO_BEGINNING_OF_LINE: &str = "delete-to-beginning-of-line";

pub const SHORTCUT_UPPERCASE_WORD: &str = "uppercase-word";
pub const SHORTCUT_LOWERCASE_WORD: &str = "lowercase-word";
pub const SHORTCUT_CAPITALIZE_WORD: &str = "capitalize-word";
pub const SHORTCUT_FILL_PARAGRAPH: &str = "justify-paragraph";
pub const SHORTCUT_JOIN_LINES: &str = "join-lines";

pub const SHORTCUT_BOLD: &str = "bold";
pub const SHORTCUT_ITALIC: &str = "italic";
pub const SHORTCUT_UNDERLINE: &str = "underline";
pub const SHORTCUT_INSERT_HYPERLINK: &str = "insert-hyperlink";

pub const SHORTCUT_TOGGLE_COMMENT: &str = "toggle-comment";
pub const SHORTCUT_DUPLICATE_LINE: &str = "duplicate-line-selection";
pub const SHORTCUT_MOVE_LINE_UP: &str = "move-line-up";
pub const SHORTCUT_MOVE_LINE_DOWN: &str = "move-line-down";
pub const SHORTCUT_INDENT_REGION: &str = "indent-region";
pub const SHORTCUT_DEDENT_REGION: &str = "dedent-region";

pub const SHORTCUT_INCREMENTAL_SEARCH_FORWARD: &str = "incremental-search-forward";
pub const SHORTCUT_INCREMENTAL_SEARCH_BACKWARD: &str = "incremental-search-backward";
pub const SHORTCUT_FIND_AND_REPLACE: &str = "find-and-replace";
pub const SHORTCUT_FIND_AND_REPLACE_REGEX: &str = "find-and-replace-with-regex";
pub const SHORTCUT_FIND_NEXT: &str = "find-next";
pub const SHORTCUT_FIND_PREVIOUS: &str = "find-previous";

pub const SHORTCUT_SET_MARK: &str = "set-mark";
pub const SHORTCUT_SELECT_ALL: &str = "select-all";

pub const SHORTCUT_DEFINITIONS: &[ShortcutDefinition] = &[
    // Cursor movement
    ShortcutDefinition {
        key: SHORTCUT_MOVE_TO_BEGINNING_OF_LINE,
        description: "Move cursor to beginning of line",
        pc: &["Home", "Ctrl+A"],
        mac: &["Cmd+Left", "Ctrl+A"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_TO_END_OF_LINE,
        description: "Move cursor to end of line",
        pc: &["End", "Ctrl+E"],
        mac: &["Cmd+Right", "Ctrl+E"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_FORWARD_ONE_CHARACTER,
        description: "Move cursor forward one character",
        pc: &["Ctrl+F"],
        mac: &["Ctrl+F"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_BACKWARD_ONE_CHARACTER,
        description: "Move cursor backward one character",
        pc: &["Ctrl+B"],
        mac: &["Ctrl+B"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_TO_NEXT_LINE,
        description: "Move cursor to next line",
        pc: &["Ctrl+N"],
        mac: &["Ctrl+N"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_TO_PREVIOUS_LINE,
        description: "Move cursor to previous line",
        pc: &["Ctrl+P"],
        mac: &["Ctrl+P"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_FORWARD_ONE_WORD,
        description: "Move cursor forward one word",
        pc: &["Alt+F", "Ctrl+Right"],
        mac: &["Option+Right"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_BACKWARD_ONE_WORD,
        description: "Move cursor backward one word",
        pc: &["Alt+B", "Ctrl+Left"],
        mac: &["Option+Left"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_TO_BEGINNING_OF_SENTENCE,
        description: "Move cursor to beginning of sentence",
        pc: &["Alt+A"],
        mac: &["Option+A"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_TO_END_OF_SENTENCE,
        description: "Move cursor to end of sentence",
        pc: &["Alt+E"],
        mac: &["Option+E"],
    },
    ShortcutDefinition {
        key: SHORTCUT_SCROLL_DOWN_ONE_SCREEN,
        description: "Scroll viewport down",
        pc: &["PageDown"],
        mac: &["PageDown"],
    },
    ShortcutDefinition {
        key: SHORTCUT_SCROLL_UP_ONE_SCREEN,
        description: "Scroll viewport up",
        pc: &["PageUp"],
        mac: &["PageUp"],
    },
    ShortcutDefinition {
        key: SHORTCUT_RECENTER_SCREEN,
        description: "Recenter screen on cursor",
        pc: &["Ctrl+L"],
        mac: &["Ctrl+L"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_TO_BEGINNING_OF_DOCUMENT,
        description: "Move cursor to beginning of document",
        pc: &["Ctrl+Home"],
        mac: &["Cmd+Up"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_TO_END_OF_DOCUMENT,
        description: "Move cursor to end of document",
        pc: &["Ctrl+End"],
        mac: &["Cmd+Down"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_TO_BEGINNING_OF_PARAGRAPH,
        description: "Move cursor to beginning of paragraph",
        pc: &["Ctrl+Up"],
        mac: &["Option+Up"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_TO_END_OF_PARAGRAPH,
        description: "Move cursor to end of paragraph",
        pc: &["Ctrl+Down"],
        mac: &["Option+Down"],
    },
    ShortcutDefinition {
        key: SHORTCUT_GO_TO_LINE_NUMBER,
        description: "Open go to line dialog",
        pc: &["Ctrl+G"],
        mac: &["Cmd+L"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_TO_MATCHING_PAREN,
        description: "Jump to matching parenthesis",
        pc: &["Ctrl+Alt+F"],
        mac: &["Ctrl+Option+F"],
    },
    // Editing & deletion
    ShortcutDefinition {
        key: SHORTCUT_DELETE_CHARACTER_FORWARD,
        description: "Delete character forward",
        pc: &["Delete", "Ctrl+D"],
        mac: &["Delete", "Ctrl+D"],
    },
    ShortcutDefinition {
        key: SHORTCUT_DELETE_CHARACTER_BACKWARD,
        description: "Delete character backward",
        pc: &["Backspace", "Ctrl+H"],
        mac: &["Backspace"],
    },
    ShortcutDefinition {
        key: SHORTCUT_DELETE_WORD_FORWARD,
        description: "Delete word forward",
        pc: &["Ctrl+Delete", "Alt+D"],
        mac: &["Option+Delete"],
    },
    ShortcutDefinition {
        key: SHORTCUT_DELETE_WORD_BACKWARD,
        description: "Delete word backward",
        pc: &["Ctrl+Backspace", "Alt+Backspace"],
        mac: &["Option+Backspace"],
    },
    ShortcutDefinition {
        key: SHORTCUT_DELETE_TO_END_OF_LINE,
        description: "Delete to end of line",
        pc: &["Ctrl+K"],
        mac: &["Ctrl+K"],
    },
    ShortcutDefinition {
        key: SHORTCUT_CUT,
        description: "Cut selection",
        pc: &["Ctrl+X", "Ctrl+W"],
        mac: &["Cmd+X"],
    },
    ShortcutDefinition {
        key: SHORTCUT_COPY,
        description: "Copy selection",
        pc: &["Ctrl+C", "Alt+W"],
        mac: &["Cmd+C"],
    },
    ShortcutDefinition {
        key: SHORTCUT_PASTE,
        description: "Paste",
        pc: &["Ctrl+V", "Ctrl+Y"],
        mac: &["Cmd+V"],
    },
    ShortcutDefinition {
        key: SHORTCUT_CYCLE_CLIPBOARD,
        description: "Cycle clipboard entries",
        pc: &["Alt+Y"],
        mac: &["Option+Y"],
    },
    ShortcutDefinition {
        key: SHORTCUT_TRANSPOSE_CHARACTERS,
        description: "Transpose characters",
        pc: &["Ctrl+T"],
        mac: &["Ctrl+T"],
    },
    ShortcutDefinition {
        key: SHORTCUT_TRANSPOSE_WORDS,
        description: "Transpose words",
        pc: &["Alt+T"],
        mac: &["Option+T"],
    },
    ShortcutDefinition {
        key: SHORTCUT_UNDO,
        description: "Undo last edit",
        pc: &["Ctrl+Z", "Ctrl+/"],
        mac: &["Cmd+Z"],
    },
    ShortcutDefinition {
        key: SHORTCUT_REDO,
        description: "Redo last edit",
        pc: &["Ctrl+Y", "Ctrl+Shift+Z"],
        mac: &["Cmd+Shift+Z"],
    },
    ShortcutDefinition {
        key: SHORTCUT_OPEN_NEW_LINE,
        description: "Open new line below",
        pc: &["Ctrl+O"],
        mac: &["Ctrl+O"],
    },
    ShortcutDefinition {
        key: SHORTCUT_INDENT_OR_COMPLETE,
        description: "Indent or complete",
        pc: &["Ctrl+Shift+I"],
        mac: &["Ctrl+Shift+I"],
    },
    ShortcutDefinition {
        key: SHORTCUT_DELETE_TO_BEGINNING_OF_LINE,
        description: "Delete to beginning of line",
        pc: &["Ctrl+U"],
        mac: &["Cmd+Backspace"],
    },
    // Text transformation
    ShortcutDefinition {
        key: SHORTCUT_UPPERCASE_WORD,
        description: "Uppercase word",
        pc: &["Alt+U"],
        mac: &["Option+U"],
    },
    ShortcutDefinition {
        key: SHORTCUT_LOWERCASE_WORD,
        description: "Lowercase word",
        pc: &["Alt+L"],
        mac: &["Option+L"],
    },
    ShortcutDefinition {
        key: SHORTCUT_CAPITALIZE_WORD,
        description: "Capitalize word",
        pc: &["Alt+C"],
        mac: &["Option+C"],
    },
    ShortcutDefinition {
        key: SHORTCUT_FILL_PARAGRAPH,
        description: "Justify paragraph",
        pc: &["Alt+Q"],
        mac: &["Option+Q"],
    },
    ShortcutDefinition {
        key: SHORTCUT_JOIN_LINES,
        description: "Join lines",
        pc: &["Alt+Shift+6"],
        mac: &["Option+Shift+6"],
    },
    // Formatting
    ShortcutDefinition {
        key: SHORTCUT_BOLD,
        description: "Toggle bold formatting",
        pc: &["Ctrl+B"],
        mac: &["Cmd+B"],
    },
    ShortcutDefinition {
        key: SHORTCUT_ITALIC,
        description: "Toggle italic formatting",
        pc: &["Ctrl+I"],
        mac: &["Cmd+I"],
    },
    ShortcutDefinition {
        key: SHORTCUT_UNDERLINE,
        description: "Toggle underline formatting",
        pc: &["Ctrl+Shift+U"],
        mac: &["Cmd+U"],
    },
    ShortcutDefinition {
        key: SHORTCUT_INSERT_HYPERLINK,
        description: "Insert hyperlink",
        pc: &["Ctrl+Shift+K"],
        mac: &["Cmd+K"],
    },
    // Code editing
    ShortcutDefinition {
        key: SHORTCUT_TOGGLE_COMMENT,
        description: "Toggle comment",
        pc: &["Ctrl+/", "Alt+;"],
        mac: &["Cmd+/"],
    },
    ShortcutDefinition {
        key: SHORTCUT_DUPLICATE_LINE,
        description: "Duplicate line or selection",
        pc: &["Ctrl+D"],
        mac: &["Cmd+Shift+D"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_LINE_UP,
        description: "Move line up",
        pc: &["Alt+Up"],
        mac: &["Option+Up"],
    },
    ShortcutDefinition {
        key: SHORTCUT_MOVE_LINE_DOWN,
        description: "Move line down",
        pc: &["Alt+Down"],
        mac: &["Option+Down"],
    },
    ShortcutDefinition {
        key: SHORTCUT_INDENT_REGION,
        description: "Indent region",
        pc: &["Ctrl+]"],
        mac: &["Cmd+]"],
    },
    ShortcutDefinition {
        key: SHORTCUT_DEDENT_REGION,
        description: "Dedent region",
        pc: &["Ctrl+["],
        mac: &["Cmd+["],
    },
    // Search & replace
    ShortcutDefinition {
        key: SHORTCUT_INCREMENTAL_SEARCH_FORWARD,
        description: "Incremental search forward",
        pc: &["Ctrl+F", "Ctrl+S"],
        mac: &["Cmd+F"],
    },
    ShortcutDefinition {
        key: SHORTCUT_INCREMENTAL_SEARCH_BACKWARD,
        description: "Incremental search backward",
        pc: &["Ctrl+R"],
        mac: &["Ctrl+R"],
    },
    ShortcutDefinition {
        key: SHORTCUT_FIND_AND_REPLACE,
        description: "Find and replace",
        pc: &["Ctrl+H"],
        mac: &["Cmd+Shift+H"],
    },
    ShortcutDefinition {
        key: SHORTCUT_FIND_AND_REPLACE_REGEX,
        description: "Find and replace with regex",
        pc: &["Ctrl+Alt+Shift+5"],
        mac: &["Ctrl+Option+Shift+5"],
    },
    ShortcutDefinition {
        key: SHORTCUT_FIND_NEXT,
        description: "Find next match",
        pc: &["F3"],
        mac: &["Cmd+G"],
    },
    ShortcutDefinition {
        key: SHORTCUT_FIND_PREVIOUS,
        description: "Find previous match",
        pc: &["Shift+F3"],
        mac: &["Cmd+Shift+G"],
    },
    // Mark & region
    ShortcutDefinition {
        key: SHORTCUT_SET_MARK,
        description: "Set mark for selection",
        pc: &["Ctrl+Space", "Ctrl+@"],
        mac: &["Ctrl+Space"],
    },
    ShortcutDefinition {
        key: SHORTCUT_SELECT_ALL,
        description: "Select all text",
        pc: &["Ctrl+A"],
        mac: &["Cmd+A", "Ctrl+A"],
    },
    // Application controls
    ShortcutDefinition {
        key: SHORTCUT_LAUNCH_TASK,
        description: "Launch selected draft task",
        pc: &["Enter"],
        mac: &["Enter"],
    },
    ShortcutDefinition {
        key: SHORTCUT_NEW_LINE,
        description: "Insert newline in draft",
        pc: &["Shift+Enter"],
        mac: &["Shift+Enter"],
    },
    ShortcutDefinition {
        key: SHORTCUT_NEXT_FIELD,
        description: "Focus next control",
        pc: &["Tab"],
        mac: &["Tab"],
    },
    ShortcutDefinition {
        key: SHORTCUT_PREV_FIELD,
        description: "Focus previous control",
        pc: &["Shift+Tab"],
        mac: &["Shift+Tab"],
    },
    ShortcutDefinition {
        key: SHORTCUT_SHORTCUT_HELP,
        description: "Open keyboard shortcut help",
        pc: &["Ctrl+?"],
        mac: &["Cmd+?"],
    },
    ShortcutDefinition {
        key: SHORTCUT_NAVIGATE_CARDS,
        description: "Navigate between cards",
        pc: &["Up", "Down"],
        mac: &["Up", "Down"],
    },
    ShortcutDefinition {
        key: SHORTCUT_OPEN_SETTINGS,
        description: "Open settings dialog",
        pc: &["Enter"],
        mac: &["Enter"],
    },
    ShortcutDefinition {
        key: SHORTCUT_STOP_TASK,
        description: "Stop active task",
        pc: &["Enter"],
        mac: &["Enter"],
    },
    ShortcutDefinition {
        key: SHORTCUT_QUIT,
        description: "Quit the TUI",
        pc: &["Ctrl+C"],
        mac: &["Ctrl+C"],
    },
];
