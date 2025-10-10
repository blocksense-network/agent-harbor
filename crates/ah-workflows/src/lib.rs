//! Agent Harbor Workflows - Dynamic task content and environment setup
//!
//! This crate implements the workflow commands feature for Agent Harbor tasks.
//! It processes `/command` directives and `@agents-setup` environment directives
//! in task descriptions, expanding them into dynamic content and environment variables.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use ah_repo::VcsRepo;

/// Result of processing workflow commands and environment directives
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowResult {
    /// Processed text with workflow commands expanded
    pub processed_text: String,
    /// Environment variables from @agents-setup directives
    pub environment: HashMap<String, String>,
    /// Diagnostic messages (errors, warnings)
    pub diagnostics: Vec<String>,
}

/// Errors that can occur during workflow processing
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Repository error: {0}")]
    Repo(#[from] ah_repo::VcsError),

    #[error("Script execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Command '{0}' is not in the workflow whitelist")]
    CommandNotWhitelisted(String),

    #[error("Command '{0}' not found in PATH")]
    CommandNotFoundInPath(String),

    #[error("Script not executable: {0}")]
    NotExecutable(String),
}

/// Configuration for workflow processing
#[derive(Debug, Clone)]
pub struct WorkflowConfig {
    /// Whitelisted executables that can be used as workflow commands
    pub extra_workflow_executables: Vec<String>,
    /// Base directory for repository workflows (.agents/workflows)
    pub repo_workflows_dir: Option<PathBuf>,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            // Default whitelist of common development tools
            extra_workflow_executables: vec![
                "git".to_string(),
                "cargo".to_string(),
                "npm".to_string(),
                "node".to_string(),
                "python".to_string(),
                "python3".to_string(),
                "ruby".to_string(),
                "make".to_string(),
                "docker".to_string(),
                "kubectl".to_string(),
            ],
            repo_workflows_dir: None,
        }
    }
}

/// Main workflow processor
pub struct WorkflowProcessor {
    config: WorkflowConfig,
    repo: Option<VcsRepo>,
}

impl WorkflowProcessor {
    /// Create a new workflow processor with the given configuration
    pub fn new(config: WorkflowConfig) -> Self {
        Self {
            config,
            repo: None,
        }
    }

    /// Create a workflow processor for a specific repository
    pub fn for_repo(config: WorkflowConfig, repo_path: &Path) -> Result<Self, WorkflowError> {
        let repo = VcsRepo::new(repo_path)?;
        Ok(Self {
            config,
            repo: Some(repo),
        })
    }

    /// Process text containing workflow commands and environment directives
    pub async fn process_workflows(&self, text: &str) -> Result<WorkflowResult, WorkflowError> {
        let mut env_vars: HashMap<String, EnvVarInfo> = HashMap::new();
        let mut diagnostics = Vec::new();
        let mut output_lines = Vec::new();

        for line in text.lines() {
            let line = line.trim_end();
            if line.starts_with('/') {
                // Process workflow command
                match self.process_workflow_command(line).await {
                    Ok(command_result) => {
                        diagnostics.extend(command_result.diagnostics);
                        // Process each line of command output for @agents-setup directives
                        for output_line in command_result.output.lines() {
                            self.handle_workflow_line(output_line, &mut env_vars, &mut diagnostics, &mut output_lines);
                        }
                    }
                    Err(e) => {
                        diagnostics.push(format!("Workflow error: {}", e));
                    }
                }
            } else {
                self.handle_workflow_line(line, &mut env_vars, &mut diagnostics, &mut output_lines);
            }
        }

        // Convert env_vars to final environment map
        let environment = self.finalize_environment(env_vars);

        Ok(WorkflowResult {
            processed_text: output_lines.join("\n"),
            environment,
            diagnostics,
        })
    }

