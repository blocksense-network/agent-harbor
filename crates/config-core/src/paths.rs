//! Configuration file path discovery

use std::path::{Path, PathBuf};

/// Configuration file paths for different scopes
#[derive(Debug, Clone)]
pub struct Paths {
    pub system: PathBuf,
    pub user: PathBuf,
    pub repo: Option<PathBuf>,
    pub repo_user: Option<PathBuf>,
    pub cli_config: Option<PathBuf>,
}

/// Discover configuration file paths for the current environment
pub fn discover_paths(repo_root: Option<&Path>) -> Paths {
    let system = get_system_config_path();
    let user = get_user_config_path();

    let (repo, repo_user) = if let Some(repo_root) = repo_root {
        let repo = repo_root.join(".agents").join("config.toml");
        let repo_user = repo_root.join(".agents").join("config.user.toml");
        (Some(repo), Some(repo_user))
    } else {
        (None, None)
    };

    Paths {
        system,
        user,
        repo,
        repo_user,
        cli_config: None,
    }
}

/// Get system configuration path based on platform
fn get_system_config_path() -> PathBuf {
    if cfg!(target_os = "linux") {
        PathBuf::from("/etc/agent-harbor/config.toml")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/Library/Application Support/agent-harbor/config.toml")
    } else if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into()))
            .join("agent-harbor")
            .join("config.toml")
    } else {
        // Fallback for other platforms
        PathBuf::from("/etc/agent-harbor/config.toml")
    }
}

/// Get user configuration path based on platform and AH_HOME
fn get_user_config_path() -> PathBuf {
    // Check AH_HOME override first
    if let Ok(ah_home) = std::env::var("AH_HOME") {
        return PathBuf::from(ah_home).join("config.toml");
    }

    if cfg!(target_os = "linux") {
        std::env::var("XDG_CONFIG_HOME")
            .map(|p| PathBuf::from(p).join("agent-harbor").join("config.toml"))
            .unwrap_or_else(|_| {
                PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
                    .join(".config")
                    .join("agent-harbor")
                    .join("config.toml")
            })
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
            .join("Library")
            .join("Application Support")
            .join("agent-harbor")
            .join("config.toml")
    } else if cfg!(target_os = "windows") {
        // On Windows, ~/.config takes precedence over %APPDATA% when both exist
        let home_config = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "C:\\".into()))
            .join(".config")
            .join("agent-harbor")
            .join("config.toml");

        if home_config.exists() {
            home_config
        } else {
            PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| "C:\\".into()))
                .join("agent-harbor")
                .join("config.toml")
        }
    } else {
        // Fallback for other platforms
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
            .join(".config")
            .join("agent-harbor")
            .join("config.toml")
    }
}
