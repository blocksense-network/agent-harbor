// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent Activity TUI view (mock mode).

use crate::view::Theme;
use crate::view::draft_card::DraftCardLayout;
use crate::view::hit_test::HitTestRegistry;
use crate::view_model::agent_session_model::{
    AgentSessionMouseAction, AgentSessionViewModel, ControlFocus, FocusArea, OutputModalKind,
};
use crate::view_model::task_execution::AgentActivityRow;
#[cfg(test)]
use crate::view_model::task_execution::{PipelineMeta, PipelineStatus};
#[cfg(test)]
use ah_domain_types::task::ToolStatus;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

const CARD_HEADER_HEIGHT: u16 = 3;
const CARD_BOTTOM_HEIGHT: u16 = 1;
const CARD_HORIZONTAL_PADDING: u16 = 1;
const CONTENT_MARGIN_X: u16 = 1; // leave a single column gutter; combined with spine = ~2-space mockup padding
const MIN_SEGMENT_WIDTH: usize = 3;

// GUI Component Architecture - Nested Controls with Measure/Layout/Render
#[derive(Debug, Clone, Copy)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

pub trait Measureable {
    fn measure(&self, available_width: u16) -> Size;
}

pub trait Renderable {
    fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme);
}

// Auto-implement Component for anything that is both Measureable and Renderable
pub trait Component: Measureable + Renderable {}
impl<T> Component for T where T: Measureable + Renderable {}

// Direct drawing utilities
fn draw_char_at(frame: &mut Frame, x: u16, y: u16, ch: char, color: Color) {
    if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
        cell.set_char(ch);
        cell.set_fg(color);
    }
}

fn draw_string_at(frame: &mut Frame, x: u16, y: u16, s: &str, color: Color) {
    for (i, ch) in s.chars().enumerate() {
        if let Some(cell) = frame.buffer_mut().cell_mut((x + i as u16, y)) {
            cell.set_char(ch);
            cell.set_fg(color);
        }
    }
}

// TitleBox Component - Floating title that straddles the border
#[derive(Debug, Clone)]
pub struct TitleBox {
    pub label: String,
}

impl Measureable for TitleBox {
    fn measure(&self, _available_width: u16) -> Size {
        // Width = "╭─┤ " + label + " ├" = 4 + label.len() + 2
        let width = 6 + self.label.len() as u16;
        Size { width, height: 3 } // Top cap, main line, bottom cap
    }
}

impl Renderable for TitleBox {
    fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let border_color = theme.border;

        // Top cap
        draw_string_at(frame, area.x, area.y, "╭", border_color);
        draw_string_at(
            frame,
            area.x + 1,
            area.y,
            &"─".repeat(self.label.len() + 2),
            border_color,
        );
        draw_string_at(
            frame,
            area.x + self.label.len() as u16 + 3,
            area.y,
            "╮",
            border_color,
        );

        // Main line (straddles the card border)
        draw_string_at(frame, area.x, area.y + 1, "╭─┤ ", border_color);
        draw_string_at(frame, area.x + 3, area.y + 1, &self.label, theme.text);
        draw_string_at(
            frame,
            area.x + 3 + self.label.len() as u16,
            area.y + 1,
            " ├",
            border_color,
        );

        // Bottom cap
        draw_string_at(frame, area.x + 1, area.y + 2, "╰", border_color);
        draw_string_at(
            frame,
            area.x + 2,
            area.y + 2,
            &"─".repeat(self.label.len() + 2),
            border_color,
        );
        draw_string_at(
            frame,
            area.x + self.label.len() as u16 + 4,
            area.y + 2,
            "╯",
            border_color,
        );
    }
}

// ControlBox Component - Segmented control area
#[derive(Debug, Clone)]
pub struct ControlSegment {
    pub content: String,
    pub style: Style,
    pub width: usize,
}

impl From<&str> for ControlSegment {
    fn from(s: &str) -> Self {
        ControlSegment {
            content: s.to_string(),
            style: Style::default(),
            width: s.chars().count(),
        }
    }
}

impl From<String> for ControlSegment {
    fn from(s: String) -> Self {
        ControlSegment {
            width: s.chars().count(),
            content: s,
            style: Style::default(),
        }
    }
}

