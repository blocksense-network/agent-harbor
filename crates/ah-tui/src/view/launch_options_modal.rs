// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: Apache-2.0

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};

use super::Theme;
use crate::view_model::agents_selector_model::{
    AdvancedLaunchOptions, FilteredOption, LaunchOptionsColumn, LaunchOptionsViewModel,
};

pub fn render_advanced_launch_options_modal(
    frame: &mut Frame,
    model: &LaunchOptionsViewModel,
    area: Rect,
    theme: &Theme,
) {
    // Calculate modal dimensions (large modal)
    let modal_width = 100.min(area.width.saturating_sub(4));
    let modal_height = 35.min(area.height.saturating_sub(4));

    let modal_area = Rect {
        x: (area.width - modal_width) / 2,
        y: (area.height - modal_height) / 2,
        width: modal_width,
        height: modal_height,
    };

    // Shadow
    let mut shadow_area = modal_area;
    shadow_area.x += 1;
    shadow_area.y += 1;
    let shadow = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(shadow, shadow_area);

    // Main Block
    let title_line = Line::from(vec![
        Span::raw("┤").fg(theme.primary),
        Span::raw(" Advanced Launch Options ")
            .style(Style::default().fg(theme.text).add_modifier(Modifier::BOLD)),
        Span::raw("├").fg(theme.primary),
    ]);

    let modal_block = Block::default()
        .title(title_line)
        .title_alignment(Alignment::Left)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_focused))
        .padding(Padding::new(1, 1, 1, 1))
        .style(Style::default().bg(theme.surface));

    frame.render_widget(Clear, modal_area);
    let inner_area = modal_block.inner(modal_area);
    frame.render_widget(modal_block, modal_area);

    // Split into two columns
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60), // Options (Left)
            Constraint::Length(1),      // Separator
            Constraint::Percentage(40), // Actions (Right)
        ])
        .split(inner_area);

    let left_area = chunks[0];
    let _separator_area = chunks[1];
    let right_area = chunks[2];

    // Draw Vertical Separator manually
    // We draw it on the separator area
    // But since we have a gap, let's just ensure visual separation.
    // We can draw a line in the separator column.
    // Rendering this on right_area will draw a left border for it, acting as separator
    // But we need to be careful not to overwrite content or mess up padding.
    // Let's try rendering a vertical line widget if available, or just use border on right pane.

    let _options_height = render_options_column(frame, model, left_area, theme);

    // Draw separator line
    let separator_x = left_area.x + left_area.width;
    for y in inner_area.y..inner_area.y + inner_area.height {
        let buf = frame.buffer_mut();
        if separator_x < buf.area.width {
            buf.cell_mut((separator_x, y))
                .map(|cell| cell.set_symbol("│").set_fg(theme.muted));
        }
    }

    render_actions_column(frame, model, right_area, theme);
}

struct RenderableOption<'a> {
    label: &'a str,
    value: String,
    is_header: bool,
    indent: u16,
}

