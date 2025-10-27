// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! MVVM Architecture TUI Application
//!
//! This is a new implementation of the TUI using proper MVVM architecture
//! with clean separation between Model (business logic), ViewModel (UI logic),
//! and View (rendering).

use ah_core::{BranchesEnumerator, RepositoriesEnumerator, TaskManager, WorkspaceFilesEnumerator};
use ah_repo::VcsRepo;
use ah_rest_mock_client::MockRestClient;
use ah_tui::{
    dashboard_loop::{DashboardDependencies, run_dashboard},
    settings::Settings,
};
use ah_workflows::{WorkflowConfig, WorkflowProcessor, WorkspaceWorkflowsEnumerator};
use std::{fs::OpenOptions, sync::Arc};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting MVVM TUI application...");

    // Initialize tracing for key logging (disabled by default, enable with RUST_LOG=trace)
    // Output goes to tui-mvvm-trace.log file
    let trace_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("tui-mvvm-trace.log")
        .expect("Failed to open trace log file");

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(trace_file)
        .init();

    // Create mock service dependencies
    let workspace_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let workspace_files: Arc<dyn WorkspaceFilesEnumerator> =
        Arc::new(VcsRepo::new(&workspace_dir).unwrap());
    let config = WorkflowConfig::default();
    let workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator> = Arc::new(
        WorkflowProcessor::for_repo(config, &workspace_dir)
            .unwrap_or_else(|_| WorkflowProcessor::new(WorkflowConfig::default())),
    );
    let task_manager: Arc<dyn TaskManager> = Arc::new(MockRestClient::with_mock_data());
    let settings = Settings::default();

    // Create dashboard dependencies
    let deps = DashboardDependencies {
        workspace_files,
        workspace_workflows,
        task_manager,
        repositories_enumerator: Arc::new(MockRestClient::with_mock_data()),
        branches_enumerator: Arc::new(MockRestClient::with_mock_data()),
        settings,
        current_repository: None,
    };

    // Run the dashboard (handles its own signal/panic handling)
    run_dashboard(deps).await
}
