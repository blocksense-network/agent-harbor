// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: Apache-2.0

//! Modal rendering functions
//!
//! This module contains functions for rendering modal dialogs and overlays.

use ratatui::layout::Rect;
use ratatui::Frame;

use super::dialogs::{
    render_fuzzy_modal, render_settings_dialog, FuzzySearchModal,
};
use super::Theme;
use crate::view_model::{ModalState, ViewModel};

/// Render active modal dialogs
pub fn render_modals(
    frame: &mut Frame,
    view_model: &ViewModel,
    area: Rect,
    theme: &Theme,
) {
    match view_model.modal_state {
        ModalState::None => {
            // No modal to render
        }
        ModalState::RepositorySearch => {
            // Use the actual modal data from view_model
            if let Some(modal) = &view_model.active_modal {
                let fuzzy_modal = FuzzySearchModal {
                    input: modal.input_value.clone(),
                    options: modal.filtered_options.iter().map(|(opt, _)| opt.clone()).collect(),
                    selected_index: modal.selected_index,
                };
                render_fuzzy_modal(frame, &fuzzy_modal, area, theme);
            }
        }
        ModalState::BranchSearch => {
            // Use the actual modal data from view_model
            if let Some(modal) = &view_model.active_modal {
                let fuzzy_modal = FuzzySearchModal {
                    input: modal.input_value.clone(),
                    options: modal.filtered_options.iter().map(|(opt, _)| opt.clone()).collect(),
                    selected_index: modal.selected_index,
                };
                render_fuzzy_modal(frame, &fuzzy_modal, area, theme);
            }
        }
        ModalState::ModelSearch => {
            // Use the actual modal data from view_model
            if let Some(modal) = &view_model.active_modal {
                let fuzzy_modal = FuzzySearchModal {
                    input: modal.input_value.clone(),
                    options: modal.filtered_options.iter().map(|(opt, _)| opt.clone()).collect(),
                    selected_index: modal.selected_index,
                };
                render_fuzzy_modal(frame, &fuzzy_modal, area, theme);
            }
        }
        ModalState::Settings => {
            render_settings_dialog(frame, area, theme);
        }
    }
}