impl ControlSegment {
    pub fn new(content: impl Into<String>, style: Style) -> Self {
        let content = content.into();
        let width = UnicodeWidthStr::width(content.as_str()).max(MIN_SEGMENT_WIDTH);
        Self {
            content,
            width,
            style,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ControlBox {
    pub segments: Vec<ControlSegment>,
    pub focused_segment: Option<ControlFocus>,
}

impl Measureable for ControlBox {
    fn measure(&self, _available_width: u16) -> Size {
        let mut width = 1; // Leading ┤
        for (i, segment) in self.segments.iter().enumerate() {
            if i > 0 {
                width += 1; // │ separator
            }
            let segment_width = segment.width + 2; // space S space
            width += segment_width as u16;
        }
        width += 1; // Trailing ├
        Size { width, height: 1 }
    }
}

impl Renderable for ControlBox {
    fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // This default implementation is kept for safety but Card::render now handles it directly
        // to ensure proper integration with the card border.
        let mut x = area.x;
        draw_char_at(frame, x, area.y, '├', theme.border);
        x += 1;

        for (i, segment) in self.segments.iter().enumerate() {
            if i > 0 {
                draw_char_at(frame, x, area.y, '│', theme.border);
                x += 1;
            }

            let style = if Some(ControlFocus::from_index(i)) == self.focused_segment {
                Style::default().fg(theme.primary)
            } else {
                segment.style
            };

            let color = style.fg.unwrap_or(theme.muted);

            draw_string_at(frame, x, area.y, &format!(" {} ", segment.content), color);
            x += segment.width as u16 + 2;
        }

        draw_char_at(frame, x, area.y, '├', theme.border);
    }
}

// ContentArea Component - Multi-line content with borders
#[derive(Debug, Clone)]
pub struct ContentLine {
    pub text: String,
    pub style: Option<Style>,
}

impl ContentLine {
    pub fn plain<S: Into<String>>(text: S) -> Self {
        Self {
            text: text.into(),
            style: None,
        }
    }

    pub fn styled<S: Into<String>>(text: S, style: Style) -> Self {
        Self {
            text: text.into(),
            style: Some(style),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContentArea {
    pub lines: Vec<ContentLine>,
}

impl Measureable for ContentArea {
    fn measure(&self, available_width: u16) -> Size {
        let height = self.lines.len() as u16;
        Size {
            width: available_width,
            height,
        }
    }
}

impl Renderable for ContentArea {
    fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        for (i, line) in self.lines.iter().enumerate() {
            let y = area.y + i as u16;
            if y >= area.y + area.height {
                break;
            }

            // Left border
            draw_char_at(frame, area.x, y, '│', theme.border);
            draw_char_at(frame, area.x + 1, y, ' ', theme.bg);

            // Content
            let content_x = area.x + 2;
            let max_content_width = area.width.saturating_sub(4); // borders and padding

            let truncated = if line.text.len() > max_content_width as usize {
                format!(
                    "{}...",
                    &line.text[..max_content_width.saturating_sub(3) as usize]
                )
            } else {
                line.text.clone()
            };

            let line_style =
                line.style.unwrap_or_else(|| Style::default().fg(theme.text).bg(theme.bg));
            for (idx, ch) in truncated.chars().enumerate() {
                if let Some(cell) = frame.buffer_mut().cell_mut((content_x + idx as u16, y)) {
                    cell.set_char(ch);
                    cell.set_style(line_style);
                }
            }

            // Fill remaining space
            let remaining = max_content_width.saturating_sub(truncated.len() as u16);
            for j in 0..remaining {
                draw_char_at(
                    frame,
                    content_x + truncated.len() as u16 + j,
                    y,
                    ' ',
                    theme.bg,
                );
            }

            // Right border
            draw_char_at(frame, area.x + area.width - 2, y, ' ', theme.bg);
            draw_char_at(frame, area.x + area.width - 1, y, '│', theme.border);
        }
    }
}

// Card Container - Orchestrates the layout of nested components
#[derive(Debug, Clone)]
pub struct Card {
    pub title_box: Option<TitleBox>,
    pub control_box: ControlBox,
    pub content_area: ContentArea,
}

impl Measureable for Card {
    fn measure(&self, available_width: u16) -> Size {
        let inner_width = available_width.saturating_sub(2 * CARD_HORIZONTAL_PADDING + 4); // borders

        // Measure components
        let title_size = self.title_box.as_ref().map_or(
            Size {
                width: 0,
                height: 0,
            },
            |tb| tb.measure(inner_width),
        );
        let control_size = self.control_box.measure(inner_width);
        let content_size = self
            .content_area
            .measure(inner_width.saturating_sub(title_size.width + control_size.width));

        // Total width is determined by the components
        let total_width = CARD_HORIZONTAL_PADDING * 2
            + 4u16
            + title_size.width
            + control_size.width
            + content_size.width;
        let total_height = CARD_HEADER_HEIGHT + content_size.height + CARD_BOTTOM_HEIGHT;

        Size {
            width: total_width,
            height: total_height,
        }
    }
}

impl Renderable for Card {
    fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let w = area.width as usize;
        let pad_left = CARD_HORIZONTAL_PADDING as usize;
        let pad_right = CARD_HORIZONTAL_PADDING as usize;

        // Paint the full card area with the surface background so borders don't inherit the terminal bg.
        let buf = frame.buffer_mut();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_bg(theme.bg);
                }
            }
        }

        let border_color = theme.border;
        let top_y = area.y;
        let main_y = area.y + 1;
        let bottom_y = area.y + 2;

        let mut x = area.x + pad_left as u16;

        // 1) Render the rounded corner fragment
        // Top: "  "
        // Mid: "╭─"
        // Bot: "│ "
        draw_string_at(frame, x, top_y, "  ", border_color);
        draw_string_at(frame, x, main_y, "╭─", border_color);
        draw_string_at(frame, x, bottom_y, "│ ", border_color);
        x += 2;

        // 2) & 3) Render title box (if present)
        if let Some(ref title_box) = &self.title_box {
            let label_len = width_of(&title_box.label);
            let box_width = label_len + 4; // "┤ " + label + " ├"

            // Top: ╭─────╮
            draw_string_at(frame, x, top_y, "╭", border_color);
            draw_string_at(
                frame,
                x + 1,
                top_y,
                &"─".repeat(label_len + 2),
                border_color,
            );
            draw_string_at(
                frame,
                x + 1 + (label_len + 2) as u16,
                top_y,
                "╮",
                border_color,
            );

            // Mid: ┤ LABEL ├
            draw_string_at(frame, x, main_y, "┤ ", border_color);
            draw_string_at(frame, x + 2, main_y, &title_box.label, theme.text);
            draw_string_at(frame, x + 2 + label_len as u16, main_y, " ├", border_color);

            // Bot: ╰─────╯
            draw_string_at(frame, x, bottom_y, "╰", border_color);
            draw_string_at(
                frame,
                x + 1,
                bottom_y,
                &"─".repeat(label_len + 2),
                border_color,
            );
            draw_string_at(
                frame,
                x + 1 + (label_len + 2) as u16,
                bottom_y,
                "╯",
                border_color,
            );

            x += box_width as u16;
        }

        // 4) Render connecting border to control box
        // Calculate available width for the dash
        let control_width = self.control_box.measure(area.width).width as usize;
        // Total used so far: x - area.x
        // Total needed at end: control_width + 2 (for "─╮") + pad_right
        // Available for dash: w - (x - area.x) - (control_width + 2) - pad_right
        let current_offset = (x - area.x) as usize;
        let dash_len = w
            .saturating_sub(current_offset)
            .saturating_sub(control_width)
            .saturating_sub(2) // "─╮"
            .saturating_sub(pad_right);

        // Top: " " * dash_len
        draw_string_at(frame, x, top_y, &" ".repeat(dash_len), border_color);
        // Mid: "─" * dash_len
        draw_string_at(frame, x, main_y, &"─".repeat(dash_len), border_color);
        // Bot: " " * dash_len
        draw_string_at(frame, x, bottom_y, &" ".repeat(dash_len), border_color);

        x += dash_len as u16;

        // 5) Render the control box and its segments
        // Start: Top "╭", Mid "┤", Bot "╰"
        draw_string_at(frame, x, top_y, "╭", border_color);
        draw_string_at(frame, x, main_y, "┤", border_color);
        draw_string_at(frame, x, bottom_y, "╰", border_color);
        x += 1;

        for (i, segment) in self.control_box.segments.iter().enumerate() {
            if i > 0 {
                // Separator: Top "┬", Mid "│", Bot "┴"
                draw_string_at(frame, x, top_y, "┬", border_color);
                draw_string_at(frame, x, main_y, "│", border_color);
                draw_string_at(frame, x, bottom_y, "┴", border_color);
                x += 1;
            }

            let seg_width = segment.width;
            let color = if Some(ControlFocus::from_index(i)) == self.control_box.focused_segment {
                theme.primary
            } else {
                segment.style.fg.unwrap_or(theme.muted)
            };

            // Content block: Top "──...", Mid " S ", Bot "──..."
            // We pad with 1 space on each side: " S "
            let block_width = seg_width + 2;

            draw_string_at(frame, x, top_y, &"─".repeat(block_width), border_color);

            draw_string_at(frame, x, main_y, " ", border_color);
            draw_string_at(frame, x + 1, main_y, &segment.content, color);
            draw_string_at(frame, x + 1 + seg_width as u16, main_y, " ", border_color);

            draw_string_at(frame, x, bottom_y, &"─".repeat(block_width), border_color);

            x += block_width as u16;
        }

        // End: Top "╮", Mid "├", Bot "╯"
        draw_string_at(frame, x, top_y, "╮", border_color);
        draw_string_at(frame, x, main_y, "├", border_color);
        draw_string_at(frame, x, bottom_y, "╯", border_color);
        x += 1;

        // 6) Render the short border and the rounded corner
        // Top: " "
        // Mid: "─╮"
        // Bot: " │"
        draw_string_at(frame, x, top_y, " ", border_color);
        draw_string_at(frame, x, main_y, "─╮", border_color);
        draw_string_at(frame, x, bottom_y, " │", border_color);
        // x += 2; // Done with header

        // Render content area
        let content_y = area.y + CARD_HEADER_HEIGHT;
        let content_height = area
            .height
            .saturating_sub(CARD_HEADER_HEIGHT.saturating_add(CARD_BOTTOM_HEIGHT));
        if content_height > 0 {
            let content_area_rect = Rect {
                x: area.x + pad_left as u16,
                y: content_y,
                width: (w - pad_left - pad_right) as u16,
                height: content_height,
            };
            self.content_area.render(frame, content_area_rect, theme);
        }

        // Bottom border
        let footer_y = area.y + area.height - 1;
        draw_string_at(frame, area.x + pad_left as u16, footer_y, "╰", border_color);
        let inner_width = w.saturating_sub(pad_left + pad_right + 2);
        draw_string_at(
            frame,
            area.x + pad_left as u16 + 1,
            footer_y,
            &"─".repeat(inner_width),
            border_color,
        );
        draw_string_at(
            frame,
            area.x + (w - pad_right - 1) as u16,
            footer_y,
            "╯",
            border_color,
        );
    }
}

