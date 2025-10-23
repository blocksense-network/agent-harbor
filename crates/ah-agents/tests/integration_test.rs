// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// Integration tests for ah-agents crate
///
/// These tests verify agent launching, credential copying, and session management
/// using the mock LLM API server from tests/tools/mock-agent/
use ah_agents::{AgentExecutor, AgentLaunchConfig};
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(feature = "claude")]
#[tokio::test]
async fn test_claude_version_detection() {
    let agent = ah_agents::claude();

    // This will fail if claude is not in PATH, which is expected in CI
    // In actual integration tests, we'd mock this or ensure claude is installed
    let result = agent.detect_version().await;

    if result.is_ok() {
        let version = result.unwrap();
        println!("Detected Claude version: {}", version.version);
        assert!(!version.version.is_empty());
    } else {
        println!("Claude not found in PATH (expected in some environments)");
    }
}

#[cfg(feature = "codex")]
#[tokio::test]
async fn test_codex_version_detection() {
    let agent = ah_agents::codex();

    let result = agent.detect_version().await;

    if result.is_ok() {
        let version = result.unwrap();
        println!("Detected Codex version: {}", version.version);
        assert!(!version.version.is_empty());
    } else {
        println!("Codex not found in PATH (expected in some environments)");
    }
}

#[cfg(feature = "claude")]
#[tokio::test]
async fn test_claude_config_dir() {
    let agent = ah_agents::claude();
    let home = PathBuf::from("/tmp/test-home");
    let config_dir = agent.config_dir(&home);

    assert_eq!(config_dir, PathBuf::from("/tmp/test-home/.claude"));
}

#[cfg(feature = "codex")]
#[tokio::test]
async fn test_codex_config_dir() {
    let agent = ah_agents::codex();
    let home = PathBuf::from("/tmp/test-home");
    let config_dir = agent.config_dir(&home);

    assert_eq!(config_dir, PathBuf::from("/tmp/test-home/.config/codex"));
}

#[tokio::test]
async fn test_session_export_import() {
    use ah_agents::session::{export_directory, import_directory};
    use std::fs;

    let temp = TempDir::new().unwrap();

    // Create source directory with test content
    let source_dir = temp.path().join("source");
    fs::create_dir_all(&source_dir).unwrap();
    fs::write(source_dir.join("config.json"), r#"{"key": "value"}"#).unwrap();

    // Export to archive
    let archive_path = temp.path().join("session.tar.gz");
    export_directory(&source_dir, &archive_path).await.unwrap();
    assert!(archive_path.exists());

    // Import to new directory
    let dest_dir = temp.path().join("dest");
    let result = import_directory(&archive_path, &dest_dir).await;
    assert!(result.is_ok());
    assert!(dest_dir.join("config.json").exists());

    let content = fs::read_to_string(dest_dir.join("config.json")).unwrap();
    assert_eq!(content, r#"{"key": "value"}"#);
}

#[tokio::test]
async fn test_credential_copying() {
    use ah_agents::credentials::copy_files;
    use std::fs;

    let temp = TempDir::new().unwrap();
    let src_home = temp.path().join("src");
    let dst_home = temp.path().join("dst");

    // Create source credentials
    fs::create_dir_all(src_home.join(".claude")).unwrap();
    fs::write(src_home.join(".claude/config.json"), "{}").unwrap();

    let files = vec![PathBuf::from(".claude/config.json")];
    let result = copy_files(&files, &src_home, &dst_home).await;

    assert!(result.is_ok());
    assert!(dst_home.join(".claude/config.json").exists());
}

#[tokio::test]
async fn test_launch_config_builder() {
    let config = AgentLaunchConfig::new("test prompt", "/tmp/home")
        .interactive(true)
        .json_output(true)
        .api_server("http://localhost:18080")
        .mcp_server("server1")
        .env("KEY1", "VALUE1")
        .working_dir("/tmp/work");

    assert_eq!(config.prompt, "test prompt");
    assert_eq!(config.home_dir, PathBuf::from("/tmp/home"));
    assert!(config.interactive);
    assert!(config.json_output);
    assert_eq!(
        config.api_server,
        Some("http://localhost:18080".to_string())
    );
    assert_eq!(config.mcp_servers, vec!["server1"]);
    assert_eq!(
        config.env_vars,
        vec![("KEY1".to_string(), "VALUE1".to_string())]
    );
    assert_eq!(config.working_dir, PathBuf::from("/tmp/work"));
}

#[test]
fn test_available_agents() {
    let agents = ah_agents::available_agents();
    assert!(!agents.is_empty());
    println!("Available agents: {:?}", agents);
}

#[cfg(feature = "claude")]
#[test]
fn test_agent_by_name_claude() {
    let agent = ah_agents::agent_by_name("claude");
    assert!(agent.is_some());
    assert_eq!(agent.unwrap().name(), "claude");
}

#[cfg(feature = "codex")]
#[test]
fn test_agent_by_name_codex() {
    let agent = ah_agents::agent_by_name("codex");
    assert!(agent.is_some());
    assert_eq!(agent.unwrap().name(), "codex");
}

#[test]
fn test_agent_by_name_unknown() {
    let agent = ah_agents::agent_by_name("nonexistent");
    assert!(agent.is_none());
}
