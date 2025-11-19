// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Git repository test helpers for unit and integration testing.
//!
//! This module provides utilities for creating and managing Git repositories
//! for testing purposes, similar to the Ruby `setup_repo` method but tailored
//! for Rust testing needs.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use tempfile::TempDir;
use tokio::process::Command;

/// Check if git is available on the system.
pub fn git_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Configuration options for git repository creation.
#[derive(Debug, Clone)]
pub struct GitRepoConfig {
    /// Git user email (default: "test@example.com")
    pub user_email: String,
    /// Git user name (default: "Test User")
    pub user_name: String,
    /// Whether to disable GPG signing (default: true)
    pub disable_gpg_signing: bool,
    /// Whether to create an initial commit with README.md (default: true)
    pub create_initial_commit: bool,
    /// Initial commit message (default: "Initial commit")
    pub initial_commit_message: String,
}

impl GitRepoConfig {
    /// Create a new GitRepoConfig with default values, allowing fluent configuration.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ah_repo::test_helpers::GitRepoConfig;
    ///
    /// // Use all defaults
    /// let config = GitRepoConfig::new();
    ///
    /// // Customize specific fields with fluent API
    /// let config = GitRepoConfig::new()
    ///     .user_email("custom@example.com")
    ///     .user_name("Custom User")
    ///     .create_initial_commit(false);
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the git user email.
    pub fn user_email(mut self, email: impl Into<String>) -> Self {
        self.user_email = email.into();
        self
    }

    /// Set the git user name.
    pub fn user_name(mut self, name: impl Into<String>) -> Self {
        self.user_name = name.into();
        self
    }

    /// Set whether to disable GPG signing (default: true).
    pub fn disable_gpg_signing(mut self, disable: bool) -> Self {
        self.disable_gpg_signing = disable;
        self
    }

    /// Set whether to create an initial commit (default: true).
    pub fn create_initial_commit(mut self, create: bool) -> Self {
        self.create_initial_commit = create;
        self
    }

    /// Set the initial commit message.
    pub fn initial_commit_message(mut self, message: impl Into<String>) -> Self {
        self.initial_commit_message = message.into();
        self
    }
}

impl Default for GitRepoConfig {
    fn default() -> Self {
        Self {
            user_email: "test@example.com".to_string(),
            user_name: "Test User".to_string(),
            disable_gpg_signing: true,
            create_initial_commit: true,
            initial_commit_message: "Initial commit".to_string(),
        }
    }
}

/// A Git test repository with local and remote setup.
pub struct GitTestRepo {
    /// Temporary directory containing the local repository
    pub local_repo: TempDir,
    /// Temporary directory containing the bare remote repository
    pub remote_repo: TempDir,
    /// Path to the local repository
    pub local_path: PathBuf,
    /// Path to the remote repository
    pub remote_path: PathBuf,
}

/// Information about a simple git repository (no remote).
pub struct SimpleGitRepo {
    /// Temporary directory containing the repository
    pub repo: TempDir,
    /// Path to the repository
    pub path: PathBuf,
}

