// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared TUI theme definition and loading helpers.
//!
//! The theme maps semantic color roles (see specs/Public/TUI-Color-Theme.md)
//! to concrete Ratatui colors. All view modules should receive a `Theme`
//! instance that is resolved once from configuration to avoid ad-hoc defaults.

use crate::tui_config::TuiConfig;
use ratatui::{
    prelude::Stylize,
    style::{Color, Modifier, Style},
};
use serde::Deserialize;
use std::collections::HashMap;
use std::{fs, path::Path};
use thiserror::Error;

/// Errors while loading or parsing custom theme definitions.
#[derive(Debug, Error)]
pub enum ThemeLoadError {
    #[error("failed to read theme file {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse theme file {path}: {source}")]
    ParseToml {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid color for field '{field}': {details}")]
    InvalidColor { field: String, details: String },
}

/// A single frame of a spinner animation
#[derive(Debug, Clone, PartialEq)]
pub struct SpinnerFrame {
    pub text: String,
    pub duration: Option<u64>,
}

/// A complete spinner definition ready for rendering
#[derive(Debug, Clone, PartialEq)]
pub struct Spinner {
    pub frames: Vec<SpinnerFrame>,
    pub interval: u64,
    pub color: Color,
}

impl Spinner {
    /// Calculate the maximum visual width of the spinner across all frames.
    /// This ignores ANSI codes and inline color tags.
    pub fn max_width(&self) -> usize {
        self.frames
            .iter()
            .map(|f| visual_len(&strip_color_tags(&f.text)))
            .max()
            .unwrap_or(0)
    }

    /// Get the current frame based on the elapsed time since the spinner started.
    pub fn current_frame(&self, elapsed: std::time::Duration) -> &SpinnerFrame {
        if self.frames.is_empty() {
            // Should not happen for a valid spinner, but handle gracefully
            static EMPTY_FRAME: SpinnerFrame = SpinnerFrame {
                text: String::new(),
                duration: None,
            };
            return &EMPTY_FRAME;
        }

        let elapsed_ms = elapsed.as_millis() as u64;
        let mut current_time = 0;

        // Calculate total cycle duration
        let total_duration: u64 =
            self.frames.iter().map(|f| f.duration.unwrap_or(self.interval)).sum();

        if total_duration == 0 {
            return &self.frames[0];
        }

        let cycle_time = elapsed_ms % total_duration;

        for frame in &self.frames {
            let duration = frame.duration.unwrap_or(self.interval);
            if current_time + duration > cycle_time {
                return frame;
            }
            current_time += duration;
        }

        // Fallback to last frame (shouldn't be reached due to modulo)
        self.frames.last().unwrap()
    }
}

// Helper to strip inline color tags like {error}
fn strip_color_tags(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '{' {
            // fast forward to '}'
            for inner in chars.by_ref() {
                if inner == '}' {
                    break;
                }
            }
        } else {
            output.push(c);
        }
    }
    output
}

// Simple visual length (char count for now, could use unicode-width)
fn visual_len(s: &str) -> usize {
    s.chars().count()
}

/// Charm-inspired theme with cohesive colors and styling
#[derive(Debug, Clone)]
pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub text: Color,
    pub muted: Color,
    pub primary: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub border: Color,
    pub border_focused: Color,

    // Extended fields from TUI-Color-Theme.md
    pub dim_text: Color,
    pub dim_border: Color,
    pub dim_error: Color,
    pub code_bg: Color,
    pub code_header_bg: Color,
    pub command_bg: Color,
    pub output_bg: Color,
    pub tooltip_bg: Color,
    pub tooltip_text: Color,
    pub gutter_stderr_bg: Color,
    pub gutter_stderr_fg: Color,

    // Shadow color for modal overlays
    pub shadow: Color,

    // Spinners
    pub spinners: HashMap<String, Spinner>,
}

