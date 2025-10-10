//! Domain model for the TUI application
//!
//! Domain Model - Core Business Logic (UI-Agnostic)
//!
//! This module contains the core business state and logic for the application,
//! completely separated from UI concerns. All state mutations happen here
//! in response to domain messages.
//!
//! ## What Belongs Here:
//!
//! ✅ **Domain Entities**: `DraftTask`, `TaskExecution`, `DeliveryStatus`, `SelectedModel`, `TaskState`
//! ✅ **Business Logic**: Task creation, launching, deletion, state transitions
//! ✅ **Domain State**: Collections of tasks, current draft, available options
//! ✅ **Domain Messages**: `DomainMsg` definition and handling for business operations
//! ✅ **Network Messages**: `NetworkMsg` definition and handling for external APIs
//! ✅ **Pure Functions**: Business rules, calculations, validations
//!
//! ## What Does NOT Belong Here:
//!
//! ❌ **UI Events**: Key handling, mouse events, focus management
//! ❌ **Presentation Logic**: How things are displayed, formatted, or rendered
//! ❌ **UI State**: Selection indices, visual focus states, modal states
//! ❌ **Rendering**: Terminal drawing, styling, layout calculations
//!
//! ## Architecture Benefits:
//!
//! - **Reusable**: Same domain logic can power terminal UI, web UI, mobile app
//! - **Testable**: Business logic can be tested without UI dependencies
//! - **Maintainable**: UI changes don't affect business rules
//! - **Clear Boundaries**: Domain concerns are isolated from presentation concerns

use crate::Msg;

/// Agent/model selection with instance count
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedModel {
    pub name: String,
    pub count: usize,
}

/// Domain-level messages that are UI-agnostic and handled by the Model
#[derive(Debug, Clone, PartialEq)]
pub enum DomainMsg {
    /// Create a new draft task
    CreateDraft,
    /// Launch the current draft task
    LaunchTask,
    /// Delete a task by its combined index
    DeleteTask(usize),
    /// Update the current draft's description
    UpdateDraftText(String),
    /// Set repository for current draft
    SetRepository(String),
    /// Set branch for current draft
    SetBranch(String),
    /// Set models for current draft (names only, ViewModel handles conversion)
    SetModelNames(Vec<String>),
}

/// Network-related messages from external systems
#[derive(Debug, Clone, PartialEq)]
pub enum NetworkMsg {
    /// Repository list loaded from API
    RepositoriesLoaded(Vec<String>),
    /// Branch list loaded from API
    BranchesLoaded(Vec<String>),
    /// Model list loaded from API
    ModelsLoaded(Vec<String>),
    /// Task creation completed successfully
    TaskCreated { id: String },
    /// Task status update received
    TaskStatusUpdate { task_id: String, status: String },
    /// Task activity update received
    AgentActivityUpdate { task_id: String, activity: String },
    /// Network error occurred
    Error(String),
}

/// Task execution states as defined in PRD
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskState {
    /// Draft task being edited
    Draft,
    /// Active task running
    Active,
    /// Completed task
    Completed,
    /// Merged task
    Merged,
}

/// Draft task - represents a task being created/edited
/// Different from TaskCard as it has different lifecycle and structure
#[derive(Debug, Clone, PartialEq)]
pub struct DraftTask {
    pub id: String,
    pub description: String,
    pub repository: String,
    pub branch: String,
    pub models: Vec<SelectedModel>,
    pub created_at: String,
}

/// Task execution record - represents executed/running tasks in the domain
#[derive(Debug, Clone, PartialEq)]
pub struct TaskExecution {
    pub id: String,
    pub repository: String,
    pub branch: String,
    pub agents: Vec<SelectedModel>,
    pub state: TaskState,
    pub timestamp: String,
    pub activity: Vec<String>, // For active tasks
    pub delivery_status: Vec<DeliveryStatus>, // For completed/merged tasks
}

/// Delivery status for completed tasks
#[derive(Debug, Clone, PartialEq)]
pub enum DeliveryStatus {
    BranchCreated,
    PullRequestCreated { pr_number: u32, title: String },
    PullRequestMerged { pr_number: u32 },
}