fn width_of(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn context_color(pct: u8, theme: &Theme) -> Color {
    if pct >= 95 {
        theme.error
    } else if pct >= 80 {
        theme.warning
    } else {
        theme.text
    }
}

/// Format a shell pipeline with semantic coloring and optional size metadata.
#[cfg(test)]
fn format_command_line(
    cmd: &str,
    theme: &Theme,
    status: Option<ToolStatus>,
    size: Option<&str>,
    dimmed: bool,
    pipeline: Option<&PipelineMeta>,
) -> Line<'static> {
    let mut spans = Vec::new();
    let segments: Vec<&str> = cmd.split('|').collect();
    let pipeline_segments = pipeline.map(|p| p.segments.as_slice()).unwrap_or(&[]);

    let mut global_size_available = size;
    for (idx, seg) in segments.iter().enumerate() {
        let trimmed = seg.trim();
        if trimmed.is_empty() {
            continue;
        }
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }

        let seg_status: Option<PipelineStatus> =
            pipeline_segments.get(idx).and_then(|p| p.status).or_else(|| {
                status.map(|s| match s {
                    ToolStatus::Failed => PipelineStatus::Failed,
                    ToolStatus::Completed => PipelineStatus::Success,
                    ToolStatus::Started => PipelineStatus::Success,
                })
            });

        let cmd_color = match (seg_status, dimmed) {
            (_, true) => theme.dim_text,
            (Some(PipelineStatus::Failed), _) => theme.error,
            (Some(PipelineStatus::Success), _) => theme.accent,
            (Some(PipelineStatus::Skipped), _) => theme.muted,
            _ => theme.text,
        };

        for (word_idx, word) in words.iter().enumerate() {
            let styled = if word_idx == 0 {
                Span::styled(
                    (*word).to_string(),
                    Style::default().fg(cmd_color).add_modifier(Modifier::BOLD),
                )
            } else if word.starts_with('-') {
                Span::styled((*word).to_string(), Style::default().fg(theme.accent))
            } else {
                Span::styled(
                    (*word).to_string(),
                    Style::default().fg(if dimmed { theme.dim_text } else { theme.text }),
                )
            };
            spans.push(styled);
            spans.push(Span::raw(" "));
        }

        // Attach per-segment output size if provided either in pipeline metadata or global size.
        let segment_size =
            pipeline_segments.get(idx).and_then(|p| p.output_size.as_deref()).or_else(|| {
                if let Some(sz) = global_size_available {
                    // Use the global size only once to avoid duplicating labels across segments.
                    global_size_available = None;
                    Some(sz)
                } else {
                    None
                }
            });
        if let Some(size_txt) = segment_size {
            spans.push(Span::styled(
                size_txt.to_string(),
                Style::default().fg(if dimmed { theme.dim_text } else { theme.muted }),
            ));
            spans.push(Span::raw(" "));
        }

        if idx + 1 != segments.len() {
            spans.push(Span::styled(
                "|",
                Style::default().fg(theme.warning).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(" "));
        }
    }

    Line::from(spans)
}