impl Default for Theme {
    fn default() -> Self {
        let mut spinners = HashMap::new();
        spinners.insert(
            "awaiting_confirmation".to_string(),
            Spinner {
                frames: vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
                    .into_iter()
                    .map(|s| SpinnerFrame {
                        text: s.to_string(),
                        duration: None,
                    })
                    .collect(),
                interval: 80,
                color: Color::Rgb(127, 132, 156), // muted
            },
        );

        Self {
            // Colors aligned with scripts/tui_mockup.py Catppuccin-derived palette
            bg: Color::Rgb(20, 20, 30),                // bg
            surface: Color::Rgb(30, 30, 45),           // surface
            text: Color::Rgb(205, 214, 244),           // text
            muted: Color::Rgb(127, 132, 156),          // muted
            primary: Color::Rgb(137, 180, 250),        // primary
            accent: Color::Rgb(150, 190, 150),         // accent (desaturated green)
            success: Color::Rgb(150, 190, 150),        // match accent for success
            warning: Color::Rgb(250, 179, 135),        // warning
            error: Color::Rgb(225, 105, 110),          // error (redder, desat)
            border: Color::Rgb(69, 71, 90),            // border
            border_focused: Color::Rgb(137, 180, 250), // primary for focus

            dim_text: Color::Rgb(90, 95, 110),  // dim_text
            dim_border: Color::Rgb(45, 47, 60), // dim_border
            dim_error: Color::Rgb(110, 60, 75), // dim_error

            code_bg: Color::Rgb(25, 25, 35),        // code_bg
            code_header_bg: Color::Rgb(35, 35, 50), // code_header_bg
            command_bg: Color::Rgb(30, 30, 46),     // command_bg
            output_bg: Color::Rgb(20, 20, 30),      // output_bg

            tooltip_bg: Color::Rgb(35, 35, 50), // keep close to header for readability
            tooltip_text: Color::Rgb(205, 214, 244), // text

            gutter_stderr_bg: Color::Rgb(225, 105, 110), // align stderr gutter to error
            gutter_stderr_fg: Color::Rgb(20, 20, 30),    // same as bg for contrast

            shadow: Color::Rgb(10, 10, 15), // shadow for modal overlays

            spinners,
        }
    }
}

impl Theme {
    /// High-contrast palette tuned for accessibility (Tailwind-inspired).
    pub fn high_contrast() -> Self {
        // High contrast uses same spinners for now, but could override if needed
        let default = Self::default();
        Self {
            bg: Color::Rgb(3, 7, 18),                  // slate-950
            surface: Color::Rgb(15, 23, 42),           // slate-900
            text: Color::Rgb(226, 232, 240),           // slate-200
            muted: Color::Rgb(148, 163, 184),          // slate-400
            primary: Color::Rgb(59, 130, 246),         // blue-500
            accent: Color::Rgb(34, 197, 94),           // green-500
            success: Color::Rgb(34, 197, 94),          // green-500
            warning: Color::Rgb(249, 115, 22),         // orange-500
            error: Color::Rgb(239, 68, 68),            // red-500
            border: Color::Rgb(30, 41, 59),            // slate-800
            border_focused: Color::Rgb(59, 130, 246),  // blue-500
            dim_text: Color::Rgb(100, 116, 139),       // slate-500
            dim_border: Color::Rgb(51, 65, 85),        // slate-700
            dim_error: Color::Rgb(127, 29, 29),        // red-800
            code_bg: Color::Rgb(15, 23, 42),           // slate-900
            code_header_bg: Color::Rgb(30, 41, 59),    // slate-800
            command_bg: Color::Rgb(15, 23, 42),        // slate-900
            output_bg: Color::Rgb(3, 7, 18),           // slate-950
            tooltip_bg: Color::Rgb(30, 41, 59),        // slate-800
            tooltip_text: Color::Rgb(226, 232, 240),   // slate-200
            gutter_stderr_bg: Color::Rgb(239, 68, 68), // red-500
            gutter_stderr_fg: Color::Rgb(3, 7, 18),    // slate-950

            shadow: Color::Rgb(0, 0, 0), // black shadow for high contrast
            spinners: default.spinners,
        }
    }

    /// Resolve a theme from configuration (optional custom file + high contrast).
    pub fn from_tui_config(config: &TuiConfig) -> Result<Self, ThemeLoadError> {
        let mut theme = if config.high_contrast.unwrap_or(false) {
            Theme::high_contrast()
        } else {
            Theme::default()
        };

        if let Some(path) = config.theme.as_ref() {
            theme = theme.merge_overrides_from_file(path)?;
        }

        Ok(theme)
    }

