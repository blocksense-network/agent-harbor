// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_tui::theme::Theme;
use ah_tui::tui_config::TuiConfig;
use ratatui::style::Color;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn loads_theme_overrides_from_file() {
    let mut file = NamedTempFile::new().expect("temp file");
    file.write_all(
        br##"base = "#010203"
primary = { r = 10, g = 20, b = 30 }
tooltip-text = [9, 8, 7]
"##,
    )
    .unwrap();

    let cfg = TuiConfig {
        theme: Some(file.path().to_string_lossy().to_string()),
        ..Default::default()
    };

    let theme = Theme::from_tui_config(&cfg).expect("theme parsed");
    let default = Theme::default();

    assert_eq!(theme.bg, Color::Rgb(1, 2, 3));
    assert_eq!(theme.primary, Color::Rgb(10, 20, 30));
    assert_eq!(theme.tooltip_text, Color::Rgb(9, 8, 7));
    // Unspecified fields should fall back to defaults
    assert_eq!(theme.border, default.border);
}

#[test]
fn respects_high_contrast_flag() {
    let cfg = TuiConfig {
        high_contrast: Some(true),
        ..Default::default()
    };

    let theme = Theme::from_tui_config(&cfg).expect("theme parsed");

    assert_eq!(theme.bg, Color::Rgb(3, 7, 18));
    assert_eq!(theme.text, Color::Rgb(226, 232, 240));
    assert_eq!(theme.border_focused, Color::Rgb(59, 130, 246));
}

#[test]
fn loads_spinner_definitions() {
    let mut file = NamedTempFile::new().expect("temp file");
    file.write_all(
        br##"
[spinners.sequences]
dots = ["a", "b"]
pulse = [
    { text = "x", duration = 100 },
    { text = "y", duration = 200 }
]

[spinners.definitions]
test_spinner = { sequence = "dots", interval = 50, color = "primary" }
complex_spinner = { sequence = "pulse", interval = 100, color = "error" }
"##,
    )
    .unwrap();

    let cfg = TuiConfig {
        theme: Some(file.path().to_string_lossy().to_string()),
        ..Default::default()
    };

    let theme = Theme::from_tui_config(&cfg).expect("theme parsed");

    let test_spinner = theme.spinners.get("test_spinner").expect("found test_spinner");
    assert_eq!(test_spinner.frames.len(), 2);
    assert_eq!(test_spinner.frames[0].text, "a");
    assert_eq!(test_spinner.interval, 50);
    assert_eq!(test_spinner.color, theme.primary);

    let complex_spinner = theme.spinners.get("complex_spinner").expect("found complex_spinner");
    assert_eq!(complex_spinner.frames.len(), 2);
    assert_eq!(complex_spinner.frames[0].text, "x");
    assert_eq!(complex_spinner.frames[0].duration, Some(100));
    assert_eq!(complex_spinner.color, theme.error);
}
