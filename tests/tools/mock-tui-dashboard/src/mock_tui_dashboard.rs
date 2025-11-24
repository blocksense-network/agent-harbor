// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! MVVM Architecture TUI Application
//!
//! This is a new implementation of the TUI using proper MVVM architecture
//! with clean separation between Model (business logic), ViewModel (UI logic),
//! and View (rendering).

use ah_core::{
    DefaultWorkspaceTermsEnumerator, RemoteBranchesEnumerator, RemoteRepositoriesEnumerator,
    TaskManager, WorkspaceFilesEnumerator, WorkspaceTermsEnumerator,
};
use ah_domain_types::ExperimentalFeature;
use ah_repo::VcsRepo;
use ah_rest_mock_client::MockRestClient;
use ah_tui::{dashboard_loop::run_dashboard, settings::Settings, view::TuiDependencies};
use ah_workflows::{WorkflowConfig, WorkflowProcessor, WorkspaceWorkflowsEnumerator};
use std::{fs::OpenOptions, sync::Arc};
use strum::IntoEnumIterator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("starting MVVM TUI application");

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
    let config = WorkflowConfig::default();
    let workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator> = Arc::new(
        WorkflowProcessor::for_repo(config, &workspace_dir)
            .unwrap_or_else(|_| WorkflowProcessor::new(WorkflowConfig::default())),
    );

    // Use real VCS repo for workspace files enumeration (fast enough for local testing)
    let workspace_files: Arc<dyn WorkspaceFilesEnumerator> =
        Arc::new(VcsRepo::new(&workspace_dir).unwrap_or_else(|_| {
            // If not in a git repo, create a minimal mock for testing
            panic!("Mock dashboard requires running from a git repository");
        }));
    let workspace_terms: Arc<dyn WorkspaceTermsEnumerator> = Arc::new(
        DefaultWorkspaceTermsEnumerator::new(Arc::clone(&workspace_files)),
    );
    let task_manager: Arc<dyn TaskManager> = Arc::new(MockRestClient::with_mock_data());
    let settings = Settings::from_config().unwrap_or_else(|_| Settings::default());

    // Create dashboard dependencies
    let mock_client = MockRestClient::with_mock_data();
    let experimental_features: Vec<ExperimentalFeature> = ExperimentalFeature::iter().collect();

    // Create local agent catalog for discovering available agents
    let local_config = ah_core::LocalAgentCatalogConfig {
        executable_paths: vec![], // Use PATH
        health_check_timeout: std::time::Duration::from_secs(1),
        query_third_party_apis: false,
        cache_ttl: std::time::Duration::from_secs(300),
        experimental_features: experimental_features.clone(),
    };
    let local_catalog = Arc::new(ah_core::LocalAgentCatalog::new(local_config));

    // Use local catalog directly for agent enumeration
    let agents_enumerator = local_catalog as Arc<dyn ah_core::AgentsEnumerator>;

    let deps = TuiDependencies {
        tui_config: ah_tui::tui_config::TuiConfig::default(),
        workspace_files,
        workspace_workflows,
        workspace_terms,
        task_manager,
        repositories_enumerator: Arc::new(RemoteRepositoriesEnumerator::new(
            mock_client.clone(),
            "mock-server".to_string(),
        )),
        branches_enumerator: Arc::new(RemoteBranchesEnumerator::new(
            mock_client,
            "mock-server".to_string(),
        )),
        agents_enumerator,
        settings,
        current_repository: None,
        experimental_features,
    };

    // Run the dashboard (handles its own signal/panic handling)
    run_dashboard(deps).await
}