    /// Merge overrides from an external TOML file on top of the current theme.
    pub fn merge_overrides_from_file<P: AsRef<Path>>(
        self,
        path: P,
    ) -> Result<Self, ThemeLoadError> {
        let path_ref = path.as_ref();
        let raw = fs::read_to_string(path_ref).map_err(|source| ThemeLoadError::Io {
            path: path_ref.display().to_string(),
            source,
        })?;
        let overrides: ThemeOverrides =
            toml::from_str(&raw).map_err(|source| ThemeLoadError::ParseToml {
                path: path_ref.display().to_string(),
                source,
            })?;
        self.apply_overrides(overrides)
    }

    fn apply_overrides(self, overrides: ThemeOverrides) -> Result<Self, ThemeLoadError> {
        let mut theme = self;
        let apply = |target: &mut Color,
                     value: Option<ColorValue>,
                     field: &str|
         -> Result<(), ThemeLoadError> {
            if let Some(v) = value {
                *target = parse_color(v, field)?;
            }
            Ok(())
        };

        apply(&mut theme.bg, overrides.base, "base")?;
        apply(&mut theme.surface, overrides.surface, "surface")?;
        apply(&mut theme.text, overrides.text, "text")?;
        apply(&mut theme.muted, overrides.muted, "muted")?;
        apply(&mut theme.primary, overrides.primary, "primary")?;
        apply(&mut theme.accent, overrides.accent, "accent")?;
        apply(&mut theme.success, overrides.success, "success")?;
        apply(&mut theme.warning, overrides.warning, "warning")?;
        apply(&mut theme.error, overrides.error, "error")?;
        apply(&mut theme.border, overrides.border, "border")?;
        apply(
            &mut theme.border_focused,
            overrides.border_focused,
            "border-focused",
        )?;
        apply(&mut theme.dim_text, overrides.dim_text, "dim-text")?;
        apply(&mut theme.dim_border, overrides.dim_border, "dim-border")?;
        apply(&mut theme.dim_error, overrides.dim_error, "dim-error")?;
        apply(&mut theme.code_bg, overrides.code_bg, "code-bg")?;
        apply(
            &mut theme.code_header_bg,
            overrides.code_header_bg,
            "code-header-bg",
        )?;
        apply(&mut theme.command_bg, overrides.command_bg, "command-bg")?;
        apply(&mut theme.output_bg, overrides.output_bg, "output-bg")?;
        apply(&mut theme.tooltip_bg, overrides.tooltip_bg, "tooltip-bg")?;
        apply(
            &mut theme.tooltip_text,
            overrides.tooltip_text,
            "tooltip-text",
        )?;
        apply(
            &mut theme.gutter_stderr_bg,
            overrides.gutter_stderr_bg,
            "gutter-stderr-bg",
        )?;
        apply(
            &mut theme.gutter_stderr_fg,
            overrides.gutter_stderr_fg,
            "gutter-stderr-fg",
        )?;
        apply(&mut theme.shadow, overrides.shadow, "shadow")?;

        // Apply spinner overrides
        if let Some(spinner_overrides) = overrides.spinners {
            let mut sequences = HashMap::new();
            if let Some(seqs) = spinner_overrides.sequences {
                for (name, val) in seqs {
                    let frames: Vec<SpinnerFrame> = match val {
                        SpinnerSequenceValue::Simple(strs) => strs
                            .into_iter()
                            .map(|s| SpinnerFrame {
                                text: s,
                                duration: None,
                            })
                            .collect(),
                        SpinnerSequenceValue::Complex(frames) => frames
                            .into_iter()
                            .map(|f| SpinnerFrame {
                                text: f.text,
                                duration: f.duration,
                            })
                            .collect(),
                    };
                    sequences.insert(name, frames);
                }
            }

            if let Some(defs) = spinner_overrides.definitions {
                for (name, def) in defs {
                    let frames = sequences.get(&def.sequence).ok_or_else(|| {
                        ThemeLoadError::InvalidColor {
                            field: format!("spinners.definitions.{}", name),
                            details: format!("unknown sequence '{}'", def.sequence),
                        }
                    })?;

                    let color = theme.get_color(&def.color).ok_or_else(|| {
                        ThemeLoadError::InvalidColor {
                            field: format!("spinners.definitions.{}", name),
                            details: format!("unknown color role '{}'", def.color),
                        }
                    })?;

                    theme.spinners.insert(
                        name,
                        Spinner {
                            frames: frames.clone(),
                            interval: def.interval,
                            color,
                        },
                    );
                }
            }
        }

        Ok(theme)
    }