    async fn process_workflow_command(&self, line: &str) -> Result<CommandResult, WorkflowError> {
        let tokens = shellwords::split(&line[1..])
            .map_err(|_| WorkflowError::ExecutionFailed("Invalid command syntax".to_string()))?;

        if tokens.is_empty() {
            return Err(WorkflowError::ExecutionFailed("Empty command".to_string()));
        }

        let cmd = &tokens[0];
        let args = &tokens[1..];

        // Check if command is whitelisted
        if !self.config.extra_workflow_executables.contains(cmd) {
            return Err(WorkflowError::CommandNotWhitelisted(cmd.clone()));
        }

        // Try repository workflows directory first (matches Ruby behavior)
        let wf_dir = if let Some(repo) = &self.repo {
            repo.root().join(".agents").join("workflows")
        } else if let Some(ref wf_dir) = self.config.repo_workflows_dir {
            wf_dir.clone()
        } else {
            // No repository workflows configured, check PATH
            if let Some(exec_path) = Self::find_in_path(cmd) {
                return self.execute_script(&exec_path, args).await;
            }
            return Err(WorkflowError::CommandNotFoundInPath(cmd.clone()));
        };

        if wf_dir.exists() {
            // Check for executable script
            let script_path = wf_dir.join(cmd);
            if script_path.exists() {
                return self.execute_script(&script_path, args).await;
            }

            // Check for text file fallback
            let txt_path = wf_dir.join(format!("{}.txt", cmd));
            if txt_path.exists() {
                let content = tokio::fs::read_to_string(&txt_path).await?;
                return Ok(CommandResult {
                    output: content,
                    diagnostics: vec![],
                });
            }
        }

        // Repository workflows not found, check PATH as fallback
        if let Some(exec_path) = Self::find_in_path(cmd) {
            return self.execute_script(&exec_path, args).await;
        }

        Err(WorkflowError::CommandNotFoundInPath(cmd.clone()))
    }

    async fn execute_script(&self, script_path: &Path, args: &[String]) -> Result<CommandResult, WorkflowError> {
        // Make script executable if needed (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = script_path.metadata()?;
            let mut permissions = metadata.permissions();
            if permissions.mode() & 0o111 == 0 {
                permissions.set_mode(0o755);
                tokio::fs::set_permissions(script_path, permissions).await?;
            }
        }

        let output = Command::new(script_path)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut diagnostics = Vec::new();
        if !output.status.success() {
            diagnostics.push(format!("$ {} {}\n{}", script_path.display(), args.join(" "), stderr));
        }

        Ok(CommandResult {
            output: stdout,
            diagnostics,
        })
    }

    fn find_in_path(command: &str) -> Option<PathBuf> {
        std::env::var_os("PATH").and_then(|path_var| {
            std::env::split_paths(&path_var)
                .find_map(|dir| {
                    let candidate = dir.join(command);
                    if candidate.is_file() && is_executable(&candidate) {
                        Some(candidate)
                    } else {
                        None
                    }
                })
        })
    }

    fn handle_workflow_line(
        &self,
        line: &str,
        env_vars: &mut HashMap<String, EnvVarInfo>,
        diagnostics: &mut Vec<String>,
        output_lines: &mut Vec<String>,
    ) {
        if let Some(rest) = line.strip_prefix("@agents-setup ") {
            for pair in shellwords::split(rest).unwrap_or_default() {
                let (op, var, val) = if pair.contains("+=") {
                    let parts: Vec<&str> = pair.splitn(2, "+=").collect();
                    if parts.len() == 2 {
                        ("+=", parts[0], parts[1])
                    } else {
                        continue;
                    }
                } else if pair.contains('=') {
                    let parts: Vec<&str> = pair.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        ("=", parts[0], parts[1])
                    } else {
                        continue;
                    }
                } else {
                    continue;
                };

                let entry = env_vars.entry(var.to_string()).or_insert(EnvVarInfo {
                    direct: None,
                    append: Vec::new(),
                });

                if op == "=" {
                    if entry.direct.is_some() && entry.direct.as_ref().unwrap() != val {
                        diagnostics.push(format!("Conflicting assignment for {}", var));
                    } else {
                        entry.direct = Some(val.to_string());
                    }
                } else {
                    entry.append.extend(val.split(',').map(|s| s.trim().to_string()));
                }
            }
        } else {
            output_lines.push(line.to_string());
        }
    }

    fn finalize_environment(&self, env_vars: HashMap<String, EnvVarInfo>) -> HashMap<String, String> {
        env_vars.into_iter()
            .map(|(var, info)| {
                let mut values = Vec::new();
                if let Some(direct) = info.direct {
                    values.extend(direct.split(',').map(|s| s.trim().to_string()));
                }
                values.extend(info.append);

                // Deduplicate while preserving order (like Ruby's uniq)
                let mut seen = std::collections::HashSet::new();
                let deduplicated: Vec<String> = values.into_iter()
                    .filter(|v| seen.insert(v.clone()))
                    .collect();

                let final_value = deduplicated.join(",");
                (var, final_value)
            })
            .collect()
    }
}