#[derive(Debug, Clone)]
struct ControlBoxLines {
    top: String,
    mid: Vec<Span<'static>>,
    bottom: String,
    width: usize,
    segment_offsets: Vec<usize>,
    segment_widths: Vec<usize>,
}

fn build_control_box(segments: &[ControlSegment], border_style: Style) -> ControlBoxLines {
    let widths: Vec<usize> = segments.iter().map(|s| s.width).collect();
    let mut top = String::from("╭");
    let mut bottom = String::from("╰");
    let mut segment_offsets = Vec::new();
    let mut cursor = 1; // after the leading corner

    for (idx, width) in widths.iter().enumerate() {
        if idx > 0 {
            top.push('┬');
            bottom.push('┴');
            cursor += 1;
        }
        top.push_str(&"─".repeat(*width));
        bottom.push_str(&"─".repeat(*width));
        segment_offsets.push(cursor);
        cursor += *width;
    }
    top.push('╮');
    bottom.push('╯');

    let mut mid = Vec::new();
    mid.push(Span::styled("┤", border_style));
    for (idx, seg) in segments.iter().enumerate() {
        let content_width = width_of(&seg.content);
        let total_pad = seg.width.saturating_sub(content_width);
        let pad_left = total_pad / 2;
        let pad_right = total_pad - pad_left;
        let padded = format!(
            "{}{}{}",
            " ".repeat(pad_left),
            seg.content,
            " ".repeat(pad_right)
        );

        mid.push(Span::styled(padded, seg.style));
        if idx + 1 != segments.len() {
            mid.push(Span::styled("│", border_style));
        }
    }
    mid.push(Span::styled("├", border_style));

    let total_width = top.chars().count();
    ControlBoxLines {
        top,
        mid,
        bottom,
        width: total_width,
        segment_offsets,
        segment_widths: widths,
    }
}

fn format_activity_row(row: &AgentActivityRow, theme: &Theme, _dimmed: bool) -> Card {
    match row {
        AgentActivityRow::AgentThought { thought } => Card {
            title_box: Some(TitleBox {
                label: "THOUGHT".into(),
            }),
            control_box: ControlBox {
                segments: vec!["❐".into(), "▼".into(), "14:22".into()],
                focused_segment: None,
            },
            content_area: ContentArea {
                lines: vec![ContentLine::plain(thought.clone())],
            },
        },
        AgentActivityRow::AgentEdit {
            file_path,
            lines_added,
            lines_removed,
            description,
        } => {
            let mut content_lines = vec![ContentLine::plain(format!(
                "{} +{} -{}",
                file_path, lines_added, lines_removed
            ))];
            if let Some(desc) = description {
                for line in desc.lines() {
                    let style = if line.trim_start().starts_with('+') {
                        Some(Style::default().fg(theme.accent))
                    } else if line.trim_start().starts_with('-') {
                        Some(Style::default().fg(theme.error))
                    } else {
                        None
                    };
                    content_lines.push(ContentLine {
                        text: line.to_string(),
                        style,
                    });
                }
            }

            Card {
                title_box: Some(TitleBox {
                    label: "EDITED".into(),
                }),
                control_box: ControlBox {
                    segments: vec!["❐".into(), "▼".into(), "14:22".into()],
                    focused_segment: None,
                },
                content_area: ContentArea {
                    lines: content_lines,
                },
            }
        }
        AgentActivityRow::AgentRead { file_path, range } => {
            let mut content_lines = vec![ContentLine::plain(file_path.clone())];
            if let Some(r) = range {
                let dim = Some(Style::default().fg(theme.dim_text));
                content_lines.push(ContentLine {
                    text: format!("({})", r),
                    style: dim,
                });
                content_lines.push(ContentLine {
                    text: format!("lines {}", r),
                    style: dim,
                });
            }

            Card {
                title_box: Some(TitleBox {
                    label: "READ".into(),
                }),
                control_box: ControlBox {
                    segments: vec!["❐".into(), "▼".into(), "14:22".into()],
                    focused_segment: None,
                },
                content_area: ContentArea {
                    lines: content_lines,
                },
            }
        }
        AgentActivityRow::AgentDeleted {
            file_path,
            lines_removed,
        } => Card {
            title_box: Some(TitleBox {
                label: "DELETED".into(),
            }),
            control_box: ControlBox {
                segments: vec!["❐".into(), "▼".into(), "14:22".into()],
                focused_segment: None,
            },
            content_area: ContentArea {
                lines: vec![ContentLine::plain(format!(
                    "{} -{}",
                    file_path, lines_removed
                ))],
            },
        },
        AgentActivityRow::UserInput {
            author,
            content,
            confirmed,
            timestamp,
        } => {
            let label = if author.eq_ignore_ascii_case("you") {
                "YOU WROTE".to_string()
            } else {
                format!("{} WROTE", author.to_ascii_uppercase())
            };

            let mut segments: Vec<ControlSegment> = vec!["❐".into(), "▼".into(), "14:22".into()];
            if !confirmed {
                // Render spinner for unconfirmed state
                if let Some(spinner) = theme.spinners.get("awaiting_confirmation") {
                    let frame = spinner.current_frame(timestamp.elapsed());
                    segments.push(ControlSegment {
                        content: frame.text.clone(),
                        style: Style::default().fg(spinner.color),
                        width: spinner.max_width(),
                    });
                } else {
                    segments.push("...".into());
                }
            }

            Card {
                title_box: Some(TitleBox { label }),
                control_box: ControlBox {
                    segments,
                    focused_segment: None,
                },
                content_area: ContentArea {
                    lines: vec![ContentLine::plain(content.clone())],
                },
            }
        }
        AgentActivityRow::ToolUse {
            tool_name,
            tool_execution_id: _,
            last_line,
            completed,
            status: _,
            pipeline: _,
        } => {
            let command_name = tool_name.split_whitespace().next().unwrap_or(tool_name);
            let mut title_label = format!("RAN {}", command_name);
            if !*completed {
                title_label.push_str(" ■");
            }

            let segments = vec!["❐".into(), "▼".into(), "00:45".into()];

            let mut content_lines = vec![format!("$ {}", tool_name)];
            if let Some(line) = last_line {
                content_lines.push(line.clone());
            }

            Card {
                title_box: Some(TitleBox { label: title_label }),
                control_box: ControlBox {
                    segments,
                    focused_segment: None,
                },
                content_area: ContentArea {
                    lines: content_lines.into_iter().map(ContentLine::plain).collect(),
                },
            }
        }
    }
}