    pub fn get_color(&self, role: &str) -> Option<Color> {
        match role.to_lowercase().as_str() {
            "base" => Some(self.bg),
            "surface" => Some(self.surface),
            "text" => Some(self.text),
            "muted" => Some(self.muted),
            "primary" => Some(self.primary),
            "accent" => Some(self.accent),
            "success" => Some(self.success),
            "warning" => Some(self.warning),
            "error" => Some(self.error),
            "border" => Some(self.border),
            "border-focused" | "border_focused" => Some(self.border_focused),
            "dim-text" | "dim_text" => Some(self.dim_text),
            "dim-border" | "dim_border" => Some(self.dim_border),
            "dim-error" | "dim_error" => Some(self.dim_error),
            "code-bg" | "code_bg" => Some(self.code_bg),
            "code-header-bg" | "code_header_bg" => Some(self.code_header_bg),
            "command-bg" | "command_bg" => Some(self.command_bg),
            "output-bg" | "output_bg" => Some(self.output_bg),
            "tooltip-bg" | "tooltip_bg" => Some(self.tooltip_bg),
            "tooltip-text" | "tooltip_text" => Some(self.tooltip_text),
            "gutter-stderr-bg" | "gutter_stderr_bg" => Some(self.gutter_stderr_bg),
            "gutter-stderr-fg" | "gutter_stderr_fg" => Some(self.gutter_stderr_fg),
            "shadow" => Some(self.shadow),
            _ => None,
        }
    }

    /// Create a card block with Charm-style rounded borders and padding
    pub fn card_block(&self, title: &str) -> ratatui::widgets::Block<'_> {
        let title_line = ratatui::text::Line::from(vec![
            ratatui::text::Span::raw("┤").fg(self.border),
            ratatui::text::Span::raw(format!(" {} ", title))
                .style(Style::default().fg(self.text).add_modifier(Modifier::BOLD)),
            ratatui::text::Span::raw("├").fg(self.border),
        ]);