/// Core domain model - contains only business state, no UI concerns
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    // Task management - separate collections for different types
    pub draft_tasks: Vec<DraftTask>,
    pub task_executions: Vec<TaskExecution>,

    // Current draft task state (for the draft being edited)
    pub current_draft: Option<DraftTask>,

    // Available options (loaded from API/config)
    pub available_repositories: Vec<String>,
    pub available_branches: Vec<String>,
    pub available_models: Vec<String>,

    // Application state (moved all UI state to ViewModel)

    // Settings (moved UI settings to ViewModel)
    pub activity_lines_count: usize, // 1-3 configurable activity lines
    pub word_wrap_enabled: bool,
    pub show_autocomplete_border: bool,

    // Status and error handling (moved UI messages to ViewModel)
    pub loading_states: LoadingStates,
}

/// Loading states for different async operations
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoadingStates {
    pub repositories: bool,
    pub branches: bool,
    pub models: bool,
    pub task_creation: bool,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            draft_tasks: Vec::new(),
            task_executions: Vec::new(),
            current_draft: Some(DraftTask {
                id: "current".to_string(),
                description: String::new(),
                repository: "blocksense/agent-harbor".to_string(),
                branch: "main".to_string(),
                models: vec![SelectedModel {
                    name: "Claude 3.5 Sonnet".to_string(),
                    count: 1
                }],
                created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            }),
            available_repositories: vec![
                "blocksense/agent-harbor".to_string(),
                "example/project".to_string(),
            ],
            available_branches: vec!["main".to_string(), "develop".to_string()],
            available_models: vec![
                "Claude 3.5 Sonnet".to_string(),
                "GPT-4".to_string(),
                "Claude 3 Opus".to_string(),
            ],
            activity_lines_count: 3,
            word_wrap_enabled: true,
            show_autocomplete_border: false,
            loading_states: LoadingStates::default(),
        }
    }
}

