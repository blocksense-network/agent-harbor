#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_key_binding_parsing() {
        // Test basic key binding parsing
        let binding = KeyBinding::from_string("Ctrl+A").unwrap();
        assert_eq!(binding.key, "A");
        assert!(binding.ctrl);
        assert!(!binding.alt);
        assert!(!binding.shift);
        assert!(!binding.super_key);

        let binding = KeyBinding::from_string("Alt+F").unwrap();
        assert_eq!(binding.key, "F");
        assert!(!binding.ctrl);
        assert!(binding.alt);
        assert!(!binding.shift);
        assert!(!binding.super_key);

        let binding = KeyBinding::from_string("Shift+Enter").unwrap();
        assert_eq!(binding.key, "Enter");
        assert!(!binding.ctrl);
        assert!(!binding.alt);
        assert!(binding.shift);
        assert!(!binding.super_key);

        let binding = KeyBinding::from_string("Cmd+X").unwrap();
        assert_eq!(binding.key, "X");
        assert!(!binding.ctrl);
        assert!(!binding.alt);
        assert!(!binding.shift);
        assert!(binding.super_key);
    }

    #[test]
    fn test_key_binding_to_string() {
        let binding = KeyBinding {
            key: "A".to_string(),
            ctrl: true,
            alt: false,
            shift: false,
            super_key: false,
        };
        assert_eq!(binding.to_string(), "Ctrl+A");

        let binding = KeyBinding {
            key: "Enter".to_string(),
            ctrl: true,
            alt: true,
            shift: true,
            super_key: false,
        };
        assert_eq!(binding.to_string(), "Ctrl+Alt+Shift+Enter");

        let binding = KeyBinding {
            key: "x".to_string(),
            ctrl: false,
            alt: false,
            shift: false,
            super_key: true,
        };
        assert_eq!(binding.to_string(), "Cmd+x");
    }

    #[test]
    fn test_key_binding_matches() {
        let binding = KeyBinding::from_string("Ctrl+A").unwrap();

        // Should match Ctrl+A
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert!(binding.matches(&event));

        // Should not match without Ctrl
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
        assert!(!binding.matches(&event));

        // Should not match different key
        let event = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
        assert!(!binding.matches(&event));

        // Should not match with extra modifiers
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        assert!(!binding.matches(&event));
    }

    #[test]
    fn test_special_key_matching() {
        let binding = KeyBinding::from_string("Home").unwrap();
        let event = KeyEvent::new(KeyCode::Home, KeyModifiers::empty());
        assert!(binding.matches(&event));

        let binding = KeyBinding::from_string("Enter").unwrap();
        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        assert!(binding.matches(&event));

        let binding = KeyBinding::from_string("Tab").unwrap();
        let event = KeyEvent::new(KeyCode::Tab, KeyModifiers::empty());
        assert!(binding.matches(&event));

        let binding = KeyBinding::from_string("Esc").unwrap();
        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
        assert!(binding.matches(&event));
    }

    #[test]
    fn test_case_insensitive_character_matching() {
        let binding = KeyBinding::from_string("a").unwrap();

        let event_lower = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
        assert!(binding.matches(&event_lower));

        let event_upper = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::empty());
        assert!(binding.matches(&event_upper));
    }

    #[test]
    fn test_multiple_modifiers() {
        let binding = KeyBinding::from_string("Ctrl+Shift+A").unwrap();

        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        assert!(binding.matches(&event));

        // Missing shift
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert!(!binding.matches(&event));

        // Missing ctrl
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SHIFT);
        assert!(!binding.matches(&event));

        // Extra alt
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL | KeyModifiers::SHIFT | KeyModifiers::ALT);
        assert!(!binding.matches(&event));
    }

    #[test]
    fn test_keymap_config_matches() {
        let config = KeymapConfig::default();

        // Test that move-to-beginning-of-line matches multiple bindings
        let ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert!(config.matches(KeyboardOperation::MoveToBeginningOfLine, &ctrl_a));

        let home = KeyEvent::new(KeyCode::Home, KeyModifiers::empty());
        assert!(config.matches(KeyboardOperation::MoveToBeginningOfLine, &home));

        let cmd_left = KeyEvent::new(KeyCode::Left, KeyModifiers::SUPER);
        assert!(config.matches(KeyboardOperation::MoveToBeginningOfLine, &cmd_left));

        // Test that it doesn't match wrong operation
        assert!(!config.matches(KeyboardOperation::MoveToEndOfLine, &ctrl_a));
    }

    #[test]
    fn test_keymap_config_get_bindings_display() {
        let config = KeymapConfig::default();

        let bindings = config.get_bindings_display(KeyboardOperation::MoveToBeginningOfLine);
        assert_eq!(bindings.len(), 3);
        // Should contain expected key combinations
        assert!(bindings.contains(&"Ctrl+A".to_string()));
        assert!(bindings.contains(&"Home".to_string()));
        assert!(bindings.contains(&"Cmd+Left".to_string()));

        // Test operation with single binding
        let bindings = config.get_bindings_display(KeyboardOperation::MoveForwardOneCharacter);
        assert_eq!(bindings.len(), 1);
        assert!(bindings.contains(&"Ctrl+F".to_string()));
    }

    #[test]
    fn test_default_keymap_has_multiple_bindings() {
        let config = KeymapConfig::default();

        // Test some operations that should have multiple default bindings
        assert!(config.move_to_beginning_of_line.as_ref().unwrap().len() >= 2);
        assert!(config.move_to_end_of_line.as_ref().unwrap().len() >= 2);
        assert!(config.move_forward_one_word.as_ref().unwrap().len() >= 2);
        assert!(config.undo.as_ref().unwrap().len() >= 2);
        assert!(config.copy.as_ref().unwrap().len() >= 2);
    }

    #[test]
    fn test_complex_key_binding_parsing() {
        // Test various complex key binding formats
        let test_cases = vec![
            ("C-a", KeyBinding { key: "a".to_string(), ctrl: true, alt: false, shift: false, super_key: false }),
            ("M-f", KeyBinding { key: "f".to_string(), ctrl: false, alt: true, shift: false, super_key: false }),
            ("S-Enter", KeyBinding { key: "Enter".to_string(), ctrl: false, alt: false, shift: true, super_key: false }),
            ("Cmd+X", KeyBinding { key: "X".to_string(), ctrl: false, alt: false, shift: false, super_key: true }),
            ("Ctrl+Alt+Shift+Delete", KeyBinding { key: "Delete".to_string(), ctrl: true, alt: true, shift: true, super_key: false }),
            ("Home", KeyBinding { key: "Home".to_string(), ctrl: false, alt: false, shift: false, super_key: false }),
        ];

        for (input, expected) in test_cases {
            let parsed = KeyBinding::from_string(input).unwrap();
            assert_eq!(parsed, expected, "Failed to parse: {}", input);
        }
    }

    #[test]
    fn test_key_binding_roundtrip() {
        let test_cases = vec![
            "Ctrl+A",
            "Alt+F",
            "Shift+Enter",
            "Cmd+X",
            "Ctrl+Alt+Shift+Delete",
            "Home",
            "Tab",
            "Esc",
        ];

        for original in test_cases {
            let binding = KeyBinding::from_string(original).unwrap();
            let roundtrip = binding.to_string();
            assert_eq!(roundtrip, original, "Roundtrip failed for: {}", original);
        }
    }

    #[test]
    fn test_platform_specific_defaults() {
        // The defaults should work regardless of platform
        let config = KeymapConfig::default();

        // These operations should have multiple bindings (cross-platform support)
        let multi_binding_ops = vec![
            KeyboardOperation::MoveToBeginningOfLine,
            KeyboardOperation::MoveToEndOfLine,
            KeyboardOperation::MoveForwardOneWord,
            KeyboardOperation::MoveBackwardOneWord,
            KeyboardOperation::ScrollDownOneScreen,
            KeyboardOperation::ScrollUpOneScreen,
            KeyboardOperation::MoveToBeginningOfDocument,
            KeyboardOperation::MoveToEndOfDocument,
            KeyboardOperation::DeleteCharacterForward,
            KeyboardOperation::DeleteWordForward,
            KeyboardOperation::DeleteWordBackward,
            KeyboardOperation::Cut,
            KeyboardOperation::Copy,
            KeyboardOperation::Paste,
            KeyboardOperation::Undo,
            KeyboardOperation::Redo,
            KeyboardOperation::ToggleComment,
            KeyboardOperation::IncrementalSearchForward,
            KeyboardOperation::FindAndReplace,
            KeyboardOperation::FindNext,
            KeyboardOperation::SetMark,
            KeyboardOperation::SelectAll,
        ];

        for op in multi_binding_ops {
            let bindings = config.get_bindings_display(op);
            assert!(!bindings.is_empty(), "Operation {:?} should have default bindings", op);
        }
    }

    #[test]
    fn test_keyboard_operation_enum() {
        // Test that all operations have localization keys and descriptions
        let all_operations = vec![
            KeyboardOperation::MoveToBeginningOfLine,
            KeyboardOperation::MoveToEndOfLine,
            KeyboardOperation::MoveForwardOneCharacter,
            KeyboardOperation::MoveBackwardOneCharacter,
            KeyboardOperation::MoveToNextLine,
            KeyboardOperation::MoveToPreviousLine,
            KeyboardOperation::MoveForwardOneWord,
            KeyboardOperation::MoveBackwardOneWord,
            KeyboardOperation::MoveToBeginningOfSentence,
            KeyboardOperation::MoveToEndOfSentence,
            KeyboardOperation::ScrollDownOneScreen,
            KeyboardOperation::ScrollUpOneScreen,
            KeyboardOperation::RecenterScreenOnCursor,
            KeyboardOperation::MoveToBeginningOfDocument,
            KeyboardOperation::MoveToEndOfDocument,
            KeyboardOperation::MoveToBeginningOfParagraph,
            KeyboardOperation::MoveToEndOfParagraph,
            KeyboardOperation::GoToLineNumber,
            KeyboardOperation::MoveToMatchingParenthesis,
            KeyboardOperation::DeleteCharacterForward,
            KeyboardOperation::DeleteCharacterBackward,
            KeyboardOperation::DeleteWordForward,
            KeyboardOperation::DeleteWordBackward,
            KeyboardOperation::DeleteToEndOfLine,
            KeyboardOperation::Cut,
            KeyboardOperation::Copy,
            KeyboardOperation::Paste,
            KeyboardOperation::CycleThroughClipboard,
            KeyboardOperation::TransposeCharacters,
            KeyboardOperation::TransposeWords,
            KeyboardOperation::Undo,
            KeyboardOperation::Redo,
            KeyboardOperation::OpenNewLine,
            KeyboardOperation::IndentOrComplete,
            KeyboardOperation::DeleteToBeginningOfLine,
            KeyboardOperation::UppercaseWord,
            KeyboardOperation::LowercaseWord,
            KeyboardOperation::CapitalizeWord,
            KeyboardOperation::JustifyParagraph,
            KeyboardOperation::JoinLines,
            KeyboardOperation::Bold,
            KeyboardOperation::Italic,
            KeyboardOperation::Underline,
            KeyboardOperation::InsertHyperlink,
            KeyboardOperation::ToggleComment,
            KeyboardOperation::DuplicateLineSelection,
            KeyboardOperation::MoveLineUp,
            KeyboardOperation::MoveLineDown,
            KeyboardOperation::IndentRegion,
            KeyboardOperation::DedentRegion,
            KeyboardOperation::IncrementalSearchForward,
            KeyboardOperation::IncrementalSearchBackward,
            KeyboardOperation::FindAndReplace,
            KeyboardOperation::FindAndReplaceWithRegex,
            KeyboardOperation::FindNext,
            KeyboardOperation::FindPrevious,
            KeyboardOperation::SetMark,
            KeyboardOperation::SelectAll,
        ];

        for op in all_operations {
            assert!(!op.localization_key().is_empty(), "Operation {:?} should have localization key", op);
            assert!(!op.english_description().is_empty(), "Operation {:?} should have description", op);
        }
    }

    #[test]
    fn test_operation_definitions() {
        let definitions = KeymapConfig::get_operation_definitions();

        // Should have a reasonable number of definitions
        assert!(definitions.len() > 20);

        // Test a specific operation
        let move_to_beginning = definitions.iter()
            .find(|d| d.operation == KeyboardOperation::MoveToBeginningOfLine)
            .unwrap();

        assert_eq!(move_to_beginning.pc_defaults.len(), 2); // Home, Ctrl+A
        assert_eq!(move_to_beginning.mac_defaults.len(), 2); // Cmd+Left, Ctrl+A
    }

    #[test]
    fn test_keyboard_shortcut_creation() {
        let mut matchers = Vec::new();

        // Create a simple matcher for Ctrl+A
        let binding = KeyBinding::from_string("Ctrl+A").unwrap();
        let matcher = binding.to_matcher().unwrap();
        matchers.push(matcher);

        let shortcut = KeyboardShortcut::new(KeyboardOperation::MoveToBeginningOfLine, matchers);

        // Test matching
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert!(shortcut.matches(&event));

        // Test display strings
        let displays = shortcut.display_strings();
        assert_eq!(displays.len(), 1);
        assert_eq!(displays[0], "Ctrl+A");
    }

    #[test]
    fn test_error_handling() {
        // Test invalid key binding format
        let result = KeyBinding::from_string("");
        assert!(result.is_err());

        let result = KeyBinding::from_string("Invalid+Key");
        assert!(result.is_err());

        // Test that valid bindings work
        let result = KeyBinding::from_string("Ctrl+A");
        assert!(result.is_ok());
    }

    #[test]
    fn test_localization_context() {
        let locale = unic_langid::langid!("en-US");
        let localization = KeyboardLocalization::new(locale);

        let desc = localization.get_description(KeyboardOperation::MoveToBeginningOfLine);
        assert!(!desc.is_empty());
        assert_eq!(desc, "Move cursor to beginning of line");

        let modifier_name = localization.get_modifier_name("ctrl");
        assert_eq!(modifier_name, "Ctrl");

        let modifier_name = localization.get_modifier_name("cmd");
        assert_eq!(modifier_name, "Cmd");
    }

    #[test]
    fn test_platform_detection() {
        // This test just ensures the platform detection doesn't panic
        let platform = Platform::current();
        match platform {
            Platform::Pc | Platform::Mac => {} // Valid values
        }
    }

    #[test]
    fn test_key_matcher_advanced_features() {
        // Test required vs optional modifiers
        let matcher = KeyMatcher::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL, // Required
            KeyModifiers::SHIFT,   // Optional
            Some('a'),             // Lower case for case-insensitive matching
        );

        // Should match Ctrl+A
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert!(matcher.matches(&event));

        // Should match Ctrl+Shift+A (optional modifier present)
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        assert!(matcher.matches(&event));

        // Should NOT match just Shift+A (required modifier missing)
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::SHIFT);
        assert!(!matcher.matches(&event));

        // Should NOT match Ctrl+A with extra Alt (not optional)
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL | KeyModifiers::ALT);
        assert!(!matcher.matches(&event));
    }
}
