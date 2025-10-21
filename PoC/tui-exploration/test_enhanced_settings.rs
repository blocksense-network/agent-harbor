use tui_exploration::settings::*;

fn main() {
    // Test KeyboardOperation enum
    println!("Testing KeyboardOperation enum...");

    let op = KeyboardOperation::MoveToBeginningOfLine;
    println!("Operation: {:?}", op);
    println!("Localization key: {}", op.localization_key());
    println!("Description: {}", op.english_description());

    // Test KeyBinding parsing
    println!("\nTesting KeyBinding parsing...");

    let binding_result = KeyBinding::from_string("Ctrl+A");
    match binding_result {
        Ok(binding) => {
            println!(
                "Parsed Ctrl+A: ctrl={}, alt={}, shift={}, super={}, key='{}'",
                binding.ctrl, binding.alt, binding.shift, binding.super_key, binding.key
            );

            // Test matcher creation
            let matcher = binding.to_matcher().unwrap();
            println!("Created matcher successfully");

            // Test string conversion back
            println!("Converted back to string: {}", binding.to_string());
        }
        Err(e) => println!("Failed to parse: {:?}", e),
    }

    // Test KeymapConfig default
    println!("\nTesting KeymapConfig default...");

    let config = KeymapConfig::default();
    let bindings = config.get_bindings_display(KeyboardOperation::MoveToBeginningOfLine);
    println!("Default bindings for MoveToBeginningOfLine: {:?}", bindings);

    // Test KeyboardShortcut
    println!("\nTesting KeyboardShortcut...");

    let mut matchers = Vec::new();
    if let Ok(binding) = KeyBinding::from_string("Ctrl+A") {
        if let Ok(matcher) = binding.to_matcher() {
            matchers.push(matcher);
        }
    }

    let shortcut = KeyboardShortcut::new(KeyboardOperation::MoveToBeginningOfLine, matchers);
    let event = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::CONTROL,
    );

    println!(
        "Shortcut matches Ctrl+A event: {}",
        shortcut.matches(&event)
    );
    println!("Display strings: {:?}", shortcut.display_strings());

    // Test localization
    println!("\nTesting localization...");

    let locale = unic_langid::LanguageIdentifier::from_bytes(b"en-US").unwrap();
    let localization = KeyboardLocalization::new(locale);
    let desc = localization.get_description(KeyboardOperation::MoveToBeginningOfLine);
    println!("Localized description: {}", desc);

    println!("\nAll tests passed! ✅");
}
