// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use anyhow::{Context, Result};
use serde::Deserialize;
use serial_test::serial;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

#[derive(Debug, Deserialize)]
struct Scenario {
    name: String,
    #[serde(default)]
    repo: RepoSpec,
    timeline: Vec<ScenarioEvent>,
}

#[derive(Debug, Default, Deserialize)]
struct RepoSpec {
    #[serde(default)]
    init: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ScenarioEvent {
    UserCommand {
        #[serde(rename = "userCommand")]
        user_command: UserCommand,
    },
    AssertGit {
        #[serde(rename = "assertGit")]
        assert_git: AssertGit,
    },
}

#[derive(Debug, Deserialize)]
struct UserCommand {
    #[serde(default)]
    program: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    expect: Option<Expect>,
}

#[derive(Debug, Default, Deserialize)]
struct Expect {
    #[serde(default, rename = "exitCode")]
    exit_code: Option<i32>,
    #[serde(default, rename = "stdoutContains")]
    stdout_contains: Option<String>,
    #[serde(default, rename = "stderrContains")]
    stderr_contains: Option<String>,
    #[serde(default)]
    fs: Option<FsExpect>,
}

#[derive(Debug, Default, Deserialize)]
struct FsExpect {
    #[serde(default)]
    exists: Vec<String>,
    #[serde(default, rename = "notExists")]
    not_exists: Vec<String>,
    #[serde(default)]
    contains: Vec<FileContains>,
}

#[derive(Debug, Deserialize)]
struct FileContains {
    path: String,
    substring: String,
}

#[derive(Debug, Deserialize)]
struct AssertGit {
    branch: String,
    #[serde(default)]
    exists: Option<bool>,
    #[serde(default, rename = "commitsAheadOf")]
    commits_ahead_of: Option<String>,
    #[serde(default, rename = "commitCount")]
    commit_count: Option<u32>,
    #[serde(default, rename = "headMessageContains")]
    head_message_contains: Option<String>,
    #[serde(default, rename = "remoteExists")]
    remote_exists: Option<bool>,
}

struct ScenarioContext {
    _home_dir: TempDir,
    _ah_home: TempDir,
    _remote_dir: TempDir,
    repo_dir: TempDir,
    ah_bin: PathBuf,
}

#[test]
#[serial]
fn scenario_task_create_new_branch() -> Result<()> {
    run_task_scenario("task_create_new_branch")
}

#[test]
#[serial]
fn scenario_task_follow_up_append() -> Result<()> {
    run_task_scenario("task_follow_up_append")
}

#[test]
#[serial]
fn scenario_task_metadata_only_commit() -> Result<()> {
    run_task_scenario("task_metadata_only_commit")
}

#[test]
#[serial]
fn scenario_task_push_behaviors() -> Result<()> {
    run_task_scenario("task_push_behaviors")
}

#[test]
#[serial]
fn scenario_task_multi_agent_follow() -> Result<()> {
    run_task_scenario("task_multi_agent_follow")
}

#[test]
#[serial]
fn scenario_task_delivery_pr_requires_push() -> Result<()> {
    run_task_scenario("task_delivery_pr_requires_push")
}

#[test]
#[serial]
fn scenario_task_browser_disabled() -> Result<()> {
    run_task_scenario("task_browser_disabled")
}

#[test]
#[serial]
fn scenario_task_follow_tmux() -> Result<()> {
    run_task_scenario("task_follow_tmux")
}

fn run_task_scenario(name: &str) -> Result<()> {
    let scenario_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("tools")
        .join("mock-agent")
        .join("scenarios")
        .join(format!("{name}.yaml"));

    let scenario: Scenario = serde_yaml::from_str(
        &fs::read_to_string(&scenario_path)
            .with_context(|| format!("Failed to read scenario {}", scenario_path.display()))?,
    )
    .with_context(|| format!("Failed to parse scenario {}", scenario_path.display()))?;

    if !scenario.repo.init {
        anyhow::bail!("Scenario {} requires repo.init=true", scenario.name);
    }

    let ctx = setup_context()?;

    for event in scenario.timeline {
        match event {
            ScenarioEvent::UserCommand { user_command } => {
                run_user_command(&user_command, &ctx)?;
            }
            ScenarioEvent::AssertGit { assert_git } => {
                assert_git_state(&assert_git, &ctx)?;
            }
        }
    }

    Ok(())
}

fn setup_context() -> Result<ScenarioContext> {
    let home_dir = TempDir::new().context("failed to create temp HOME")?;
    std::env::set_var("HOME", home_dir.path());

    let ah_home = reset_ah_home()?;
    let (repo_dir, remote_dir) = init_git_repo().context("failed to create git repo")?;

    let ah_bin = ah_binary_path();

    Ok(ScenarioContext {
        _home_dir: home_dir,
        _ah_home: ah_home,
        _remote_dir: remote_dir,
        repo_dir,
        ah_bin,
    })
}

fn ah_binary_path() -> PathBuf {
    if let Ok(bin) = std::env::var("CARGO_BIN_EXE_ah") {
        return PathBuf::from(bin);
    }

    // Fallback to workspace target/debug
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("debug")
        .join("ah")
}

fn reset_ah_home() -> Result<TempDir> {
    let temp_dir = TempDir::new().context("failed to create AH_HOME dir")?;
    std::env::set_var("AH_HOME", temp_dir.path());
    Ok(temp_dir)
}

fn init_git_repo() -> Result<(TempDir, TempDir)> {
    let repo_dir = TempDir::new().context("failed to create repo temp dir")?;
    let remote_dir = TempDir::new().context("failed to create remote temp dir")?;

    Command::new("git")
        .args(["init", "--bare"])
        .current_dir(remote_dir.path())
        .output()
        .context("failed to init bare remote repo")?;

    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_dir.path())
        .output()
        .context("failed to init git repo")?;

