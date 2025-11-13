// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: Apache-2.0

//! Modal rendering functions
//!
//! This module contains functions for rendering modal dialogs and overlays.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::Theme;
use super::dialogs::{FuzzySearchModal, render_fuzzy_modal, render_settings_dialog};
use crate::view_model::dashboard_model::FilteredOption;
use crate::view_model::{ModalState, ViewModel};

/// Render active modal dialogs
pub fn render_modals(frame: &mut Frame, view_model: &ViewModel, area: Rect, theme: &Theme) {
    match view_model.modal_state {
        ModalState::None => {
            // No modal to render
        }
        ModalState::RepositorySearch => {
            // Use the actual modal data from view_model
            if let Some(modal) = &view_model.active_modal {
                let fuzzy_modal = FuzzySearchModal {
                    input: modal.input_value.clone(),
                    options: modal
                        .filtered_options
                        .iter()
                        .filter_map(|opt| match opt {
                            FilteredOption::Option { text, .. } => Some(text.clone()),
                            FilteredOption::Separator { .. } => None,
                        })
                        .collect(),
                    selected_index: modal.selected_index,
                };
                render_fuzzy_modal(frame, &fuzzy_modal, area, theme, 3);
            }
        }
        ModalState::BranchSearch => {
            // Use the actual modal data from view_model
            if let Some(modal) = &view_model.active_modal {
                let fuzzy_modal = FuzzySearchModal {
                    input: modal.input_value.clone(),
                    options: modal
                        .filtered_options
                        .iter()
                        .filter_map(|opt| match opt {
                            FilteredOption::Option { text, .. } => Some(text.clone()),
                            FilteredOption::Separator { .. } => None,
                        })
                        .collect(),
                    selected_index: modal.selected_index,
                };
                render_fuzzy_modal(frame, &fuzzy_modal, area, theme, 3);
            }
        }
        ModalState::ModelSearch => {
            // Use the actual modal data from view_model
            if let Some(modal) = &view_model.active_modal {
                match &modal.modal_type {
                    crate::view_model::ModalType::ModelSelection { .. } => {
                        // For ModelSelection, use single-line input
                        let options = modal
                            .filtered_options
                            .iter()
                            .filter_map(|opt| match opt {
                                FilteredOption::Option { text, .. } => Some(text.clone()),
                                FilteredOption::Separator { .. } => None,
                            })
                            .collect();

                        let fuzzy_modal = FuzzySearchModal {
                            input: modal.input_value.clone(),
                            options,
                            selected_index: modal.selected_index,
                        };
                        render_fuzzy_modal(frame, &fuzzy_modal, area, theme, 3);
                    }
                    _ => {
                        // For other modal types, use fuzzy search rendering
                        let fuzzy_modal = FuzzySearchModal {
                            input: modal.input_value.clone(),
                            options: modal
                                .filtered_options
                                .iter()
                                .filter_map(|opt| match opt {
                                    FilteredOption::Option { text, .. } => Some(text.clone()),
                                    FilteredOption::Separator { .. } => None,
                                })
                                .collect(),
                            selected_index: modal.selected_index,
                        };
                        render_fuzzy_modal(frame, &fuzzy_modal, area, theme, 3);
                    }
                }
            }
        }
        ModalState::Settings => {
            render_settings_dialog(frame, area, theme);
        }
    }
}
