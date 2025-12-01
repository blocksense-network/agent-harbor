// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_tui::settings::Settings;
use ah_tui::theme::Theme;
use ah_tui::view::hit_test::HitTestRegistry;
use ah_tui::view_model::agent_session_model::{AgentSessionMouseAction, AgentSessionViewModel};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    style::{Color, Modifier, Style},
};
use std::{fs, path::Path};

const CELL_W: u16 = 8;
const CELL_H: u16 = 16;
const PADDING: u16 = 4;

#[allow(dead_code)]
pub fn render_snapshot(name: &str, vm: &AgentSessionViewModel, width: u16, height: u16) -> String {
    render_snapshot_with_theme(name, vm, width, height, &Theme::default())
}

/// Render with an explicit theme (to align tests with PRD/theme DI).
pub fn render_snapshot_with_theme(
    name: &str,
    vm: &AgentSessionViewModel,
    width: u16,
    height: u16,
    theme: &Theme,
) -> String {
    let buffer = render_buffer(vm, width, height, theme);
    write_svg_sidecar(name, &buffer);
    buffer_to_ascii(&buffer)
}

/// Render and return the raw buffer for style-aware assertions.
pub fn render_buffer(vm: &AgentSessionViewModel, width: u16, height: u16, theme: &Theme) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| ah_tui::view::agent_session_view::render_agent_session(f, vm, theme, None))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn buffer_to_ascii(buffer: &Buffer) -> String {
    framed_ascii(buffer)
}

fn escape_xml(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&apos;".into(),
            other => other.to_string(),
        })
        .collect()
}

fn color_to_hex(color: Option<Color>) -> Option<String> {
    match color? {
        Color::Reset => None,
        Color::Black => Some("#000000".into()),
        Color::Red => Some("#FF0000".into()),
        Color::Green => Some("#00FF00".into()),
        Color::Yellow => Some("#FFFF00".into()),
        Color::Blue => Some("#0000FF".into()),
        Color::Magenta => Some("#FF00FF".into()),
        Color::Cyan => Some("#00FFFF".into()),
        Color::Gray => Some("#808080".into()),
        Color::DarkGray => Some("#404040".into()),
        Color::LightRed => Some("#FF6666".into()),
        Color::LightGreen => Some("#66FF66".into()),
        Color::LightYellow => Some("#FFFF99".into()),
        Color::LightBlue => Some("#6699FF".into()),
        Color::LightMagenta => Some("#FF66FF".into()),
        Color::LightCyan => Some("#66FFFF".into()),
        Color::White => Some("#FFFFFF".into()),
        Color::Rgb(r, g, b) => Some(format!("#{:02X}{:02X}{:02X}", r, g, b)),
        Color::Indexed(idx) => Some(xterm_256_to_hex(idx)),
    }
}

fn xterm_256_to_hex(idx: u8) -> String {
    // Standard xterm 256-color palette conversion.
    if idx < 16 {
        return match idx {
            0 => "#000000",
            1 => "#800000",
            2 => "#008000",
            3 => "#808000",
            4 => "#000080",
            5 => "#800080",
            6 => "#008080",
            7 => "#c0c0c0",
            8 => "#808080",
            9 => "#ff0000",
            10 => "#00ff00",
            11 => "#ffff00",
            12 => "#0000ff",
            13 => "#ff00ff",
            14 => "#00ffff",
            _ => "#ffffff", // 15
        }
        .into();
    }
    if idx >= 232 {
        let gray = (idx - 232) * 10 + 8;
        return format!("#{:02x}{:02x}{:02x}", gray, gray, gray);
    }
    // 6x6x6 cube
    let idx = idx - 16;
    let r = idx / 36;
    let g = (idx % 36) / 6;
    let b = idx % 6;
    let to_rgb = |v: u8| if v == 0 { 0 } else { v * 40 + 55 };
    format!("#{:02x}{:02x}{:02x}", to_rgb(r), to_rgb(g), to_rgb(b))
}

fn framed_ascii(buffer: &Buffer) -> String {
    // Draw a frame around the rendered grid to expose padding/margins.
    let w = buffer.area().width as usize;
    let h = buffer.area().height as usize;
    let mut ascii = String::new();
    ascii.push('╔');
    ascii.push_str(&"═".repeat(w));
    ascii.push('╗');
    ascii.push('\n');
    for y in 0..h {
        ascii.push('║');
        for x in 0..w {
            let cell = buffer.cell((x as u16, y as u16)).unwrap();
            ascii.push(cell.symbol().chars().next().unwrap_or(' '));
        }
        ascii.push('║');
        ascii.push('\n');
    }
    ascii.push('╚');
    ascii.push_str(&"═".repeat(w));
    ascii.push('╝');
    ascii.push('\n');
    ascii
}

