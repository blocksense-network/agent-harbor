// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared test utilities for TUI tests

use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ah_core::{
    BranchesEnumerator, DefaultWorkspaceTermsEnumerator, MockWorkspaceTermsEnumerator,
    RepositoriesEnumerator, TaskManager, WorkspaceFilesEnumerator, WorkspaceTermsEnumerator,
};
use ah_rest_mock_client::MockRestClient;
use ah_tui::settings::Settings;
use ah_tui::view_model::ViewModel;
use ah_workflows::{
    WorkflowCommand, WorkflowCommandSource, WorkflowError, WorkspaceWorkflowsEnumerator,
};
use futures::stream::{self, StreamExt};
use std::path::PathBuf;

/// Simple test implementation of WorkspaceFilesEnumerator for testing
#[derive(Clone)]
pub struct TestWorkspaceFilesEnumerator {
    pub files: Vec<String>,
}

impl TestWorkspaceFilesEnumerator {
    pub fn new(files: Vec<String>) -> Self {
        Self { files }
    }
}

#[async_trait::async_trait]
impl WorkspaceFilesEnumerator for TestWorkspaceFilesEnumerator {
    async fn stream_repository_files(
        &self,
    ) -> Result<ah_core::FileStream, ah_core::RepositoryError> {
        let files = self
            .files
            .clone()
            .into_iter()
            .map(|path| ah_core::RepositoryFile {
                path,
                detail: Some("Test file".to_string()),
            })
            .collect::<Vec<_>>();
        let stream = stream::iter(files.into_iter().map(Ok));
        Ok(Box::pin(stream))
    }
}

/// Simple test implementation of WorkspaceWorkflowsEnumerator for testing
#[derive(Clone)]
pub struct TestWorkspaceWorkflowsEnumerator {
    pub workflows: Vec<String>,
}

impl TestWorkspaceWorkflowsEnumerator {
    pub fn new(workflows: Vec<String>) -> Self {
        Self { workflows }
    }
}

#[async_trait::async_trait]
impl WorkspaceWorkflowsEnumerator for TestWorkspaceWorkflowsEnumerator {
    async fn enumerate_workflow_commands(
        &self,
    ) -> std::result::Result<Vec<WorkflowCommand>, WorkflowError> {
        let commands = self
            .workflows
            .iter()
            .map(|name| WorkflowCommand {
                name: name.clone(),
                source: WorkflowCommandSource::Script(PathBuf::from(format!(
                    ".agents/workflows/{}.sh",
                    name
                ))),
                description: Some(format!("Test workflow: {}", name)),
            })
            .collect();
        Ok(commands)
    }
}

/// Create a temporary log file for test debugging
pub fn create_test_log(test_name: &str) -> (std::fs::File, std::path::PathBuf) {
    let mut dir = std::env::temp_dir();
    dir.push("ah_tui_vm_logs");
    std::fs::create_dir_all(&dir).expect("create log directory");

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("valid time");
    let file_name = format!(
        "{}_{}_{}.log",
        test_name,
        std::process::id(),
        timestamp.as_nanos()
    );
    dir.push(file_name);
    let file = std::fs::File::create(&dir).expect("create log file");
    (file, dir)
}

/// Build a ViewModel with test enumerators for predictable test data
pub fn build_view_model() -> ViewModel {
    // Use test enumerators for predictable test data
    let workspace_files: Arc<dyn WorkspaceFilesEnumerator> =
        Arc::new(TestWorkspaceFilesEnumerator::new(vec![
            "src/main.rs".to_string(),
            "Cargo.toml".to_string(),
        ]));
    let workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator> =
        Arc::new(TestWorkspaceWorkflowsEnumerator::new(vec![
            "test-workflow".to_string(),
            "another-workflow".to_string(),
        ]));
    let workspace_terms: Arc<dyn WorkspaceTermsEnumerator> = Arc::new(
        DefaultWorkspaceTermsEnumerator::new(Arc::clone(&workspace_files)),
    );
    let task_manager = Arc::new(MockRestClient::new());
    let mock_client = MockRestClient::new();
    let repositories_enumerator: Arc<dyn RepositoriesEnumerator> = Arc::new(
        ah_core::RemoteRepositoriesEnumerator::new(mock_client.clone(), "http://test".to_string()),
    );
    let branches_enumerator: Arc<dyn BranchesEnumerator> = Arc::new(
        ah_core::RemoteBranchesEnumerator::new(mock_client, "http://test".to_string()),
    );
    let settings = Settings::from_config().unwrap_or_else(|_| Settings::default());
    let (ui_tx, _ui_rx) = crossbeam_channel::unbounded();

    ViewModel::new(
        workspace_files,
        workspace_workflows,
        workspace_terms,
        task_manager,
        repositories_enumerator,
        branches_enumerator,
        settings,
        ui_tx,
    )
}

pub fn build_view_model_with_terms(terms: Vec<String>) -> ViewModel {
    build_view_model_with_terms_and_settings(terms, Settings::default())
}

pub fn build_view_model_with_terms_and_settings(
    terms: Vec<String>,
    settings: Settings,
) -> ViewModel {
    let workspace_files: Arc<dyn WorkspaceFilesEnumerator> =
        Arc::new(TestWorkspaceFilesEnumerator::new(vec![
            "src/main.rs".to_string(),
            "Cargo.toml".to_string(),
        ]));
    let workspace_workflows: Arc<dyn WorkspaceWorkflowsEnumerator> =
        Arc::new(TestWorkspaceWorkflowsEnumerator::new(vec![
            "test-workflow".to_string(),
            "another-workflow".to_string(),
        ]));
    let workspace_terms: Arc<dyn WorkspaceTermsEnumerator> =
        Arc::new(MockWorkspaceTermsEnumerator::new(terms));
    let task_manager = Arc::new(MockRestClient::new());
    let mock_client = MockRestClient::new();
    let repositories_enumerator: Arc<dyn RepositoriesEnumerator> = Arc::new(
        ah_core::RemoteRepositoriesEnumerator::new(mock_client.clone(), "http://test".to_string()),
    );
    let branches_enumerator: Arc<dyn BranchesEnumerator> = Arc::new(
        ah_core::RemoteBranchesEnumerator::new(mock_client, "http://test".to_string()),
    );
    let (ui_tx, _ui_rx) = crossbeam_channel::unbounded();

    ViewModel::new(
        workspace_files,
        workspace_workflows,
        workspace_terms,
        task_manager,
        repositories_enumerator,
        branches_enumerator,
        settings,
        ui_tx,
    )
}

/// Build a ViewModel with additional repository/branch data for modal tests
pub fn build_view_model_with_repos() -> ViewModel {
    let mut vm = build_view_model();

    // For tests, synchronously populate the available repositories and branches
    // since background loading doesn't run in test environment
    vm.available_repositories = vec![
        "myapp/backend".to_string(),
        "myapp/frontend".to_string(),
        "myapp/mobile".to_string(),
    ];
    vm.available_branches = vec![
        "main".to_string(),
        "develop".to_string(),
        "feature/auth".to_string(),
    ];

    vm
}
