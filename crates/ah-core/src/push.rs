// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_repo::VcsRepo;
use anyhow::{Context, Result};
use std::io::{self, Write};

/// Push options for controlling push behavior
#[derive(Debug, Clone)]
pub struct PushOptions {
    /// Whether to push to remote (None = interactive prompt, Some(bool) = explicit)
    pub push_to_remote: Option<bool>,
    /// The branch name to push
    pub branch_name: String,
    /// The remote name to push to (defaults to "origin")
    pub remote: String,
}

impl PushOptions {
    /// Create new push options
    pub fn new(branch_name: String) -> Self {
        Self {
            push_to_remote: None,
            branch_name,
            remote: "origin".to_string(),
        }
    }

    /// Set the push to remote flag
    pub fn with_push_to_remote(mut self, push: Option<bool>) -> Self {
        self.push_to_remote = push;
        self
    }

    /// Set the remote name
    pub fn with_remote(mut self, remote: String) -> Self {
        self.remote = remote;
        self
    }
}

/// Handle push operations with interactive prompts and VCS integration
pub struct PushHandler {
    repo: VcsRepo,
}

impl PushHandler {
    /// Create a new push handler for the given repository
    pub async fn new<P: AsRef<std::path::Path>>(repo_path: P) -> Result<Self> {
        let repo = VcsRepo::new(repo_path).context("Failed to create VCS repository instance")?;
        Ok(Self { repo })
    }

    /// Handle push operations according to the provided options
    ///
    /// This replicates the push logic from the Ruby implementation:
    /// - Parse boolean flag or prompt interactively
    /// - Execute push operation if requested
    pub async fn handle_push(&self, options: &PushOptions) -> Result<()> {
        let should_push = self.determine_push_behavior(options).await?;

        if should_push {
            self.execute_push(&options.branch_name, &options.remote).await?;
        }

        Ok(())
    }

    /// Determine whether to push based on options (interactive or explicit)
    async fn determine_push_behavior(&self, options: &PushOptions) -> Result<bool> {
        match options.push_to_remote {
            Some(push) => Ok(push),
            None => self.prompt_for_push().await,
        }
    }

    /// Prompt the user interactively for push decision
    async fn prompt_for_push(&self) -> Result<bool> {
        print!("Push to default remote? [Y/n]: ");
        io::stdout().flush().context("Failed to flush stdout")?;

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => {
                // EOF reached (non-interactive environment)
                Err(anyhow::anyhow!(
                    "Error: Non-interactive environment, use --push-to-remote option."
                ))
            }
            Ok(_) => {
                let answer = input.trim();
                let answer = if answer.is_empty() { "y" } else { answer };
                Ok(answer.to_lowercase().starts_with('y'))
            }
            Err(e) => Err(anyhow::anyhow!("Failed to read input: {}", e)),
        }
    }

    /// Execute the push operation using the VCS repository
    async fn execute_push(&self, branch_name: &str, remote: &str) -> Result<()> {
        self.repo
            .push_current_branch(branch_name, remote)
            .context("Failed to push branch to remote")?;
        Ok(())
    }

    /// Get the default remote HTTP URL for commit messages
    pub async fn default_remote_http_url(&self) -> Result<Option<String>> {
        self.repo
            .default_remote_http_url()
            .context("Failed to get default remote HTTP URL")
    }
}

/// Parse a string value into a boolean for push-to-remote flag
///
/// This replicates the Ruby boolean parsing logic:
/// - "1", "true", "yes", "y" (case insensitive) are truthy
/// - "0", "false", "no", "n" (case insensitive) are falsy
/// - Anything else is invalid
pub fn parse_push_to_remote_flag(value: &str) -> Result<bool> {
    let value = value.to_lowercase();
    let truthy = ["1", "true", "yes", "y"];
    let falsy = ["0", "false", "no", "n"];

    if truthy.contains(&value.as_str()) {
        Ok(true)
    } else if falsy.contains(&value.as_str()) {
        Ok(false)
    } else {
        Err(anyhow::anyhow!(
            "Error: Invalid value for --push-to-remote: '{}'. Valid values are: {}",
            value,
            truthy.iter().chain(falsy.iter()).copied().collect::<Vec<_>>().join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_push_to_remote_flag_truthy() {
        assert!(parse_push_to_remote_flag("1").unwrap());
        assert!(parse_push_to_remote_flag("true").unwrap());
        assert!(parse_push_to_remote_flag("yes").unwrap());
        assert!(parse_push_to_remote_flag("y").unwrap());
        assert!(parse_push_to_remote_flag("YES").unwrap());
        assert!(parse_push_to_remote_flag("True").unwrap());
    }

    #[test]
    fn test_parse_push_to_remote_flag_falsy() {
        assert!(!parse_push_to_remote_flag("0").unwrap());
        assert!(!parse_push_to_remote_flag("false").unwrap());
        assert!(!parse_push_to_remote_flag("no").unwrap());
        assert!(!parse_push_to_remote_flag("n").unwrap());
        assert!(!parse_push_to_remote_flag("NO").unwrap());
        assert!(!parse_push_to_remote_flag("False").unwrap());
    }

    #[test]
    fn test_parse_push_to_remote_flag_invalid() {
        assert!(parse_push_to_remote_flag("maybe").is_err());
        assert!(parse_push_to_remote_flag("invalid").is_err());
        assert!(parse_push_to_remote_flag("").is_err());
    }

    #[test]
    fn test_push_options_builder() {
        let options = PushOptions::new("feature-branch".to_string())
            .with_push_to_remote(Some(true))
            .with_remote("upstream".to_string());

        assert_eq!(options.branch_name, "feature-branch");
        assert_eq!(options.push_to_remote, Some(true));
        assert_eq!(options.remote, "upstream");
    }

    // Note: Interactive prompt tests and actual push tests would require
    // setting up actual git repositories and are better tested in integration tests
}