#[derive(Debug)]
struct CommandResult {
    output: String,
    diagnostics: Vec<String>,
}

#[derive(Debug)]
struct EnvVarInfo {
    direct: Option<String>,
    append: Vec<String>,
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        // On Windows, check for .exe, .bat, .cmd extensions
        if let Some(extension) = path.extension() {
            let ext = extension.to_string_lossy().to_lowercase();
            matches!(ext.to_string().as_str(), "exe" | "bat" | "cmd")
        } else {
            false
        }
    }
}

mod shellwords {
    /// Simple shell-like word splitting (similar to Ruby's Shellwords.split)
    pub fn split(s: &str) -> Result<Vec<String>, &'static str> {
        let mut result = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '"' => {
                    in_quotes = !in_quotes;
                }
                ' ' | '\t' if !in_quotes => {
                    if !current.is_empty() {
                        result.push(current);
                        current = String::new();
                    }
                }
                '\\' => {
                    if let Some(next_ch) = chars.next() {
                        current.push(next_ch);
                    }
                }
                _ => {
                    current.push(ch);
                }
            }
        }

        if in_quotes {
            return Err("Unclosed quote");
        }

        if !current.is_empty() {
            result.push(current);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_process_workflows_basic() {
        let config = WorkflowConfig::default();
        let processor = WorkflowProcessor::new(config);

        let input = "This is a test task.\n@agents-setup TEST_VAR=test_value\nAnother line.";
        let result = processor.process_workflows(input).await.unwrap();

        assert_eq!(result.processed_text, "This is a test task.\nAnother line.");
        assert_eq!(result.environment.get("TEST_VAR"), Some(&"test_value".to_string()));
        assert!(result.diagnostics.is_empty());
    }

    #[tokio::test]
    async fn test_process_workflows_with_append() {
        let config = WorkflowConfig::default();
        let processor = WorkflowProcessor::new(config);

        let input = "@agents-setup VAR=base\n@agents-setup VAR+=extra\n@agents-setup VAR+=more";
        let result = processor.process_workflows(input).await.unwrap();

        assert_eq!(result.environment.get("VAR"), Some(&"base,extra,more".to_string()));
    }

    #[tokio::test]
    async fn test_process_workflows_conflicting_assignment() {
        let config = WorkflowConfig::default();
        let processor = WorkflowProcessor::new(config);

        let input = "@agents-setup CONFLICT=first\n@agents-setup CONFLICT=second";
        let result = processor.process_workflows(input).await.unwrap();

        assert!(result.diagnostics.iter().any(|d| d.contains("Conflicting assignment")));
    }

    #[tokio::test]
    async fn test_process_workflows_text_file_fallback() {
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir(&repo_dir).unwrap();

        // Initialize a git repo
        std::process::Command::new("git")
            .args(&["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        // Create workflows directory and text file
        let wf_dir = repo_dir.join(".agents").join("workflows");
        std::fs::create_dir_all(&wf_dir).unwrap();
        std::fs::write(wf_dir.join("test.txt"), "Text file content").unwrap();

        let config = WorkflowConfig {
            extra_workflow_executables: vec!["test".to_string()],
            repo_workflows_dir: Some(wf_dir),
        };

        let processor = WorkflowProcessor::for_repo(config, &repo_dir).unwrap();
        let result = processor.process_workflows("/test").await.unwrap();

        assert_eq!(result.processed_text, "Text file content");
    }

    #[tokio::test]
    async fn test_workflow_expansion_and_env() {
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir(&repo_dir).unwrap();

        // Initialize git repo
        std::process::Command::new("git")
            .args(&["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(&["config", "user.email", "test@example.com"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(&["config", "user.name", "Test User"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        // Create workflows directory and scripts
        let wf_dir = repo_dir.join(".agents").join("workflows");
        std::fs::create_dir_all(&wf_dir).unwrap();

        let hello_script = wf_dir.join("hello");
        std::fs::write(&hello_script, "#!/bin/sh\necho hello\necho '@agents-setup FOO=bar'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&hello_script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&hello_script, perms).unwrap();
        }

        let bye_script = wf_dir.join("bye");
        std::fs::write(&bye_script, "#!/bin/sh\necho bye\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bye_script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bye_script, perms).unwrap();
        }

        let config = WorkflowConfig {
            extra_workflow_executables: vec!["hello".to_string(), "bye".to_string()],
            repo_workflows_dir: Some(wf_dir),
        };
        let processor = WorkflowProcessor::for_repo(config, &repo_dir).unwrap();

        let input = "/hello\nThis task uses two workflows.\n/bye\n@agents-setup BAZ=1";
        let result = processor.process_workflows(input).await.unwrap();

        // Should contain output from both scripts
        assert!(result.processed_text.contains("hello"));
        assert!(result.processed_text.contains("bye"));
        assert!(result.processed_text.contains("This task uses two workflows."));

        // Should not contain setup directives in processed text
        assert!(!result.processed_text.contains("@agents-setup"));

        // Should have environment variables
        assert_eq!(result.environment.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(result.environment.get("BAZ"), Some(&"1".to_string()));
    }

    #[tokio::test]
    async fn test_ruby_workflow_command() {
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir(&repo_dir).unwrap();

        // Initialize git repo
        std::process::Command::new("git")
            .args(&["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        // Create workflows directory
        let wf_dir = repo_dir.join(".agents").join("workflows");
        std::fs::create_dir_all(&wf_dir).unwrap();

        let ruby_script = wf_dir.join("ruby_wf");
        std::fs::write(&ruby_script, "#!/usr/bin/env ruby\nputs 'ruby works'\nputs '@agents-setup RUBY_FLAG=1'").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&ruby_script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&ruby_script, perms).unwrap();
        }

        let config = WorkflowConfig {
            extra_workflow_executables: vec!["ruby_wf".to_string()],
            repo_workflows_dir: Some(wf_dir),
        };
        let processor = WorkflowProcessor::for_repo(config, &repo_dir).unwrap();

        let input = "This task demonstrates a ruby workflow.\n/ruby_wf";
        let result = processor.process_workflows(input).await.unwrap();

        assert!(result.processed_text.contains("ruby works"));
        assert!(result.processed_text.contains("This task demonstrates a ruby workflow."));
        assert_eq!(result.environment.get("RUBY_FLAG"), Some(&"1".to_string()));
    }

    #[tokio::test]
    async fn test_text_workflow_command() {
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir(&repo_dir).unwrap();

        // Initialize git repo
        std::process::Command::new("git")
            .args(&["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        // Create workflows directory and text file
        let wf_dir = repo_dir.join(".agents").join("workflows");
        std::fs::create_dir_all(&wf_dir).unwrap();
        std::fs::write(wf_dir.join("info.txt"), "hello from txt").unwrap();

        let config = WorkflowConfig {
            extra_workflow_executables: vec!["info".to_string()],
            repo_workflows_dir: Some(wf_dir),
        };
        let processor = WorkflowProcessor::for_repo(config, &repo_dir).unwrap();

        let input = "/info\nSome additional details about the task.";
        let result = processor.process_workflows(input).await.unwrap();

        assert!(result.processed_text.contains("hello from txt"));
        assert!(result.processed_text.contains("Some additional details about the task."));
    }

    #[tokio::test]
    async fn test_workflow_with_arguments() {
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir(&repo_dir).unwrap();

        // Initialize git repo
        std::process::Command::new("git")
            .args(&["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        // Create workflows directory and script
        let wf_dir = repo_dir.join(".agents").join("workflows");
        std::fs::create_dir_all(&wf_dir).unwrap();

        let echo_script = wf_dir.join("echo_args");
        std::fs::write(&echo_script, "#!/bin/sh\necho \"$1 $2\"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&echo_script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&echo_script, perms).unwrap();
        }

        let config = WorkflowConfig {
            extra_workflow_executables: vec!["echo_args".to_string()],
            repo_workflows_dir: Some(wf_dir),
        };
        let processor = WorkflowProcessor::for_repo(config, &repo_dir).unwrap();

        let input = "Before running commands.\n/echo_args foo \"bar baz\"\n/echo_args qux quux\nAfter commands.";
        let result = processor.process_workflows(input).await.unwrap();

        assert!(result.processed_text.contains("Before running commands."));
        assert!(result.processed_text.contains("foo bar baz"));
        assert!(result.processed_text.contains("qux quux"));
        assert!(result.processed_text.contains("After commands."));
    }

    #[tokio::test]
    async fn test_setup_script_receives_env_vars() {
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir(&repo_dir).unwrap();

        // Initialize git repo
        std::process::Command::new("git")
            .args(&["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        // Create workflows directory and script
        let wf_dir = repo_dir.join(".agents").join("workflows");
        std::fs::create_dir_all(&wf_dir).unwrap();

        let envgen_script = wf_dir.join("envgen");
        std::fs::write(&envgen_script, "#!/bin/sh\necho '@agents-setup FOO=BAR'").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&envgen_script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&envgen_script, perms).unwrap();
        }

        let config = WorkflowConfig {
            extra_workflow_executables: vec!["envgen".to_string()],
            repo_workflows_dir: Some(wf_dir),
        };
        let processor = WorkflowProcessor::for_repo(config, &repo_dir).unwrap();

        let input = "Prepare env.\n/envgen\nDone.";
        let result = processor.process_workflows(input).await.unwrap();

        assert!(result.processed_text.contains("Prepare env."));
        assert!(result.processed_text.contains("Done."));
        assert_eq!(result.environment.get("FOO"), Some(&"BAR".to_string()));
    }

    #[tokio::test]
    async fn test_unknown_workflow_command() {
        let temp_dir = TempDir::new().unwrap();
        let repo_dir = temp_dir.path().join("repo");
        std::fs::create_dir(&repo_dir).unwrap();

        // Initialize git repo
        std::process::Command::new("git")
            .args(&["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        let config = WorkflowConfig::default();
        let processor = WorkflowProcessor::for_repo(config, &repo_dir).unwrap();

        let input = "/missing\nTrailing text";
        let result = processor.process_workflows(input).await.unwrap();

        assert!(result.processed_text.contains("Trailing text"));
        assert!(result.diagnostics.iter().any(|d| d.contains("not in the workflow whitelist")));
    }

    #[tokio::test]
    async fn test_conflicting_env_assignments() {
        let config = WorkflowConfig::default();
        let processor = WorkflowProcessor::new(config);

        let input = "@agents-setup VAR=1\n@agents-setup VAR=2";
        let result = processor.process_workflows(input).await.unwrap();

        assert!(result.diagnostics.iter().any(|d| d.contains("Conflicting assignment")));
    }

    #[tokio::test]
    async fn test_assignment_with_appends() {
        let config = WorkflowConfig::default();
        let processor = WorkflowProcessor::new(config);

        let input = "@agents-setup VAR=base\n@agents-setup VAR+=extra";
        let result = processor.process_workflows(input).await.unwrap();

        assert_eq!(result.environment.get("VAR"), Some(&"base,extra".to_string()));
        assert!(result.diagnostics.is_empty());
    }

    #[tokio::test]
    async fn test_append_only_combines_values() {
        let config = WorkflowConfig::default();
        let processor = WorkflowProcessor::new(config);

        let input = "@agents-setup VAR+=one\n@agents-setup VAR+=two";
        let result = processor.process_workflows(input).await.unwrap();

        assert_eq!(result.environment.get("VAR"), Some(&"one,two".to_string()));
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_shellwords_split() {
        use super::shellwords::split;

        assert_eq!(split("hello world"), Ok(vec!["hello".to_string(), "world".to_string()]));
        assert_eq!(split("hello \"world test\""), Ok(vec!["hello".to_string(), "world test".to_string()]));
        assert_eq!(split("cmd arg1 arg2"), Ok(vec!["cmd".to_string(), "arg1".to_string(), "arg2".to_string()]));
        assert!(split("unclosed \"quote").is_err());
    }
}