    Command::new("git")
        .args(["config", "user.email", "tester@example.com"])
        .current_dir(repo_dir.path())
        .output()
        .context("failed to set git user.email")?;
    Command::new("git")
        .args(["config", "user.name", "Tester"])
        .current_dir(repo_dir.path())
        .output()
        .context("failed to set git user.name")?;

    fs::write(repo_dir.path().join("README.md"), "initial")
        .context("failed to write seed README")?;
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo_dir.path())
        .output()
        .context("failed to add README.md")?;
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(repo_dir.path())
        .output()
        .context("failed to commit seed README")?;

    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            &remote_dir.path().to_string_lossy(),
        ])
        .current_dir(repo_dir.path())
        .output()
        .context("failed to add origin remote")?;

    Ok((repo_dir, remote_dir))
}

fn run_user_command(cmd: &UserCommand, ctx: &ScenarioContext) -> Result<()> {
    let program = cmd.program.clone().unwrap_or_else(|| "ah".to_string());

    let mut command = if program == "ah" {
        Command::new(&ctx.ah_bin)
    } else {
        Command::new(&program)
    };

    command
        .current_dir(ctx.repo_dir.path())
        .env("AH_HOME", std::env::var("AH_HOME").unwrap_or_default())
        .env("HOME", std::env::var("HOME").unwrap_or_default())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "echo")
        .env("SSH_ASKPASS", "echo")
        .env("AH_TASK_FORCE_MOCK_MANAGER", "1")
        .env("AH_BIN", &ctx.ah_bin);

    for (k, v) in &cmd.env {
        command.env(k, v);
    }

    for arg in &cmd.args {
        command.arg(arg);
    }

    let output = command.output().context("failed to execute userCommand")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if let Some(expect) = &cmd.expect {
        if let Some(code) = expect.exit_code {
            let actual = output.status.code().unwrap_or(-1);
            assert_eq!(
                actual, code,
                "exit code mismatch. stdout: {stdout}, stderr: {stderr}"
            );
        }
        if let Some(snippet) = &expect.stdout_contains {
            assert!(
                stdout.contains(snippet),
                "stdout missing expected snippet '{snippet}'. stdout: {stdout}"
            );
        }
        if let Some(snippet) = &expect.stderr_contains {
            assert!(
                stderr.contains(snippet),
                "stderr missing expected snippet '{snippet}'. stderr: {stderr}"
            );
        }
        if let Some(fs_expect) = &expect.fs {
            check_fs(fs_expect, ctx.repo_dir.path())?;
        }
    }

    Ok(())
}

