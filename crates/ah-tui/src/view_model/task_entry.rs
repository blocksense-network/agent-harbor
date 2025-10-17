//! Task Entry ViewModel - for draft/editable task cards

use ah_domain_types::SelectedModel;
use super::{ButtonViewModel, DraftSaveState, FocusElement};

#[derive(Debug, Clone, PartialEq)]
pub struct DraftControlsViewModel {
    pub repository_button: ButtonViewModel,
    pub branch_button: ButtonViewModel,
    pub model_button: ButtonViewModel,
    pub go_button: ButtonViewModel,
}

/// ViewModel for draft/editable task entries
#[derive(Clone)] // Debug and PartialEq removed due to TextArea
pub struct TaskEntryViewModel {
    pub id: String, // Unique identifier for the task entry
    pub repository: String, // Repository name
    pub branch: String, // Branch name
    pub models: Vec<SelectedModel>, // Selected models
    pub created_at: String, // Creation timestamp
    pub height: u16,
    pub controls: DraftControlsViewModel,
    pub save_state: DraftSaveState,
    pub description: tui_textarea::TextArea<'static>, // TextArea stores content, cursor, and placeholder
    pub focus_element: FocusElement, // Current focus within this card
    pub auto_save_timer: Option<std::time::Instant>, // Timer for auto-save functionality
}
