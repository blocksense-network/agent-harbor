// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Task Execution ViewModel - for active/completed/merged task cards

// No super imports needed here; remove unused imports
use crate::settings::Settings;
use ah_core::TaskEvent;
use ah_domain_types::task::ToolStatus;
use ah_domain_types::{AgentChoice, TaskExecution, TaskState};
use tracing::debug;

/// Focus states specific to task execution cards
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskExecutionFocusState {
    /// The card is not focused (default state)
    None,
    /// The stop button is focused
    StopButton,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskMetadataViewModel {
    pub repository: String,
    pub branch: String,
    pub models: Vec<AgentChoice>,
    pub state: TaskState,
    pub timestamp: String,
    pub delivery_indicators: String, // Delivery status indicators (⎇ ⇄ ✓)
}

/// Activity entries for active task cards
#[derive(Debug, Clone, PartialEq)]
pub enum AgentActivityRow {
    /// Agent thought/reasoning
    AgentThought { thought: String },
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
        completed: bool,           // true when ToolResult received
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
    pub id: String,          // Unique identifier for the task card
    pub task: TaskExecution, // Domain object
    pub title: String,
    pub metadata: TaskMetadataViewModel,
    pub height: u16,
    pub card_type: TaskCardType, // Active, Completed, or Merged
    pub focus_element: TaskExecutionFocusState, // Current focus within this card
    pub needs_redraw: bool,      // Flag indicating if the task card needs to be redrawn
}

impl TaskExecutionViewModel {
    /// Process a TaskEvent directly on this task card
    /// This is the core event processing logic that can be called directly
    pub fn process_task_event(&mut self, event: TaskEvent, settings: &Settings) {
        debug!(
            "Processing task event directly on active task {}: {:?}",
            self.id, event
        );

        // Handle status changes that affect the entire task
        if let TaskEvent::Status { status, .. } = &event {
            // Update the task's state based on the status
            use ah_domain_types::task::TaskState;

            // Map TaskState to determine card type
            let new_task_state = match status {
                TaskState::Queued
                | TaskState::Provisioning
                | TaskState::Running
                | TaskState::Pausing
                | TaskState::Paused
                | TaskState::Resuming
                | TaskState::Stopping
                | TaskState::Stopped => TaskState::Running, // Keep as running/active state for UI
                TaskState::Completed | TaskState::Failed | TaskState::Cancelled => {
                    TaskState::Completed
                }
                TaskState::Draft | TaskState::Merged => *status, // Keep as-is for these states
            };

            // Update the domain task state
            self.task.state = new_task_state;

            // Mark that we need a redraw due to state change
            self.needs_redraw = true;

            // If task is in a final state, change card type and clear activity entries
            let is_final_state = matches!(
                new_task_state,
                TaskState::Completed | TaskState::Failed | TaskState::Cancelled | TaskState::Merged
            );
            if is_final_state {
                self.card_type = match new_task_state {
                    TaskState::Completed | TaskState::Failed | TaskState::Cancelled => {
                        TaskCardType::Completed {
                            delivery_indicators: match new_task_state {
                                TaskState::Failed => "Failed".to_string(),
                                TaskState::Cancelled => "Cancelled".to_string(),
                                _ => String::new(),
                            },
                        }
                    }
                    TaskState::Merged => TaskCardType::Merged {
                        delivery_indicators: String::new(),
                    },
                    _ => unreachable!("Should not transition to running states here"),
                };
                // Update metadata to reflect final state
                self.metadata.state = new_task_state;
                return; // Don't process as activity entry since card is no longer active
            } else {
                // Update metadata state for active tasks
                self.metadata.state = new_task_state;
            }
        }

        // Only process activity entries if the task is still active
        if let TaskCardType::Active {
            ref mut activity_entries,
            ..
        } = self.card_type
        {
            match event {
                TaskEvent::Thought { thought, .. } => {
                    // Add new thought entry
                    let activity_entry = AgentActivityRow::AgentThought {
                        thought: thought.clone(),
                    };
                    activity_entries.push(activity_entry);
                    self.needs_redraw = true;
                }
                TaskEvent::FileEdit {
                    file_path,
                    lines_added,
                    lines_removed,
                    description,
                    ..
                } => {
                    // Add new file edit entry
                    let activity_entry = AgentActivityRow::AgentEdit {
                        file_path: file_path.clone(),
                        lines_added,
                        lines_removed,
                        description: description.clone(),
                    };
                    activity_entries.push(activity_entry);
                    self.needs_redraw = true;
                }
                TaskEvent::ToolUse {
                    tool_name,
                    tool_execution_id,
                    status,
                    ..
                } => {
                    // Add new tool use entry
                    let activity_entry = AgentActivityRow::ToolUse {
                        tool_name: tool_name.clone(),
                        tool_execution_id: tool_execution_id.clone(),
                        last_line: None,
                        completed: false,
                        status,
                    };
                    activity_entries.push(activity_entry);
                    self.needs_redraw = true;
                }
                TaskEvent::Log {
                    message,
                    tool_execution_id: Some(tool_exec_id),
                    ..
                } => {
                    // Update existing tool use entry with log message as last_line
                    if let Some(AgentActivityRow::ToolUse { tool_execution_id: _, ref mut last_line, .. }) =
                        activity_entries.iter_mut().rev().find(|entry| {
                            matches!(entry, AgentActivityRow::ToolUse { tool_execution_id: exec_id, .. } if exec_id == &tool_exec_id)
                        }) {
                        *last_line = Some(message.clone());
                        self.needs_redraw = true;
                    } else {
                        debug!("Log message for unknown tool execution ID: {}", tool_exec_id);
                    }
                }
                TaskEvent::Log {
                    message,
                    tool_execution_id: None,
                    level,
                    ..
                } => {
                    // Add general log messages as activity entries
                    use ah_domain_types::task::LogLevel;
                    let activity_entry = match level {
                        LogLevel::Error => AgentActivityRow::AgentThought {
                            thought: format!("Error: {}", message),
                        },
                        LogLevel::Warn => AgentActivityRow::AgentThought {
                            thought: format!("Warning: {}", message),
                        },
                        LogLevel::Info => AgentActivityRow::AgentThought {
                            thought: message.clone(),
                        },
                        LogLevel::Debug | LogLevel::Trace => AgentActivityRow::AgentThought {
                            thought: format!("Debug: {}", message),
                        },
                    };
                    activity_entries.push(activity_entry);
                    self.needs_redraw = true;
                }
                TaskEvent::ToolResult {
                    tool_name: _,
                    tool_output,
                    tool_execution_id,
                    status: result_status,
                    ..
                } => {
                    // Update existing tool use entry to mark as completed
                    if let Some(AgentActivityRow::ToolUse { ref mut completed, ref mut last_line, ref mut status, .. }) =
                        activity_entries.iter_mut().rev().find(|entry| {
                            matches!(entry, AgentActivityRow::ToolUse { tool_execution_id: exec_id, .. } if exec_id == &tool_execution_id)
                        }) {
                        *completed = true;
                        *status = result_status;
                        // Set last_line to first line of final output if not already set
                        if last_line.is_none() {
                            *last_line = Some(tool_output.lines().next().unwrap_or("Completed").to_string());
                        }
                        self.needs_redraw = true;
                    } else {
                        debug!("Tool result for unknown tool execution ID: {}", tool_execution_id);
                    }
                }
                TaskEvent::Status { .. } => {
                    // Status events are handled above, before the activity entry processing
                }
            };

            // Keep only the most recent N events
            let before_trim = activity_entries.len();
            while activity_entries.len() > settings.activity_rows() {
                activity_entries.remove(0);
            }
            if before_trim > activity_entries.len() {
                debug!(
                    "Trimmed {} activity entries to fit limit",
                    before_trim - activity_entries.len()
                );
                self.needs_redraw = true;
            }

            // Height remains fixed at 5 for active cards (title + separator + max 3 activity lines)
        }
    }

    /// Check if this task card needs to be redrawn
    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    /// Clear the needs_redraw flag (call after redrawing)
    pub fn clear_needs_redraw(&mut self) {
        self.needs_redraw = false;
    }
}