/// Create a new git test repository with local and remote setup.
///
/// This is similar to the Ruby `setup_repo(:git)` method but returns
/// structured data for easier testing.
pub async fn create_git_repo_with_remote(
    config: Option<GitRepoConfig>,
) -> Result<GitTestRepo, Box<dyn std::error::Error>> {
    let config = config.unwrap_or_default();
    let remote_repo = TempDir::new()?;
    let local_repo = TempDir::new()?;

    let remote_path = remote_repo.path().to_path_buf();
    let local_path = local_repo.path().to_path_buf();

    // Create bare remote repository
    let status = Command::new("git")
        .args(["init", "--bare"])
        .current_dir(&remote_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    if !status.success() {
        return Err("Failed to create bare remote repository".into());
    }

    // Create local repository and commit
    initialize_git_repo_with_config(&local_path, &config)?;

    // Add remote
    let status = Command::new("git")
        .args(["remote", "add", "origin", &remote_path.to_string_lossy()])
        .current_dir(&local_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    if !status.success() {
        return Err("Failed to add remote".into());
    }

    Ok(GitTestRepo {
        local_repo,
        remote_repo,
        local_path,
        remote_path,
    })
}

/// Create a simple git repository in a temporary directory.
pub async fn create_git_repo(
    config: Option<GitRepoConfig>,
) -> Result<SimpleGitRepo, Box<dyn std::error::Error>> {
    let config = config.unwrap_or_default();
    let repo = TempDir::new()?;
    let path = repo.path().to_path_buf();

    initialize_git_repo_with_config(&path, &config)?;

    Ok(SimpleGitRepo { repo, path })
}

/// Initialize git repository on an existing directory with the given configuration.
pub fn initialize_git_repo_with_config(
    repo_path: &Path,
    config: &GitRepoConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize git repo
    let status = std::process::Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if !status.success() {
        return Err("Failed to initialize git repository".into());
    }

    // Configure git
    let mut config_commands = vec![
        vec!["config", "user.email", &config.user_email],
        vec!["config", "user.name", &config.user_name],
    ];

    if config.disable_gpg_signing {
        config_commands.push(vec!["config", "commit.gpgsign", "false"]);
        config_commands.push(vec!["config", "tag.gpgsign", "false"]);
    }

    for args in config_commands {
        let status = std::process::Command::new("git")
            .args(&args)
            .current_dir(repo_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        if !status.success() {
            return Err(format!("Failed to configure git: {:?}", args).into());
        }
    }

    // Create initial commit if requested
    if config.create_initial_commit {
        std::fs::write(repo_path.join("README.md"), "Initial content")?;
        let status = std::process::Command::new("git")
            .args(["add", "README.md"])
            .current_dir(repo_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        if !status.success() {
            return Err("Failed to stage initial file".into());
        }

        let output = std::process::Command::new("git")
            .args(["commit", "-m", &config.initial_commit_message])
            .current_dir(repo_path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to create initial commit: {}", stderr).into());
        }
    }

    Ok(())
}

/// Initialize git repository on an existing directory with default configuration.
pub fn initialize_git_repo(repo_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    initialize_git_repo_with_config(repo_path, &GitRepoConfig::default())
}

/// Create a commit in an existing git repository.
pub async fn create_commit(
    repo_path: &Path,
    filename: &str,
    content: &str,
    message: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    tokio::fs::write(repo_path.join(filename), content).await?;

    let status = Command::new("git")
        .args(["add", filename])
        .current_dir(repo_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    if !status.success() {
        return Err(format!("Failed to stage {}", filename).into());
    }

    let status = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(repo_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    if !status.success() {
        return Err(format!("Failed to commit: {}", message).into());
    }

    // Get the commit hash
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;

    if !output.status.success() {
        return Err("Failed to get commit hash".into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Stage a file in an existing git repository without committing.
pub async fn stage_file(
    repo_path: &Path,
    filename: &str,
    content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    tokio::fs::write(repo_path.join(filename), content).await?;

    let status = Command::new("git")
        .args(["add", filename])
        .current_dir(repo_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    if !status.success() {
        return Err(format!("Failed to stage {}", filename).into());
    }

    Ok(())
}

/// Create an uncommitted file in an existing git repository.
pub async fn create_uncommitted_file(
    repo_path: &Path,
    filename: &str,
    content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    tokio::fs::write(repo_path.join(filename), content).await?;
    Ok(())
}

/// Get the current git status (porcelain format) for an existing repository.
pub async fn git_status(repo_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;

    if !output.status.success() {
        return Err("Failed to get git status".into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Push the current branch to the remote.
pub async fn push_to_remote(repo_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(repo_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    if !status.success() {
        return Err("Failed to push to remote".into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_git_repo_creation() {
        if !git_available() {
            tracing::warn!("skipping git test: git command not available");
            return;
        }

        let repo = create_git_repo(None).await.unwrap();

        // Verify it's a git repo
        assert!(repo.path.join(".git").exists());

        // Verify initial commit exists
        let output = Command::new("git")
            .args(["log", "--oneline", "-1"])
            .current_dir(&repo.path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .unwrap();

        assert!(output.status.success());
        let log = String::from_utf8_lossy(&output.stdout);
        assert!(log.contains("Initial commit"));
    }

    #[tokio::test]
    async fn test_git_test_repo_creation() {
        if !git_available() {
            tracing::warn!("skipping git test: git command not available");
            return;
        }

        let repo = create_git_repo_with_remote(None).await.unwrap();

        // Verify local repo exists
        assert!(repo.local_path.join(".git").exists());

        // Verify remote repo exists and is bare
        assert!(repo.remote_path.join("HEAD").exists());

        // Verify remote is configured
        let output = Command::new("git")
            .args(["remote", "-v"])
            .current_dir(&repo.local_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .unwrap();

        assert!(output.status.success());
        let remotes = String::from_utf8_lossy(&output.stdout);
        assert!(remotes.contains("origin"));
    }

    #[tokio::test]
    async fn test_simple_git_repo_commit_creation() {
        if !git_available() {
            tracing::warn!("skipping git test: git command not available");
            return;
        }

        let repo = create_git_repo(None).await.unwrap();

        let commit_hash = create_commit(&repo.path, "test.txt", "test content", "Test commit")
            .await
            .unwrap();

        // Verify commit exists
        let output = Command::new("git")
            .args(["show", "--name-only", &commit_hash])
            .current_dir(&repo.path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
            .unwrap();

        assert!(output.status.success());
        let show_output = String::from_utf8_lossy(&output.stdout);
        assert!(show_output.contains("test.txt"));
        assert!(show_output.contains("Test commit"));
    }

    #[tokio::test]
    async fn test_simple_git_repo_staging() {
        if !git_available() {
            tracing::warn!("skipping git test: git command not available");
            return;
        }

        let repo = create_git_repo(None).await.unwrap();

        stage_file(&repo.path, "staged.txt", "staged content").await.unwrap();

        let status = git_status(&repo.path).await.unwrap();
        assert!(status.contains("A  staged.txt"));
    }

    #[tokio::test]
    async fn test_simple_git_repo_uncommitted() {
        if !git_available() {
            tracing::warn!("skipping git test: git command not available");
            return;
        }

        let repo = create_git_repo(None).await.unwrap();

        create_uncommitted_file(&repo.path, "uncommitted.txt", "uncommitted content")
            .await
            .unwrap();

        let status = git_status(&repo.path).await.unwrap();
        assert!(status.contains("?? uncommitted.txt"));
    }

    #[test]
    fn test_git_repo_config_builder() {
        // Test default config
        let default_config = GitRepoConfig::default();
        assert_eq!(default_config.user_email, "test@example.com");
        assert_eq!(default_config.user_name, "Test User");
        assert!(default_config.disable_gpg_signing);
        assert!(default_config.create_initial_commit);
        assert_eq!(default_config.initial_commit_message, "Initial commit");

        // Test new() with all defaults
        let new_config = GitRepoConfig::new();
        assert_eq!(new_config.user_email, default_config.user_email);
        assert_eq!(new_config.user_name, default_config.user_name);
        assert_eq!(
            new_config.disable_gpg_signing,
            default_config.disable_gpg_signing
        );
        assert_eq!(
            new_config.create_initial_commit,
            default_config.create_initial_commit
        );
        assert_eq!(
            new_config.initial_commit_message,
            default_config.initial_commit_message
        );

        // Test fluent API with customizations
        let custom_config = GitRepoConfig::new()
            .user_email("custom@example.com")
            .user_name("Custom User")
            .disable_gpg_signing(false)
            .create_initial_commit(false)
            .initial_commit_message("Custom initial commit");

        assert_eq!(custom_config.user_email, "custom@example.com");
        assert_eq!(custom_config.user_name, "Custom User");
        assert!(!custom_config.disable_gpg_signing);
        assert!(!custom_config.create_initial_commit);
        assert_eq!(
            custom_config.initial_commit_message,
            "Custom initial commit"
        );

        // Test partial customization (other fields should remain default)
        let partial_config = GitRepoConfig::new().user_email("partial@example.com");

        assert_eq!(partial_config.user_email, "partial@example.com");
        assert_eq!(partial_config.user_name, "Test User"); // default
        assert!(partial_config.disable_gpg_signing); // default
        assert!(partial_config.create_initial_commit); // default
        assert_eq!(partial_config.initial_commit_message, "Initial commit"); // default
    }
}
