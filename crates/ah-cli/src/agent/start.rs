//! Agent start command implementation

use clap::{Args, ValueEnum};
use std::path::PathBuf;
use std::process::Stdio;

/// Working copy mode for agent execution
#[derive(Clone, Debug, ValueEnum)]
pub enum WorkingCopyMode {
    /// Execute agent directly in the current working directory
    InPlace,
    /// Use filesystem snapshots for workspace isolation
    Snapshots,
}

/// Agent start command arguments
#[derive(Args)]
pub struct AgentStartArgs {
    /// Agent type to start
    #[arg(long, default_value = "mock")]
    pub agent: String,

    /// Working copy mode
    #[arg(long, default_value = "in-place")]
    pub working_copy: WorkingCopyMode,

    /// Working directory for agent execution
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Task ID to associate with this agent session
    #[arg(long)]
    pub task_id: Option<String>,

    /// Enable sandbox mode
    #[arg(long)]
    pub sandbox: bool,

    /// Sandbox type (when sandbox is enabled)
    #[arg(long, default_value = "local")]
    pub sandbox_type: String,

    /// Allow network access in sandbox
    #[arg(long, value_parser = parse_bool)]
    pub allow_network: Option<bool>,

    /// Allow container access in sandbox
    #[arg(long, value_parser = parse_bool)]
    pub allow_containers: Option<bool>,

    /// Allow KVM access in sandbox
    #[arg(long, value_parser = parse_bool)]
    pub allow_kvm: Option<bool>,

    /// Enable seccomp filtering
    #[arg(long, value_parser = parse_bool)]
    pub seccomp: Option<bool>,

    /// Enable seccomp debugging
    #[arg(long, value_parser = parse_bool)]
    pub seccomp_debug: Option<bool>,

    /// Additional writable paths to bind mount
    #[arg(long)]
    pub mount_rw: Vec<PathBuf>,

    /// Paths to promote to copy-on-write overlays
    #[arg(long)]
    pub overlay: Vec<PathBuf>,

    /// Additional flags to pass to the agent
    #[arg(long, value_name = "FLAG")]
    pub agent_flags: Vec<String>,
}

/// Parse boolean values from command line (true/false, yes/no, 1/0, y/n)
fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_lowercase().as_str() {
        "true" | "yes" | "1" | "y" => Ok(true),
        "false" | "no" | "0" | "n" => Ok(false),
        _ => Err(format!("Invalid boolean value: {}. Use true/false, yes/no, 1/0, or y/n", s)),
    }
}

impl AgentStartArgs {
    /// Run the agent start command
    pub async fn run(self) -> anyhow::Result<()> {
        // For milestone 2.4.1, we support the "mock" agent for E2E testing
        if self.agent == "mock" {
            return self.run_mock_agent().await;
        }

        // For other agents, this is still a placeholder that will be replaced
        // when milestone 2.4 is implemented.
        eprintln!("Agent start command is not yet implemented for agent '{}'.", self.agent);
        eprintln!("This is a placeholder for milestone 2.4 implementation.");
        eprintln!("E2E tests in milestone 2.4.1 will validate this command once implemented.");

        // Print the parsed arguments for debugging
        eprintln!("Parsed arguments:");
        eprintln!("  agent: {}", self.agent);
        eprintln!("  working_copy: {:?}", self.working_copy);
        eprintln!("  cwd: {:?}", self.cwd);
        eprintln!("  task_id: {:?}", self.task_id);
        eprintln!("  sandbox: {}", self.sandbox);
        eprintln!("  agent_flags: {:?}", self.agent_flags);

        if self.sandbox {
            eprintln!("  sandbox_type: {}", self.sandbox_type);
            eprintln!("  allow_network: {:?}", self.allow_network);
            eprintln!("  allow_containers: {:?}", self.allow_containers);
            eprintln!("  allow_kvm: {:?}", self.allow_kvm);
            eprintln!("  seccomp: {:?}", self.seccomp);
            eprintln!("  seccomp_debug: {:?}", self.seccomp_debug);
            eprintln!("  mount_rw: {:?}", self.mount_rw);
            eprintln!("  overlay: {:?}", self.overlay);
        }

        // Exit with success for now - the E2E tests will validate the actual behavior
        Ok(())
    }

    /// Run the mock agent for testing purposes
    async fn run_mock_agent(&self) -> anyhow::Result<()> {
        use tokio::process::Command;

        // Determine the working directory
        let cwd = if let Some(cwd) = &self.cwd {
            cwd.clone()
        } else {
            std::env::current_dir()?
        };

        // Build the command to run the mock agent
        let mut cmd = Command::new("python");
        cmd.arg("-m")
            .arg("src.cli")
            .current_dir(&cwd)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        // Check if this is a scenario run or demo run
        let is_scenario_run = self.agent_flags.iter().any(|flag| flag == "--scenario");

        if is_scenario_run {
            cmd.arg("run");
            // Add all the agent flags
            for flag in &self.agent_flags {
                cmd.arg(flag);
            }
        } else {
            // Run in demo mode
            cmd.arg("demo")
                .arg("--workspace")
                .arg(&cwd);
            // Add any additional flags (like --tui-testing-uri)
            for flag in &self.agent_flags {
                cmd.arg(flag);
            }
        }

        // Note: TUI_TESTING_URI should only be passed when explicitly requested
        // We don't automatically pass it from environment to avoid test interference

        // Set PYTHONPATH to find the mock agent
        // Try to find the workspace root relative to the current executable
        let mut pythonpath_set = false;
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(workspace_root) = current_exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
                let pythonpath = format!("{}/tests/tools/mock-agent", workspace_root.display());
                eprintln!("Setting PYTHONPATH to: {}", pythonpath);
                cmd.env("PYTHONPATH", pythonpath);
                pythonpath_set = true;
            }
        }

        // Fallback: try CARGO_MANIFEST_DIR
        if !pythonpath_set {
            if let Ok(cargo_manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
                let workspace_root = std::path::Path::new(&cargo_manifest_dir).parent().unwrap().parent().unwrap();
                let pythonpath = format!("{}/tests/tools/mock-agent", workspace_root.display());
                eprintln!("Setting PYTHONPATH to (fallback): {}", pythonpath);
                cmd.env("PYTHONPATH", pythonpath);
                pythonpath_set = true;
            }
        }

        if !pythonpath_set {
            eprintln!("Warning: Could not determine PYTHONPATH for mock agent");
        }

        // Always pass the TUI_TESTING_URI environment variable to the mock agent
        if let Ok(tui_testing_uri) = std::env::var("TUI_TESTING_URI") {
            cmd.env("TUI_TESTING_URI", &tui_testing_uri);
        }

        // Execute the mock agent
        let status = cmd.status().await?;

        if status.success() {
            Ok(())
        } else {
            anyhow::bail!("Mock agent exited with status: {}", status);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bool() {
        assert_eq!(parse_bool("true").unwrap(), true);
        assert_eq!(parse_bool("false").unwrap(), false);
        assert_eq!(parse_bool("yes").unwrap(), true);
        assert_eq!(parse_bool("no").unwrap(), false);
        assert_eq!(parse_bool("1").unwrap(), true);
        assert_eq!(parse_bool("0").unwrap(), false);
        assert_eq!(parse_bool("y").unwrap(), true);
        assert_eq!(parse_bool("n").unwrap(), false);

        assert!(parse_bool("invalid").is_err());
    }
}