fn card_height(data: &Card) -> u16 {
    data.measure(u16::MAX).height
}

fn render_spine(frame: &mut Frame, area: Rect, theme: &Theme) {
    // Spines are purely spacing gutters in the mockup; render padding only (no border glyphs).
    let block = Block::default().borders(Borders::NONE).style(Style::default().bg(theme.bg));
    frame.render_widget(block, area);
}

fn split_with_spines(area: Rect) -> [Rect; 3] {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(0),
            Constraint::Min(1),
            Constraint::Length(0),
        ])
        .split(area);
    [chunks[0], chunks[1], chunks[2]]
}

#[allow(clippy::too_many_arguments)]
fn render_card(
    frame: &mut Frame,
    area: Rect,
    data: Card,
    theme: &Theme,
    hero: bool,
    dimmed: bool,
    selected: bool,
    control_focus: Option<ControlFocus>,
    hits: Option<&mut HitTestRegistry<AgentSessionMouseAction>>,
    index: Option<usize>,
) {
    // Update the card's control focus
    let mut card = data;
    card.control_box.focused_segment = control_focus;

    // Create themed version for rendering
    let mut render_theme = theme.clone();
    render_theme.border = if hero {
        theme.accent
    } else if selected {
        theme.border_focused
    } else if dimmed {
        theme.dim_border
    } else {
        theme.border
    };
    render_theme.surface = theme.surface;
    render_theme.text = if dimmed { theme.dim_text } else { theme.text };

    // Render the card component
    card.render(frame, area, &render_theme);

    // Register hit zones for interactive control segments
    if let (Some(hits), Some(index)) = (hits, index) {
        let control_width = card.control_box.measure(area.width).width as usize;
        let control_start_x = area.x + (area.width as usize - control_width) as u16;

        let mut seg_x = control_start_x + 1; // Skip the opening ┤
        for (i, segment) in card.control_box.segments.iter().enumerate() {
            if i > 0 {
                seg_x += 1; // Skip the │ separator
            }

            let segment_width = (segment.width + 2) as u16;
            let hit_area = Rect {
                x: seg_x,
                y: area.y + 1, // Main line
                width: segment_width,
                height: 1,
            };

            let action = match i {
                0 => AgentSessionMouseAction::Copy(index),
                1 => AgentSessionMouseAction::Expand(index),
                _ => continue, // Skip timestamp segment
            };

            hits.register(hit_area, action);
            seg_x += segment_width;
        }
    }
}
fn render_task_entry_card(
    frame: &mut Frame,
    area: Rect,
    vm: &AgentSessionViewModel,
    theme: &Theme,
    is_selected: bool,
) -> DraftCardLayout {
    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(Style::default().bg(theme.bg)), area);
    let instr_theme = theme;
    let border_color = if is_selected {
        theme.primary
    } else {
        theme.border_focused
    };
    let pad_style = Style::default().bg(theme.bg);
    let border_style = Style::default().fg(border_color);

    let w = area.width as usize;
    let pad_left = CARD_HORIZONTAL_PADDING as usize;
    let pad_right = CARD_HORIZONTAL_PADDING as usize;

    // Top Border (Simple)
    let top_lines = vec![Line::from(vec![
        Span::styled(" ".repeat(pad_left), pad_style),
        Span::styled("╭", border_style),
        Span::styled(
            "─".repeat(w.saturating_sub(pad_left + pad_right + 2)),
            border_style,
        ),
        Span::styled("╮", border_style),
        Span::styled(" ".repeat(pad_right), pad_style),
    ])];
    let top_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(Text::from(top_lines)).style(Style::default().bg(theme.bg)),
        top_area,
    );

    // Body Area
    let body_height = area.height.saturating_sub(1 + 3); // Top (1) + Bottom (3)
    let body_area = Rect {
        x: area.x + pad_left as u16,
        y: area.y + 1,
        width: area.width.saturating_sub((pad_left + pad_right) as u16),
        height: body_height,
    };

    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_style(border_style)
        .style(Style::default().bg(theme.bg));
    frame.render_widget(block, body_area);

    let content_area = Rect {
        x: body_area.x + 2, // Border (1) + Space (1)
        y: body_area.y,
        width: body_area.width.saturating_sub(4), // 2 sides * (Border + Space)
        height: body_area.height,
    };

    frame.render_widget(&vm.task_entry().description, content_area);

    // Bottom Border with Controls
    // Left: [ 🤖 MODELS ]
    // Right: [ ⏎ GO │ ≡ OPTIONS ]

    let left_segments = vec![ControlSegment::new(
        " 🤖 MODELS ",
        Style::default().fg(border_color).add_modifier(Modifier::BOLD),
    )];
    let left_box = build_control_box(&left_segments, border_style);

    let right_segments = vec![
        ControlSegment::new(
            " ⏎ GO ",
            Style::default().fg(instr_theme.accent).add_modifier(Modifier::BOLD),
        ),
        ControlSegment::new(
            " ≡ OPTIONS ",
            Style::default().fg(instr_theme.primary).add_modifier(Modifier::BOLD),
        ),
    ];
    let right_box = build_control_box(&right_segments, border_style);

    // Calculate spacing
    let inner_w = w.saturating_sub(pad_left + pad_right + 2); // Inside vertical borders
    // We need to fit left_box and right_box inside the bottom border line
    // Layout: ╰─┤ MODELS ├──────┤ GO │ OPTIONS ├─╯

    // Actually, build_control_box returns the full box string including caps.
    // But we want to embed them in the border line.
    // The mockup uses `draw_control_box` which returns top/mid/bot strings.
    // We need to construct 3 lines for the bottom area:
    // 1. Top caps of buttons (inside the card body effectively, or just below it?)
    //    Mockup says: "Line N-1 (Inside - Top Caps of Buttons)"
    //    "Line N (Bottom Border with Buttons)"
    //    "Line N+1 (Outside - Bottom Caps of Buttons)"

    // So the bottom area is 3 lines high.

    let spacer_len = inner_w.saturating_sub(left_box.width + right_box.width + 2);

    // Line 0: Top Caps (inside)
    let line0 = vec![
        Span::styled(" ".repeat(pad_left), pad_style),
        Span::styled("│ ", border_style), // Left border
        Span::styled(left_box.top.clone(), border_style),
        Span::styled(" ".repeat(spacer_len), pad_style), // Spacer is empty space inside
        Span::styled(right_box.top.clone(), border_style),
        Span::styled(" │", border_style), // Right border
        Span::styled(" ".repeat(pad_right), pad_style),
    ];

    // Line 1: Main Border with Buttons
    let mut line1 = vec![
        Span::styled(" ".repeat(pad_left), pad_style),
        Span::styled("╰─", border_style), // Start marker
    ];
    line1.extend(left_box.mid.clone());
    line1.push(Span::styled("─".repeat(spacer_len), border_style)); // Border line connecting boxes
    line1.extend(right_box.mid.clone());
    line1.push(Span::styled("─╯", border_style)); // End marker
    line1.push(Span::styled(" ".repeat(pad_right), pad_style));

    // Line 2: Bottom Caps (outside)
    let line2 = vec![
        Span::styled(" ".repeat(pad_left), pad_style),
        Span::styled("  ", pad_style), // Below start marker
        Span::styled(left_box.bottom.clone(), border_style),
        Span::styled(" ".repeat(spacer_len), pad_style), // Below spacer
        Span::styled(right_box.bottom.clone(), border_style),
        Span::styled("  ", pad_style), // Below end marker
        Span::styled(" ".repeat(pad_right), pad_style),
    ];

    let bottom_area = Rect {
        x: area.x,
        y: area.y + 1 + body_height,
        width: area.width,
        height: 3,
    };

    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(line0),
            Line::from(line1),
            Line::from(line2),
        ]))
        .style(Style::default().bg(theme.bg)),
        bottom_area,
    );

    // Calculate button Rects for hit testing
    let buttons_y = bottom_area.y + 1;
    let left_box_start_x = area.x + pad_left as u16 + 2; // after "╰─"

    // Model button is the first segment of left_box
    let model_button = if !left_box.segment_widths.is_empty() {
        Rect {
            x: left_box_start_x + left_box.segment_offsets[0] as u16,
            y: buttons_y,
            width: left_box.segment_widths[0] as u16,
            height: 1,
        }
    } else {
        Rect::default()
    };

    let right_box_start_x = left_box_start_x + left_box.width as u16 + spacer_len as u16;

    // Go button is first segment of right_box
    let go_button = if !right_box.segment_widths.is_empty() {
        Rect {
            x: right_box_start_x + right_box.segment_offsets[0] as u16,
            y: buttons_y,
            width: right_box.segment_widths[0] as u16,
            height: 1,
        }
    } else {
        Rect::default()
    };

    // Options button is second segment of right_box
    let advanced_options_button = if right_box.segment_widths.len() > 1 {
        Rect {
            x: right_box_start_x + right_box.segment_offsets[1] as u16,
            y: buttons_y,
            width: right_box.segment_widths[1] as u16,
            height: 1,
        }
    } else {
        Rect::default()
    };

    DraftCardLayout {
        textarea: content_area,
        repository_button: Rect::default(),
        branch_button: Rect::default(),
        model_button,
        go_button,
        advanced_options_button,
    }
}
#[allow(clippy::needless_option_as_deref)]
pub fn render_agent_session(
    frame: &mut Frame,
    vm: &AgentSessionViewModel,
    theme: &Theme,
    mut hits: Option<&mut HitTestRegistry<AgentSessionMouseAction>>,
) {
    if let Some(h) = hits.as_deref_mut() {
        h.clear();
    }

    let area = frame.area();
    // Paint the entire viewport with the base color (matches `scripts/tui_mockup.py`).
    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(Style::default().bg(theme.bg)), area);
    let inner = Rect {
        x: area.x.saturating_add(CONTENT_MARGIN_X),
        y: area.y,
        width: area.width.saturating_sub(CONTENT_MARGIN_X.saturating_mul(2)),
        height: area.height,
    };

    let mut instruction_height = vm.task_entry().height.saturating_add(4).max(8);
    let hero_idx = vm.activity().len().saturating_sub(1);
    let hero_row = vm.activity().get(hero_idx);
    let hero_height = hero_row
        .map(|row| card_height(&format_activity_row(row, theme, false)))
        .unwrap_or(6);

    let header_height = 2u16;
    let footer_height = 1u16;
    let min_timeline = CARD_HEADER_HEIGHT + CARD_BOTTOM_HEIGHT + 1;
    let fixed_heights = header_height + hero_height + footer_height;
    if fixed_heights + instruction_height + min_timeline > inner.height {
        let overflow = fixed_heights + instruction_height + min_timeline - inner.height;
        instruction_height = instruction_height.saturating_sub(overflow).max(3);
    }
    let timeline_height = inner
        .height
        .saturating_sub(fixed_heights + instruction_height)
        .max(min_timeline);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height),
            Constraint::Length(timeline_height),
            Constraint::Length(hero_height),
            Constraint::Length(instruction_height),
            Constraint::Length(footer_height),
        ])
        .split(inner);

    // Fill all layout areas with background color to prevent unrendered gaps
    for area in layout.iter() {
        frame.render_widget(Block::default().style(Style::default().bg(theme.bg)), *area);
    }

    // Header
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            "Agent Activity",
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(vm.title(), Style::default().fg(theme.muted)),
    ]))
    .style(Style::default().bg(theme.bg));
    frame.render_widget(header, layout[0]);

    // Timeline area
    let [tl_left, tl_center_raw, tl_right] = split_with_spines(layout[1]);
    render_spine(frame, tl_left, theme);
    render_spine(frame, tl_right, theme);
    let tl_center = Rect {
        x: tl_center_raw.x.saturating_add(1),
        y: tl_center_raw.y,
        width: tl_center_raw.width.saturating_sub(2),
        height: tl_center_raw.height,
    };

    let visible_indices: Vec<usize> =
        vm.visible_indices().into_iter().filter(|idx| *idx != hero_idx).collect();

    let mut card_positions: Vec<(usize, Rect)> = Vec::new();
    let mut y = tl_center.y;
    for idx in visible_indices.iter() {
        let row = &vm.activity()[*idx];
        let dimmed = vm.fork_index().map(|fork| *idx >= fork).unwrap_or(false) || !vm.auto_follow();
        let data = format_activity_row(row, theme, dimmed);
        let height = card_height(&data);
        if y + height > tl_center.y + tl_center.height {
            break;
        }
        let card_area = Rect {
            x: tl_center.x,
            y,
            width: tl_center.width,
            height,
        };
        render_card(
            frame,
            card_area,
            data,
            theme,
            false,
            dimmed,
            vm.selected() == Some(*idx),
            if vm.selected() == Some(*idx) {
                match vm.focus() {
                    FocusArea::Control(ControlFocus::Copy) => Some(ControlFocus::Copy),
                    FocusArea::Control(ControlFocus::Expand) => Some(ControlFocus::Expand),
                    FocusArea::Control(ControlFocus::Stop) => Some(ControlFocus::Stop),
                    _ => None,
                }
            } else {
                None
            },
            hits.as_deref_mut(),
            Some(*idx),
        );
        card_positions.push((*idx, card_area));
        y = y.saturating_add(height + 1);
    }

    // Hero card pinned above instructions
    if let Some(hero) = hero_row {
        let data = format_activity_row(hero, theme, false);
        let [hero_left, hero_center_raw, hero_right] = split_with_spines(layout[2]);
        render_spine(frame, hero_left, theme);
        render_spine(frame, hero_right, theme);
        let hero_center = Rect {
            x: hero_center_raw.x.saturating_add(1),
            y: hero_center_raw.y,
            width: hero_center_raw.width.saturating_sub(2),
            height: hero_center_raw.height,
        };
        render_card(
            frame,
            hero_center,
            data,
            theme,
            true,
            false,
            vm.selected() == Some(hero_idx),
            if vm.selected() == Some(hero_idx) {
                match vm.focus() {
                    FocusArea::Control(ControlFocus::Copy) => Some(ControlFocus::Copy),
                    FocusArea::Control(ControlFocus::Expand) => Some(ControlFocus::Expand),
                    FocusArea::Control(ControlFocus::Stop) => Some(ControlFocus::Stop),
                    _ => None,
                }
            } else {
                None
            },
            hits.as_deref_mut(),
            Some(hero_idx),
        );
        card_positions.push((hero_idx, hero_center));
    }

    // Instructions card
    let [instr_left, instr_center_raw, instr_right] = split_with_spines(layout[3]);
    render_spine(frame, instr_left, theme);
    render_spine(frame, instr_right, theme);
    let instr_center = Rect {
        x: instr_center_raw.x.saturating_add(1),
        y: instr_center_raw.y,
        width: instr_center_raw.width.saturating_sub(2),
        height: instr_center_raw.height,
    };
    let _layout = render_task_entry_card(
        frame,
        instr_center,
        vm,
        theme,
        matches!(vm.focus(), FocusArea::Instructions),
    );

    // Fork tooltip if enabled
    if vm.show_fork_tooltip() {
        let target_idx = vm.fork_index().unwrap_or_else(|| vm.activity().len().saturating_sub(1));
        let target_area = card_positions
            .iter()
            .find(|(idx, _)| *idx >= target_idx)
            .map(|(_, rect)| *rect)
            .or_else(|| card_positions.last().map(|(_, rect)| *rect));
        let tooltip_y = target_area.map(|rect| rect.y.saturating_sub(1)).unwrap_or(tl_center.y);
        let tooltip_width = tl_center.width.clamp(14, 30);
        let tooltip_x =
            tl_center.x.saturating_add(tl_center.width.saturating_sub(tooltip_width) / 2);
        let tooltip_area = Rect {
            x: tooltip_x,
            y: tooltip_y,
            width: tooltip_width,
            height: 1,
        };
        let tooltip = Paragraph::new(Span::styled(
            "Click here to fork",
            Style::default()
                .fg(theme.tooltip_text)
                .bg(theme.bg)
                .add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(tooltip, tooltip_area);
        if let Some(hits) = hits.as_deref_mut() {
            hits.register(tooltip_area, AgentSessionMouseAction::ForkHere(target_idx));
        }
    }

    // Footer with left/right segments
    let footer_area = layout[4];
    let footer_width = footer_area.width as usize;
    let left_text = "Alt+↑↓ Select Card   Ctrl+↑↓ Fork";
    let context_pct = vm.context_percent();
    let right_text = format!("Target: main   Context: {context_pct}%");
    let left_width = width_of(left_text);
    let right_width = width_of(&right_text);
    let spacer = footer_width.saturating_sub(left_width + right_width + 1);
    let line = Line::from(vec![
        Span::raw(" ".repeat(CONTENT_MARGIN_X as usize)),
        Span::styled(left_text, Style::default().fg(theme.muted)),
        Span::raw(" ".repeat(spacer.max(1))),
        Span::styled(
            right_text,
            Style::default().fg(context_color(context_pct, theme)),
        ),
        Span::raw(" ".repeat(CONTENT_MARGIN_X as usize)),
    ]);
    let footer = Paragraph::new(line).style(Style::default().bg(theme.bg));
    frame.render_widget(footer, footer_area);

    // Output modal overlay (dim background + centered panel)
    if let Some(modal) = vm.output_modal() {
        frame.render_widget(Block::default().style(Style::default().bg(theme.bg)), area);

        let modal_w = area.width.saturating_sub(area.width / 3).max(20);
        let modal_h = area.height.saturating_sub(area.height / 3).max(8);
        let modal_area = Rect {
            x: area.x + (area.width.saturating_sub(modal_w)) / 2,
            y: area.y + (area.height.saturating_sub(modal_h)) / 2,
            width: modal_w,
            height: modal_h,
        };

        let header = match modal.kind {
            OutputModalKind::Text => "OUTPUT",
            OutputModalKind::Stderr => "STDERR",
            OutputModalKind::Binary => "BINARY",
        };

        let header_style = match modal.kind {
            OutputModalKind::Stderr => Style::default()
                .fg(theme.gutter_stderr_fg)
                .bg(theme.bg)
                .add_modifier(Modifier::BOLD),
            _ => Style::default().fg(theme.primary).bg(theme.bg).add_modifier(Modifier::BOLD),
        };

        let body_style = match modal.kind {
            OutputModalKind::Stderr => Style::default().fg(theme.error).bg(theme.bg),
            OutputModalKind::Binary => Style::default().fg(theme.muted).bg(theme.bg),
            OutputModalKind::Text => Style::default().fg(theme.text).bg(theme.bg),
        };

        let mut lines = Vec::new();
        lines.push(Line::from(vec![Span::styled(header, header_style)]));
        lines.push(Line::from(vec![Span::raw(modal.title.clone())]));
        for chunk in modal.body.lines() {
            lines.push(Line::from(vec![Span::styled(
                chunk.to_string(),
                body_style,
            )]));
        }

        let modal_block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(theme.bg).fg(theme.text));

        frame.render_widget(Clear, modal_area);
        frame.render_widget(modal_block, modal_area);
        frame.render_widget(Paragraph::new(lines).style(body_style), modal_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_model::task_execution::{PipelineMeta, PipelineSegment, PipelineStatus};
    use ah_domain_types::task::ToolStatus;

    #[test]
    fn pipeline_metadata_colors_each_command() {
        let theme = Theme::default();
        let pipeline = PipelineMeta {
            segments: vec![
                PipelineSegment::new(Some(PipelineStatus::Success), Some("1KB".into())),
                PipelineSegment::new(Some(PipelineStatus::Failed), None),
                PipelineSegment::new(Some(PipelineStatus::Skipped), None),
            ],
        };

        let line = format_command_line(
            "cat a | grep b | sort",
            &theme,
            Some(ToolStatus::Failed),
            Some("9KB"),
            false,
            Some(&pipeline),
        );

        let find_color = |needle: &str| -> Option<Color> {
            line.spans.iter().find(|s| s.content.contains(needle)).and_then(|s| s.style.fg)
        };

        assert_eq!(find_color("cat"), Some(theme.accent));
        assert_eq!(find_color("grep"), Some(theme.error));
        assert_eq!(find_color("sort"), Some(theme.muted));
        // Size attaches to the first provided output slot
        assert_eq!(find_color("1KB"), Some(theme.muted));
    }

    #[test]
    fn dimmed_pipeline_prefers_dim_colors() {
        let theme = Theme::default();
        let pipeline = PipelineMeta {
            segments: vec![PipelineSegment::new(None, None)],
        };

        let line = format_command_line(
            "echo done",
            &theme,
            Some(ToolStatus::Completed),
            Some("2KB"),
            true,
            Some(&pipeline),
        );

        let cmd_style =
            line.spans.iter().find(|s| s.content.contains("echo")).and_then(|s| s.style.fg);
        assert_eq!(cmd_style, Some(theme.dim_text));
        assert!(line.spans.iter().any(|s| s.content.contains("2KB")));
    }

    #[test]
    fn read_range_lines_are_dimmed() {
        let theme = Theme::default();
        let card = format_activity_row(
            &AgentActivityRow::AgentRead {
                file_path: "src/lib.rs".into(),
                range: Some("10-20".into()),
            },
            &theme,
            false,
        );

        // Expect range lines (indices 1 and 2) to use dim_text foreground.
        assert!(
            card.content_area.lines.iter().skip(1).take(2).all(|l| l
                .style
                .as_ref()
                .map(|s| s.fg == Some(theme.dim_text))
                .unwrap_or(false)),
            "range lines should carry dim_text styling"
        );
    }

    #[test]
    fn dimmed_cards_use_dim_text_for_content() {
        use ratatui::{Terminal, backend::TestBackend};
        let theme = Theme::default();
        let card = format_activity_row(
            &AgentActivityRow::AgentThought {
                thought: "DimMe".into(),
            },
            &theme,
            true, // dimmed flag
        );
        let mut terminal = Terminal::new(TestBackend::new(40, 6)).unwrap();
        terminal
            .draw(|f| {
                render_card(
                    f,
                    Rect {
                        x: 0,
                        y: 0,
                        width: 40,
                        height: 5,
                    },
                    card.clone(),
                    &theme,
                    false,
                    true,
                    false,
                    None,
                    None,
                    Some(0),
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        // find text "DimMe"
        let mut found = false;
        for y in 0..buffer.area().height {
            for x in 0..buffer.area().width {
                if let Some(cell) = buffer.cell((x, y)) {
                    if cell.symbol() == "D" {
                        found = true;
                        assert_eq!(
                            cell.style().fg,
                            Some(theme.dim_text),
                            "dimmed card text should use dim_text color"
                        );
                        break;
                    }
                }
            }
            if found {
                break;
            }
        }
        assert!(found, "expected to render dimmed content text");
    }
}