#[allow(clippy::vec_init_then_push)]
fn get_renderable_options<'a>(config: &AdvancedLaunchOptions) -> Vec<RenderableOption<'a>> {
    let mut options = Vec::new();

    // Sandbox & Environment
    options.push(RenderableOption {
        label: "Sandbox & Environment",
        value: "".to_string(),
        is_header: true,
        indent: 0,
    });
    options.push(RenderableOption {
        label: "Sandbox Profile",
        value: config.sandbox_profile.clone(),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Working Copy Mode",
        value: config.working_copy_mode.clone(),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "FS Snapshots",
        value: config.fs_snapshots.clone(),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Devcontainer Path/Tag",
        value: config.devcontainer_path.clone(),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Allow Egress",
        value: bool_to_yes_no(config.allow_egress),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Allow Containers",
        value: bool_to_yes_no(config.allow_containers),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Allow VMs",
        value: bool_to_yes_no(config.allow_vms),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Allow Web Search",
        value: bool_to_yes_no(config.allow_web_search),
        is_header: false,
        indent: 1,
    });

    // Agent Configuration
    options.push(RenderableOption {
        label: "Agent Configuration",
        value: "".to_string(),
        is_header: true,
        indent: 0,
    });
    options.push(RenderableOption {
        label: "Interactive Mode",
        value: bool_to_yes_no(config.interactive_mode),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Output Format",
        value: config.output_format.clone(),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Record Output",
        value: bool_to_yes_no(config.record_output),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Timeout",
        value: config.timeout.clone(),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "LLM Provider",
        value: config.llm_provider.clone(),
        is_header: false,
        indent: 1,
    });
    // Environment Variables - simplified for display
    let env_count = config.environment_variables.len();
    options.push(RenderableOption {
        label: "Environment Variables",
        value: format!("{} items", env_count),
        is_header: false,
        indent: 1,
    });

    // Task Management
    options.push(RenderableOption {
        label: "Task Management",
        value: "".to_string(),
        is_header: true,
        indent: 0,
    });
    options.push(RenderableOption {
        label: "Delivery Method",
        value: config.delivery_method.clone(),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Target Branch",
        value: config.target_branch.clone(),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Create Task Files",
        value: bool_to_yes_no(config.create_task_files),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Create Metadata Commits",
        value: bool_to_yes_no(config.create_metadata_commits),
        is_header: false,
        indent: 1,
    });
    options.push(RenderableOption {
        label: "Notifications",
        value: bool_to_yes_no(config.notifications),
        is_header: false,
        indent: 1,
    });
    let label_count = config.labels.len();
    options.push(RenderableOption {
        label: "Labels",
        value: format!("{} items", label_count),
        is_header: false,
        indent: 1,
    });
    // Push to Remote (PLANNED - skip or show disabled?) PRD says "DON'T IMPLEMENT YET", but listed in menu.
    // The user prompt says "implement the new advanced task launch options dialog", based on "full contents of the last commit".
    // The commit lists "Push to Remote ... (PLANNED - DON'T IMPLEMENT YET)".
    // I will include it but maybe mark it or just skip to be safe. I'll skip it to follow "DON'T IMPLEMENT YET" strictly.
    options.push(RenderableOption {
        label: "Fleet",
        value: config.fleet.clone(),
        is_header: false,
        indent: 1,
    });

    options
}

fn bool_to_yes_no(v: bool) -> String {
    if v {
        "yes".to_string()
    } else {
        "no".to_string()
    }
}