        ratatui::widgets::Block::default()
            .title(title_line)
            .title_alignment(ratatui::layout::Alignment::Left)
            .borders(ratatui::widgets::Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(self.border))
            .padding(ratatui::widgets::Padding::new(1, 1, 1, 1))
            .style(Style::default().bg(self.bg))
    }

    /// Create a card block with a right-aligned button in the title area
    pub fn card_block_with_button(
        &self,
        title: &str,
        button_text: &str,
        button_focused: bool,
    ) -> ratatui::widgets::Block<'_> {
        let button_style = if button_focused {
            Style::default().fg(self.bg).bg(self.error).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.error).bg(self.bg).add_modifier(Modifier::BOLD)
        };

        let title_line = ratatui::text::Line::from(vec![
            ratatui::text::Span::raw("┤").fg(self.border),
            ratatui::text::Span::raw(format!(" {} ", title))
                .style(Style::default().fg(self.text).add_modifier(Modifier::BOLD)),
            ratatui::text::Span::raw("├").fg(self.border),
            ratatui::text::Span::raw(" ".repeat(15)), // Spacer to push button to right
            ratatui::text::Span::styled(format!(" {} ", button_text), button_style),
        ]);

        ratatui::widgets::Block::default()
            .title(title_line)
            .title_alignment(ratatui::layout::Alignment::Left)
            .borders(ratatui::widgets::Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(self.border))
            .padding(ratatui::widgets::Padding::new(2, 2, 1, 1))
            .style(Style::default().bg(self.bg))
    }

    /// Style for primary elements
    pub fn primary_style(&self) -> Style {
        Style::default().fg(self.primary).add_modifier(Modifier::BOLD)
    }

    /// Style for focused elements
    pub fn focused_style(&self) -> Style {
        Style::default().fg(self.bg).bg(self.primary).add_modifier(Modifier::BOLD)
    }

    /// Style for text elements
    pub fn text_style(&self) -> Style {
        Style::default().fg(self.text)
    }

    /// Style for muted elements
    pub fn muted_style(&self) -> Style {
        Style::default().fg(self.muted)
    }

    /// Style for success elements
    pub fn success_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    /// Style for warning elements
    pub fn warning_style(&self) -> Style {
        Style::default().fg(self.warning)
    }

    /// Style for error elements
    pub fn error_style(&self) -> Style {
        Style::default().fg(self.error)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ThemeOverrides {
    base: Option<ColorValue>,
    surface: Option<ColorValue>,
    text: Option<ColorValue>,
    muted: Option<ColorValue>,
    primary: Option<ColorValue>,
    accent: Option<ColorValue>,
    success: Option<ColorValue>,
    warning: Option<ColorValue>,
    error: Option<ColorValue>,
    border: Option<ColorValue>,
    border_focused: Option<ColorValue>,
    dim_text: Option<ColorValue>,
    dim_border: Option<ColorValue>,
    dim_error: Option<ColorValue>,
    code_bg: Option<ColorValue>,
    code_header_bg: Option<ColorValue>,
    command_bg: Option<ColorValue>,
    output_bg: Option<ColorValue>,
    tooltip_bg: Option<ColorValue>,
    tooltip_text: Option<ColorValue>,
    gutter_stderr_bg: Option<ColorValue>,
    gutter_stderr_fg: Option<ColorValue>,
    shadow: Option<ColorValue>,
    spinners: Option<SpinnerOverrides>,
}

#[derive(Debug, Deserialize)]
struct SpinnerOverrides {
    sequences: Option<HashMap<String, SpinnerSequenceValue>>,
    definitions: Option<HashMap<String, SpinnerDefinitionValue>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SpinnerSequenceValue {
    Simple(Vec<String>),
    Complex(Vec<SpinnerFrameValue>),
}

#[derive(Debug, Deserialize)]
struct SpinnerFrameValue {
    text: String,
    duration: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct SpinnerDefinitionValue {
    sequence: String,
    interval: u64,
    color: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
enum ColorValue {
    Hex(String),
    Rgb { r: u8, g: u8, b: u8 },
    Array(Vec<u8>),
}

fn parse_color(value: ColorValue, field: &str) -> Result<Color, ThemeLoadError> {
    match value {
        ColorValue::Hex(s) => parse_hex(&s, field),
        ColorValue::Rgb { r, g, b } => Ok(Color::Rgb(r, g, b)),
        ColorValue::Array(vals) => {
            if vals.len() == 3 {
                Ok(Color::Rgb(vals[0], vals[1], vals[2]))
            } else {
                Err(ThemeLoadError::InvalidColor {
                    field: field.to_string(),
                    details: format!("expected [r,g,b], got length {}", vals.len()),
                })
            }
        }
    }
}

fn parse_hex(hex: &str, field: &str) -> Result<Color, ThemeLoadError> {
    let cleaned = hex.trim_start_matches('#');
    if cleaned.len() != 6 {
        return Err(ThemeLoadError::InvalidColor {
            field: field.to_string(),
            details: format!("hex color must be 6 characters, got {}", cleaned.len()),
        });
    }
    let r = u8::from_str_radix(&cleaned[0..2], 16).map_err(|e| ThemeLoadError::InvalidColor {
        field: field.to_string(),
        details: format!("invalid red component: {}", e),
    })?;
    let g = u8::from_str_radix(&cleaned[2..4], 16).map_err(|e| ThemeLoadError::InvalidColor {
        field: field.to_string(),
        details: format!("invalid green component: {}", e),
    })?;
    let b = u8::from_str_radix(&cleaned[4..6], 16).map_err(|e| ThemeLoadError::InvalidColor {
        field: field.to_string(),
        details: format!("invalid blue component: {}", e),
    })?;

    Ok(Color::Rgb(r, g, b))
}
