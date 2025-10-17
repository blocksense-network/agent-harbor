//! View Layer - Pure Rendering and Presentation
//!
//! This module contains the Ratatui rendering code that transforms
//! ViewModel state into terminal widgets. The View layer is the final
//! step in the MVVM pipeline and should contain zero business logic.
//!
//! ## What Belongs Here:
//!
//! ✅ **Rendering Logic**: Ratatui widget creation and layout
//! ✅ **Visual Styling**: Colors, borders, spacing, typography
//! ✅ **Widget Composition**: Combining ViewModel data into UI layouts
//! ✅ **Terminal Drawing**: Converting widgets to terminal output
//! ✅ **Pure Functions**: ViewModel → Ratatui widgets transformations
//!
//! ## What Does NOT Belong Here:
//!
//! ❌ **Business Logic**: Any application behavior or state changes
//! ❌ **UI Events**: Key handling, mouse processing, input validation
//! ❌ **UI State**: Selection management, focus tracking, modal states
//! ❌ **Domain Logic**: Task operations, business rules, calculations
//!
//! ## Architecture Role:
//!
//! The View is the final, pure transformation layer:
//! 1. **Receives ViewModel** - Already prepared presentation data
//! 2. **Creates Ratatui widgets** - Terminal UI components
//! 3. **Handles layout** - Positioning, sizing, responsive design
//! 4. **Applies styling** - Colors, borders, visual hierarchy
//! 5. **Renders to terminal** - Final pixel output
//!
//! ## View Output:
//!
//! The only output of the View is a collection of hit test rectangles that are
//! determined after actual rendering and that are used by the input dispatcher
//! to determine what mouse click events should be delivered to the ViewModel.
//!
//! ## Design Principles:
//!
//! - **Pure Functions Only**: View functions should be deterministic and side-effect free
//! - **No State Mutations**: View never modifies ViewModel or Model state
//! - **Presentation Only**: Focus on visual appearance and user experience
//! - **Testable**: Rendering logic can be tested independently

use ratatui::{prelude::*, widgets::*};

// pub mod header; // Header rendering components - TODO: move later
pub mod draft_card; // Draft card rendering components

/// Cache for view-related computations and state
pub struct ViewCache {
    // Image rendering state
    pub picker: Option<ratatui_image::picker::Picker>,
    pub logo_protocol: Option<ratatui_image::protocol::StatefulProtocol>,

    // Cached computed strings - only recompute if inputs changed
    last_separator_width: Option<u16>,
    cached_separator: Option<String>,
}

impl ViewCache {
    pub fn new() -> Self {
        ViewCache {
            picker: None,
            logo_protocol: None,
            last_separator_width: None,
            cached_separator: None,
        }
    }

    /// Get a cached separator string - only recomputes if width changed
    pub fn get_separator(&mut self, width: u16) -> &str {
        if self.last_separator_width != Some(width) {
            self.cached_separator = Some("─".repeat(width as usize));
            self.last_separator_width = Some(width);
        }
        self.cached_separator.as_ref().unwrap()
    }
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
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            // Dark theme inspired by Catppuccin Mocha with Charm aesthetics
            bg: Color::Rgb(17, 17, 27),                // Base background
            surface: Color::Rgb(24, 24, 37),           // Card/surface background
            text: Color::Rgb(205, 214, 244),           // Main text
            muted: Color::Rgb(127, 132, 156),          // Secondary text
            primary: Color::Rgb(137, 180, 250),        // Blue for primary actions
            accent: Color::Rgb(166, 218, 149),         // Green for success/accent
            success: Color::Rgb(166, 218, 149),        // Green
            warning: Color::Rgb(250, 179, 135),        // Orange/yellow
            error: Color::Rgb(243, 139, 168),          // Red/pink
            border: Color::Rgb(69, 71, 90),            // Border color
            border_focused: Color::Rgb(137, 180, 250), // Focused border color
        }
    }
}

impl Theme {
    /// Create a card block with Charm-style rounded borders and padding
    pub fn card_block(&self, title: &str) -> Block {
        let title_line = Line::from(vec![
            Span::raw("┤").fg(self.border),
            Span::raw(format!(" {} ", title))
                .style(Style::default().fg(self.text).add_modifier(Modifier::BOLD)),
            Span::raw("├").fg(self.border),
        ]);

        Block::default()
            .title(title_line)
            .title_alignment(ratatui::layout::Alignment::Left)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.border))
            .padding(Padding::new(1, 1, 1, 1))
            .style(Style::default().bg(self.bg))
    }

    /// Create a card block with a right-aligned button in the title area
    pub fn card_block_with_button(
        &self,
        title: &str,
        button_text: &str,
        button_focused: bool,
    ) -> Block {
        let button_style = if button_focused {
            Style::default().fg(self.bg).bg(self.error).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.error).bg(self.surface).add_modifier(Modifier::BOLD)
        };

        let title_line = Line::from(vec![
            Span::raw("┤").fg(self.border),
            Span::raw(format!(" {} ", title))
                .style(Style::default().fg(self.text).add_modifier(Modifier::BOLD)),
            Span::raw("├").fg(self.border),
            Span::raw(" ".repeat(15)), // Spacer to push button to right
            Span::styled(format!(" {} ", button_text), button_style),
        ]);

        Block::default()
            .title(title_line)
            .title_alignment(ratatui::layout::Alignment::Left)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.border))
            .padding(Padding::new(2, 2, 1, 1))
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
