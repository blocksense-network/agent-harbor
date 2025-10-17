//! Task Execution ViewModel - for active/completed/merged task cards

use ah_domain_types::{TaskExecution, SelectedModel, TaskState};
use ah_core::task_manager::ToolStatus;
use super::{ButtonViewModel, FocusElement};

#[derive(Debug, Clone, PartialEq)]
pub struct TaskMetadataViewModel {
    pub repository: String,
    pub branch: String,
    pub models: Vec<SelectedModel>,
    pub state: TaskState,
    pub timestamp: String,
    pub delivery_indicators: String, // Delivery status indicators (⎇ ⇄ ✓)
}

/// Activity entries for active task cards
#[derive(Debug, Clone, PartialEq)]
pub enum AgentActivityRow {
    /// Agent thought/reasoning
    AgentThought {
        thought: String,
    },
    /// Agent file edit
    AgentEdit {
        file_path: String,
        lines_added: usize,
        lines_removed: usize,
        description: Option<String>,
    },
    /// Tool usage with execution state
    ToolUse {
        tool_name: String,
        tool_execution_id: String,
        last_line: Option<String>, // None = just started, Some = has output
        completed: bool, // true when ToolResult received
        status: ToolStatus,
    },
}

/// Different visual types of regular task cards (active/completed/merged)
#[derive(Debug, Clone, PartialEq)]
pub enum TaskCardType {
    /// Active task with real-time activity
    Active {
        activity_entries: Vec<AgentActivityRow>, // Processed activity data (ViewModel layer)
        pause_delete_buttons: String,
    },
    /// Completed task with delivery indicators
    Completed {
        delivery_indicators: String, // Formatted indicator text with colors
    },
    /// Merged task with delivery indicators
    Merged {
        delivery_indicators: String, // Formatted indicator text with colors
    },
}

/// ViewModel for task execution cards (active/completed/merged)
#[derive(Debug, Clone, PartialEq)]
pub struct TaskExecutionViewModel {
    pub id: String, // Unique identifier for the task card
    pub task: TaskExecution, // Domain object
    pub title: String,
    pub metadata: TaskMetadataViewModel,
    pub height: u16,
    pub card_type: TaskCardType, // Active, Completed, or Merged
    pub focus_element: FocusElement, // Current focus within this card
}
