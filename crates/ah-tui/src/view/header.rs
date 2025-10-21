//! Header View Component
//!
//! Renders the application header including logo and settings button.

use ratatui::{prelude::*, widgets::*};
use ratatui_image::StatefulImage;

/// Render the header section with logo and settings button
pub fn render_header(
    frame: &mut Frame<'_>,
    area: Rect,
    view_model: &mut crate::view_model::ViewModel,
    view_cache: &mut crate::view::ViewCache,
) {
    // Create padded content area within the header
    let content_area = if area.width >= 6 && area.height >= 4 {
        // Add padding: 1 line top/bottom, 2 columns left/right
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Top padding
                Constraint::Min(1),    // Content area
                Constraint::Length(1), // Bottom padding
            ])
            .split(area);

        let middle_area = vertical_chunks[1];

        let horizontal_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(2), // Left padding
                Constraint::Min(1),    // Content area
                Constraint::Length(2), // Right padding
            ])
            .split(middle_area);

        horizontal_chunks[1]
    } else {
        // If area is too small, use the full area (no padding)
        area
    };

    // Render settings button in upper right corner (before logo to ensure it's always visible)
    if area.width > 15 && area.height > 2 {
        let button_text = "⚙ Settings";
        let button_width = button_text.len() as u16 + 2; // +2 for padding
        let button_x = area.x.saturating_add(area.width.saturating_sub(button_width + 2));
        let button_area = Rect {
            x: button_x,   // 2 units from right edge
            y: area.y + 1, // Just below top padding
            width: button_width,
            height: 1,
        };

        let theme = crate::view::Theme::default();
        let button_style = if matches!(view_model.focus_element, crate::view_model::FocusElement::SettingsButton) {
            Style::default().fg(theme.bg).bg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.primary)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD)
        };

        let button_line = Line::from(vec![
            Span::styled(" ", button_style),
            Span::styled(button_text, button_style),
            Span::styled(" ", button_style),
        ]);

        let button_paragraph = Paragraph::new(button_line);
        frame.render_widget(button_paragraph, button_area);

    }

    // Try to render the logo as an image first using persisted protocol
    if let Some(protocol) = view_cache.logo_protocol.as_mut() {
        // Render the logo image using StatefulImage widget in the padded area
        let image_widget = StatefulImage::default();
        frame.render_stateful_widget(image_widget, content_area, protocol);

        // Check for encoding errors and log them (don't fail the whole UI)
        if let Some(Err(e)) = protocol.last_encoding_result() {
            // If image rendering fails, fall through to ASCII
            eprintln!("Image logo rendering failed: {}", e);
        } else {
            // Image rendered successfully, we're done
            return;
        }
    }

    // Fallback to ASCII logo
    render_ascii_logo(frame, content_area);
}

/// Render the ASCII logo as fallback
fn render_ascii_logo(frame: &mut Frame<'_>, area: Rect) {
    // Try to read the ASCII logo from assets
    let logo_content = include_str!("../../../../assets/agent-harbor-logo-80.ansi");

    // Create a paragraph with the logo, preserving ANSI escape codes
    let header = Paragraph::new(logo_content)
        .style(Style::default())
        .alignment(Alignment::Center);
    frame.render_widget(header, area);
}