impl Model {
    /// Update the model in response to a message (pure function, no side effects)
    pub fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Key(_) => {
                // Key events now handled by ViewModel
            },
            Msg::Mouse(_) => {
                // Mouse events handled at View layer for UI interactions
                // Domain-relevant mouse events would be handled here
            },
            Msg::Tick => self.handle_tick(),
            Msg::Quit => {
                // Quit handling done at app level
            }
        }
    }

    /// Handle network messages directly
    pub fn handle_network_msg(&mut self, net_msg: NetworkMsg) {
        self.handle_network_message(net_msg)
    }

    /// Update the model in response to a domain message (pure function, no side effects)
    pub fn update_domain(&mut self, domain_msg: DomainMsg) -> Vec<DomainMsg> {
        match domain_msg {
            DomainMsg::CreateDraft => {
                self.create_new_draft_task();
                vec![]
            },
            DomainMsg::LaunchTask => {
                self.launch_task();
                vec![]
            },
            DomainMsg::DeleteTask(index) => {
                self.delete_task_by_index(index);
                vec![]
            },
            DomainMsg::UpdateDraftText(text) => {
                self.update_draft_text(text);
                vec![]
            },
            DomainMsg::SetRepository(repo) => {
                self.set_draft_repository(repo);
                vec![]
            },
            DomainMsg::SetBranch(branch) => {
                self.set_draft_branch(branch);
                vec![]
            },
            DomainMsg::SetModelNames(model_names) => {
                self.set_draft_model_names(model_names);
                vec![]
            },
        }
    }


    fn handle_tick(&mut self) {
        // Update active task activities (simulation)
        self.update_active_task_activities();

        // Clear transient status messages
        // Status message clearing is now handled by ViewModel
        // In real app, would check timestamp and clear after delay
        // For now, keeping simple
    }

    fn handle_network_message(&mut self, net_msg: NetworkMsg) {
        match net_msg {
            NetworkMsg::RepositoriesLoaded(repos) => {
                self.available_repositories = repos;
                self.loading_states.repositories = false;
            },
            NetworkMsg::BranchesLoaded(branches) => {
                self.available_branches = branches;
                self.loading_states.branches = false;
            },
            NetworkMsg::ModelsLoaded(models) => {
                self.available_models = models;
                self.loading_states.models = false;
            },
            NetworkMsg::TaskCreated { id } => {
                self.loading_states.task_creation = false;
                // Clear draft task after successful creation
                self.clear_draft_task();
            },
            NetworkMsg::TaskStatusUpdate { task_id, status: _ } => {
                if let Some(_task) = self.task_executions.iter_mut().find(|t| t.id == task_id) {
                    // Update task status based on received status
                    // This would map to TaskState enum
                }
            },
            NetworkMsg::AgentActivityUpdate { task_id, activity } => {
                if let Some(task) = self.task_executions.iter_mut().find(|t| t.id == task_id) {
                    task.add_activity(activity);
                }
            },
            NetworkMsg::Error(_error) => {
                // Clear any loading states
                self.loading_states = LoadingStates::default();
            },
        }
    }



    fn launch_task(&mut self) {
        if let Some(draft) = &self.current_draft {
            if !draft.description.trim().is_empty() && !draft.models.is_empty() {
                // Set loading state
                self.loading_states.task_creation = true;

                // In real implementation, this would send a network request
                // For now, we simulate success
            }
        }
    }

    fn create_new_draft_task(&mut self) {
        if let Some(current_draft) = &self.current_draft {
            if !current_draft.description.trim().is_empty() {
                let draft_task = DraftTask {
                    id: format!("draft_{}", chrono::Utc::now().timestamp()),
                    description: current_draft.description.clone(),
                    repository: current_draft.repository.clone(),
                    branch: current_draft.branch.clone(),
                    models: current_draft.models.clone(),
                    created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                };

                // Add draft to collection
                self.draft_tasks.insert(0, draft_task);
                // Focus navigation is handled by ViewModel now

                // Clear current draft for new input
                if let Some(ref mut draft) = self.current_draft {
                    draft.description.clear();
                }
            }
        }
    }

    /// Delete a task by its index in the combined draft + task list
    /// Returns the new total number of tasks after deletion
    pub fn delete_task_by_index(&mut self, combined_index: usize) -> usize {
        let total_drafts = self.draft_tasks.len();

        if combined_index < total_drafts {
            // Delete draft task
            self.draft_tasks.remove(combined_index);
        } else {
            // Delete regular task
            let regular_task_index = combined_index - total_drafts;
            if regular_task_index < self.task_executions.len() {
                self.task_executions.remove(regular_task_index);
            }
        }

        self.draft_tasks.len() + self.task_executions.len()
    }

    fn update_draft_text(&mut self, text: String) {
        if let Some(ref mut draft) = self.current_draft {
            draft.description = text;
        }
    }

    fn set_draft_repository(&mut self, repo: String) {
        if let Some(ref mut draft) = self.current_draft {
            draft.repository = repo;
        }
    }

    fn set_draft_branch(&mut self, branch: String) {
        if let Some(ref mut draft) = self.current_draft {
            draft.branch = branch;
        }
    }

    fn set_draft_model_names(&mut self, model_names: Vec<String>) {
        if let Some(ref mut draft) = self.current_draft {
            // Convert model names to SelectedModel with count 1
            draft.models = model_names.into_iter()
                .map(|name| SelectedModel { name, count: 1 })
                .collect();
        }
    }



    fn clear_draft_task(&mut self) {
        if let Some(ref mut draft) = self.current_draft {
            draft.description.clear();
            // Keep last used selections for convenience
            // Don't reset repository, branch, models
        }
    }

    fn update_active_task_activities(&mut self) {
        // Simulate activity updates for active tasks
        for task in self.task_executions.iter_mut() {
            if task.state == TaskState::Active {
                // In real implementation, would receive via SSE
                // For testing, simulate random activities
            }
        }
    }

    /// Get the combined list of all tasks (drafts + executions)
    pub fn all_tasks(&self) -> Vec<TaskItem> {
        let mut result = Vec::new();

        // Add all draft tasks
        for draft in &self.draft_tasks {
            result.push(TaskItem::Draft(draft.clone()));
        }

        // Add all task executions
        for (i, task) in self.task_executions.iter().enumerate() {
            result.push(TaskItem::Task(task.clone(), i));
        }

        result
    }
}

/// Enum to represent items in the unified task list
#[derive(Debug, Clone, PartialEq)]
pub enum TaskItem {
    Draft(DraftTask),
    Task(TaskExecution, usize), // TaskExecution and its original index in the task_executions vector
}

impl TaskExecution {
    /// Add activity to an active task
    pub fn add_activity(&mut self, activity: String) {
        if self.state == TaskState::Active {
            self.activity.push(activity);
            // Keep only last 10 activities for memory efficiency
            if self.activity.len() > 10 {
                self.activity.remove(0);
            }
        }
    }

    /// Get recent activity for display
    pub fn get_recent_activity(&self, count: usize) -> Vec<String> {
        if self.state == TaskState::Active {
            let recent: Vec<String> = self.activity.iter()
                .rev()
                .take(count)
                .cloned()
                .collect();
            let mut result: Vec<String> = recent.into_iter().rev().collect();

            // Always return exactly count lines, padding with empty strings at the beginning
            while result.len() < count {
                result.insert(0, String::new());
            }
            result
        } else {
            vec![String::new(); count]
        }
    }
}