fn render_options_column(
    frame: &mut Frame,
    model: &LaunchOptionsViewModel,
    area: Rect,
    theme: &Theme,
) -> u16 {
    let options = get_renderable_options(&model.config);

    // Calculate scroll offset to keep selected item visible
    // Simple logic: if selected index is far down, scroll down.
    // Since headers are in the list but not selectable (presumably), mapping selected_option_index to list index requires care.
    // Assuming selected_option_index refers to the index in `options` vector including headers?
    // Or `selected_option_index` skips headers? The ViewModel implementation would define this.
    // Usually easier if selected_index points to the actual index in the flattened list.
    // If headers are not selectable, the controller logic ensures selected_index doesn't land on them.
    // I'll assume selected_index is the index in `options`.

    let height = area.height as usize;
    let selected_idx = model.selected_option_index;

    // Basic scroll logic
    let scroll_offset = if selected_idx >= height {
        selected_idx - height + 1
    } else {
        0
    };

    let mut current_y = area.y;
    let mut processed_count = 0;
    let mut headers_rendered = 0;

    for (i, option) in options.iter().enumerate().skip(scroll_offset) {
        if processed_count >= height {
            break;
        }

        let is_selected = model.active_column == LaunchOptionsColumn::Options && i == selected_idx;

        // Add empty line before each header except the first one
        if option.is_header && headers_rendered > 0 {
            if processed_count + 1 >= height {
                break;
            }
            current_y += 1; // Empty line before section header
            processed_count += 1;
        }

        let row_area = Rect {
            x: area.x,
            y: current_y,
            width: area.width,
            height: 1,
        };

        if option.is_header {
            let style = Style::default().fg(theme.primary).add_modifier(Modifier::BOLD);
            frame.render_widget(Paragraph::new(option.label).style(style), row_area);
            current_y += 2; // Header + empty line
            processed_count += 2;
            headers_rendered += 1;
        } else {
            let (bg, fg) = if is_selected {
                (theme.primary, theme.surface)
            } else {
                (theme.surface, theme.text)
            };

            let style = Style::default().bg(bg).fg(fg);

            // Indent
            let label_padding = "  ".repeat(option.indent as usize);
            let label_width = 25; // Fixed width for labels

            // Split label and value
            // We can use layout or manual formatting

            let label_span = Span::styled(
                format!("{}{:<w$}", label_padding, option.label, w = label_width),
                style,
            );
            let value_span = Span::styled(format!(" : {}", option.value), style);

            let line = Line::from(vec![label_span, value_span]);
            frame.render_widget(Paragraph::new(line).style(style), row_area);
            current_y += 1;
            processed_count += 1;
        }
    }

    // Render inline enum popup if active
    if let Some(popup) = &model.inline_enum_popup {
        // Find the position of the option this popup is for
        let mut popup_y = area.y;
        let mut found = false;

        for (i, option) in options.iter().enumerate().skip(scroll_offset) {
            if i == popup.option_index {
                // This is the option the popup is for - position popup below it
                if option.is_header {
                    popup_y += 2; // Header takes 2 lines
                } else {
                    popup_y += 1; // Regular option takes 1 line
                }
                found = true;
                break;
            } else {
                // Count how much space this option takes
                if option.is_header {
                    popup_y += 2; // Header takes 2 lines
                } else {
                    popup_y += 1; // Regular option takes 1 line
                }
            }
        }

        if found && popup_y < area.y + area.height {
            // Render the popup as a bordered box
            let popup_height = popup.options.len() as u16 + 2; // +2 for borders
            let popup_width = 20; // Fixed width for enum popup

            let popup_area = Rect {
                x: area.x + 30, // Position to the right of the options
                y: popup_y,
                width: popup_width,
                height: popup_height.min(area.height - (popup_y - area.y)),
            };

            // Clear the popup area to prevent elements behind it from polluting it
            frame.render_widget(Clear, popup_area);

            let popup_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .style(Style::default().fg(theme.primary));

            frame.render_widget(popup_block, popup_area);

            let inner_area = popup_area.inner(Margin {
                vertical: 1,
                horizontal: 1,
            });

            for (i, option) in popup.options.iter().enumerate() {
                if i >= inner_area.height as usize {
                    break;
                }

                let is_selected = i == popup.selected_index;
                let style = if is_selected {
                    Style::default().bg(theme.primary).fg(theme.surface)
                } else {
                    Style::default().fg(theme.text)
                };

                let option_area = Rect {
                    x: inner_area.x,
                    y: inner_area.y + i as u16,
                    width: inner_area.width,
                    height: 1,
                };

                frame.render_widget(
                    Paragraph::new(option.clone()).style(style).alignment(Alignment::Center),
                    option_area,
                );
            }
        }
    }

    processed_count as u16
}