fn check_fs(expect: &FsExpect, repo_root: &Path) -> Result<()> {
    for rel in &expect.exists {
        let path = resolve_rel_path(rel, repo_root).unwrap_or_else(|| {
            panic!(
                "expected path to exist: {}. Available task paths: {:?}",
                rel,
                debug_task_paths(repo_root)
            );
        });
        assert!(path.exists(), "expected path to exist: {}", path.display());
    }
    for rel in &expect.not_exists {
        let path = resolve_rel_path(rel, repo_root);
        assert!(path.is_none(), "expected path to be absent: {}", rel);
    }
    for FileContains { path, substring } in &expect.contains {
        let full = resolve_rel_path(path, repo_root)
            .with_context(|| format!("failed to resolve {}", path))?;
        let content = fs::read_to_string(&full)
            .with_context(|| format!("failed to read {}", full.display()))?;
        assert!(
            content.contains(substring),
            "expected '{}' to contain '{}'",
            full.display(),
            substring
        );
    }

    Ok(())
}

fn resolve_rel_path(rel: &str, repo_root: &Path) -> Option<PathBuf> {
    let direct = repo_root.join(rel);
    if direct.exists() {
        return Some(direct);
    }

    // Fallback: search within .agents/tasks for a path containing the requested segment
    let needle = rel.replace('\\', "/");
    let branch_hint = Path::new(rel).file_name().and_then(|n| n.to_str()).unwrap_or(&needle);
    let tasks_root = repo_root.join(".agents").join("tasks");
    if tasks_root.exists() {
        let mut stack = vec![tasks_root];
        while let Some(dir) = stack.pop() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path.clone());
                    }
                    let path_str = path.to_string_lossy();
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                    if path_str.contains(&needle) || file_name.contains(branch_hint) {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

fn debug_task_paths(repo_root: &Path) -> Vec<PathBuf> {
    let tasks_root = repo_root.join(".agents").join("tasks");
    let mut found = Vec::new();
    if tasks_root.exists() {
        let mut stack = vec![tasks_root];
        while let Some(dir) = stack.pop() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path.clone());
                    } else {
                        found.push(path);
                    }
                }
            }
        }
    }
    found
}

fn assert_git_state(assert: &AssertGit, ctx: &ScenarioContext) -> Result<()> {
    let branch = &assert.branch;
    if let Some(exists) = assert.exists {
        let status = Command::new("git")
            .args(["show-ref", "--verify", &format!("refs/heads/{branch}")])
            .current_dir(ctx.repo_dir.path())
            .output()
            .context("failed to check branch existence")?
            .status
            .success();
        if exists {
            assert!(status, "expected branch {branch} to exist");
        } else {
            assert!(!status, "expected branch {branch} to be absent");
        }
    }

    if let Some(count) = assert.commit_count {
        let base = assert.commits_ahead_of.clone().unwrap_or_else(|| "main".to_string());
        let out = Command::new("git")
            .args(["rev-list", "--count", &format!("{base}..{branch}")])
            .current_dir(ctx.repo_dir.path())
            .output()
            .context("failed to compute commit distance")?;
        let actual = String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse::<u32>()
            .context("failed to parse rev-list output")?;
        assert_eq!(
            actual, count,
            "unexpected commit count ahead of {base} for {branch}"
        );
    }

    if let Some(substr) = &assert.head_message_contains {
        let out = Command::new("git")
            .args(["show", "-s", "--format=%B", branch])
            .current_dir(ctx.repo_dir.path())
            .output()
            .context("failed to read head commit message")?;
        let msg = String::from_utf8_lossy(&out.stdout);
        assert!(
            msg.contains(substr),
            "expected HEAD message for {branch} to contain '{substr}', got: {msg}"
        );
    }

    if let Some(expect_remote) = assert.remote_exists {
        let remote_status = Command::new("git")
            .args(["ls-remote", "--exit-code", "origin", branch])
            .current_dir(ctx.repo_dir.path())
            .output()
            .context("failed to check remote branch")?
            .status
            .success();
        if expect_remote {
            assert!(
                remote_status,
                "expected branch {branch} to be pushed to remote"
            );
        } else {
            assert!(
                !remote_status,
                "expected branch {branch} to be absent on remote"
            );
        }
    }

    Ok(())
}