fn buffer_to_svg(buffer: &Buffer) -> String {
    let width = buffer.area().width;
    let height = buffer.area().height;
    let svg_w = (width * CELL_W + 2 * PADDING) as usize;
    let svg_h = (height * CELL_H + 2 * PADDING) as usize;
    let default_bg = "#000000";
    let default_fg = "#FFFFFF";

    // ASCII preview comment (includes frame to show margins)
    let ascii = framed_ascii(buffer);

    let mut out = String::new();
    out.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    out.push('\n');
    out.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{svg_w}" height="{svg_h}" viewBox="0 0 {svg_w} {svg_h}" font-family="JetBrains Mono, Fira Code, monospace" font-size="14" xml:space="preserve">"#,
    ));
    out.push('\n');
    out.push_str("  <!-- ASCII PREVIEW\n");
    for line in ascii.lines() {
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("  -->\n");
    out.push_str(&format!(
        r#"  <rect x="0" y="0" width="{svg_w}" height="{svg_h}" fill="{default_bg}" />"#
    ));
    out.push('\n');

    for y in 0..height {
        for x in 0..width {
            let cell = buffer.cell((x, y)).unwrap();
            let style = cell.style();
            let bg = color_to_hex(style.bg).unwrap_or_else(|| default_bg.to_string());
            let fg = color_to_hex(style.fg).unwrap_or_else(|| default_fg.to_string());
            let symbol = cell.symbol().chars().next().unwrap_or(' ');
            let px = PADDING + x * CELL_W;
            let py = PADDING + y * CELL_H;

            // background rect
            out.push_str(&format!(
                r#"  <rect x="{px}" y="{py}" width="{CELL_W}" height="{CELL_H}" fill="{bg}" />"#,
                px = px,
                py = py
            ));
            out.push('\n');

            if symbol != ' ' {
                let mut text = String::new();
                text.push(symbol);
                let font_weight = if style.add_modifier.contains(Modifier::BOLD) {
                    "700"
                } else {
                    "400"
                };
                let text_dec = if style.add_modifier.contains(Modifier::UNDERLINED) {
                    "text-decoration=\"underline\" "
                } else {
                    ""
                };
                let font_style = if style.add_modifier.contains(Modifier::ITALIC) {
                    "italic"
                } else {
                    "normal"
                };
                let escaped = escape_xml(&text);
                out.push_str(&format!(
                    r#"  <text x="{tx}" y="{ty}" fill="{fg}" font-weight="{font_weight}" font-style="{font_style}" {text_dec}>{escaped}</text>"#,
                    tx = px + 1,
                    ty = py + CELL_H - 4,
                    text_dec = text_dec,
                    escaped = escaped,
                ));
                out.push('\n');
            }
        }
    }

    out.push_str("</svg>\n");
    out
}

fn write_svg_sidecar(name: &str, buffer: &Buffer) {
    let svg = buffer_to_svg(buffer);
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("snapshots");
    let _ = fs::create_dir_all(&base);
    let path = base.join(format!("{name}.svg"));
    let _ = fs::write(path, svg);
}

#[allow(dead_code)]
pub fn test_settings() -> Settings {
    Settings {
        active_sessions_activity_rows: Some(1000),
        ..Settings::default()
    }
}

#[allow(dead_code)]
pub fn vm_with_events(
    title: &str,
    events: Vec<ah_core::TaskEvent>,
    viewport_rows: usize,
) -> AgentSessionViewModel {
    let mut vm = AgentSessionViewModel::new(
        title.to_string(),
        Vec::new(),
        viewport_rows,
        test_settings(),
        None,
        Theme::default(),
    );
    for event in events {
        vm.handle_task_event(&event);
    }
    vm
}

#[allow(dead_code)]
pub fn render_hits(
    vm: &AgentSessionViewModel,
    width: u16,
    height: u16,
) -> HitTestRegistry<AgentSessionMouseAction> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = HitTestRegistry::new();
    terminal
        .draw(|f| {
            ah_tui::view::agent_session_view::render_agent_session(
                f,
                vm,
                &ah_tui::Theme::default(),
                Some(&mut hits),
            )
        })
        .unwrap();
    hits
}

#[allow(dead_code)]
/// Find the top-left coordinate of the first occurrence of `needle` in the buffer.
pub fn find_text(buffer: &Buffer, needle: &str) -> Option<(u16, u16)> {
    for y in 0..buffer.area().height {
        let mut line = String::with_capacity(buffer.area().width as usize);
        for x in 0..buffer.area().width {
            let symbol = buffer.cell((x, y)).unwrap().symbol();
            line.push(symbol.chars().next().unwrap_or(' '));
        }
        if let Some(idx) = line.find(needle) {
            return Some((idx as u16, y));
        }
    }
    None
}

#[allow(dead_code)]
/// Convenience to fetch the style of a cell (used for color assertions).
pub fn style_at(buffer: &Buffer, x: u16, y: u16) -> Style {
    buffer.cell((x, y)).unwrap().style()
}

/// Convenience factory for key events used in keyboard-driven state transitions.
#[allow(dead_code)]
pub fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[allow(dead_code)]
pub fn key_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[allow(dead_code)]
pub fn shift_tab() -> KeyEvent {
    KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)
}

/// Apply a sequence of key events to the view model using its public input handler.
#[allow(dead_code)]
pub fn apply_keys(vm: &mut AgentSessionViewModel, keys: &[KeyEvent]) {
    for key in keys {
        vm.handle_key_with_minor_modes(*key);
    }
}

/// Standard viewport sizes to exercise layout differences.
#[allow(dead_code)]
pub const STANDARD_VIEWPORTS: &[(u16, u16)] = &[(80, 24), (100, 28), (120, 32)];

/// Themes exercised by rendering tests. Order matters for stable snapshot naming.
#[allow(dead_code)]
#[allow(clippy::type_complexity)]
pub const STANDARD_THEMES: &[(&str, fn() -> Theme)] = &[
    ("default", Theme::default),
    ("high_contrast", Theme::high_contrast),
];