fn render_actions_column(
    frame: &mut Frame,
    model: &LaunchOptionsViewModel,
    area: Rect,
    theme: &Theme,
) {
    // Render title
    let title_style = Style::default().fg(theme.primary).add_modifier(Modifier::BOLD);
    let title_area = Rect {
        x: area.x + 1, // One column padding
        y: area.y,
        width: area.width - 1,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new("Keyboard Shortcuts").style(title_style),
        title_area,
    );

    // Actions as FilteredOptions like the old implementation
    let actions = vec![
        FilteredOption::Option {
            text: "Launch in new tab (t)".to_string(),
            selected: false,
        },
        FilteredOption::Option {
            text: "Launch in split view (s)".to_string(),
            selected: false,
        },
        FilteredOption::Option {
            text: "Launch in horizontal split (h)".to_string(),
            selected: false,
        },
        FilteredOption::Option {
            text: "Launch in vertical split (v)".to_string(),
            selected: false,
        },
        FilteredOption::Separator {
            label: Some("Focus variants".to_string()),
        },
        FilteredOption::Option {
            text: "Launch in new tab and focus (T)".to_string(),
            selected: false,
        },
        FilteredOption::Option {
            text: "Launch in split view and focus (S)".to_string(),
            selected: false,
        },
        FilteredOption::Option {
            text: "Launch in horizontal split and focus (H)".to_string(),
            selected: false,
        },
        FilteredOption::Option {
            text: "Launch in vertical split and focus (V)".to_string(),
            selected: false,
        },
    ];

    // Content starts after title
    let content_area = Rect {
        x: area.x,
        y: area.y + 2, // Leave space for title + one line spacing
        width: area.width,
        height: area.height - 2,
    };

    // Calculate scroll offset to keep selected item visible
    let height = content_area.height as usize;
    let selected_idx = model.selected_action_index;
    let scroll_offset = if selected_idx >= height {
        selected_idx - height + 1
    } else {
        0
    };

    // Calculate how many actions we can show, leaving room for "ENTER Edit Value" at bottom
    let available_height = height.saturating_sub(2); // Reserve 2 lines for spacing and ENTER Edit Value
    let visible_actions = actions.iter().enumerate().skip(scroll_offset).take(available_height);

    for (i, (_, option)) in visible_actions.enumerate() {
        let is_selected = model.active_column == LaunchOptionsColumn::Actions
            && (i + scroll_offset) == selected_idx;

        let style = if is_selected {
            Style::default().bg(theme.primary).fg(theme.surface)
        } else {
            Style::default().fg(theme.text)
        };

        let row_area = Rect {
            x: content_area.x + 1, // One column padding
            y: content_area.y + i as u16,
            width: content_area.width - 1,
            height: 1,
        };

        match option {
            FilteredOption::Option { text, .. } => {
                // Parse the text to separate description from shortcut
                let (description, shortcut) = if let Some(paren_idx) = text.rfind(" (") {
                    if text.ends_with(')') {
                        let desc = &text[..paren_idx];
                        let short = &text[paren_idx + 2..text.len() - 1];
                        (desc, short)
                    } else {
                        (text.as_str(), "")
                    }
                } else {
                    (text.as_str(), "")
                };

                let line = if shortcut.is_empty() {
                    Line::from(description).style(style)
                } else {
                    // Put shortcut on the left with distinct styling
                    let shortcut_span = Span::styled(
                        format!("{} ", shortcut),
                        style.fg(theme.primary).add_modifier(Modifier::BOLD),
                    );
                    let desc_span = Span::styled(description, style);
                    Line::from(vec![shortcut_span, desc_span])
                };

                frame.render_widget(Paragraph::new(line), row_area);
            }
            FilteredOption::Separator { label } => {
                if let Some(label) = label {
                    let separator_style = Style::default().fg(theme.muted);
                    let line = Line::from(format!("─ {} ", label)).style(separator_style);
                    frame.render_widget(Paragraph::new(line), row_area);
                }
            }
        }
    }

    // Add "ENTER Edit Value" at the bottom
    if height >= 2 {
        let edit_value_area = Rect {
            x: content_area.x + 1,                   // One column padding
            y: content_area.y + (height - 1) as u16, // Last line
            width: content_area.width - 1,
            height: 1,
        };

        let edit_value_style = Style::default().fg(theme.muted);
        frame.render_widget(
            Paragraph::new("ENTER Edit Value").style(edit_value_style),
            edit_value_area,
        );
    }
}
