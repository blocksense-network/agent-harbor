// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! CLI entrypoint for running the ACP client + Agent Activity TUI.

use ah_domain_types::AcpLaunchCommand;
use ah_tui::acp_client::{AcpClientConfig, run_acp_client};
use anyhow::Context;
use clap::Parser;

#[derive(Debug, Parser)]
struct Args {
    /// Full command used to launch the ACP-compliant agent
    #[arg(long = "acp-agent-cmd")]
    acp_agent_cmd: String,
    /// Optional initial prompt
    #[arg(long)]
    prompt: Option<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let command = AcpLaunchCommand::from_command_string(&args.acp_agent_cmd)
        .map_err(anyhow::Error::msg)
        .context("invalid --acp-agent-cmd")?;
    run_acp_client(AcpClientConfig {
        acp_command: command,
        prompt: args.prompt,
    })
    .await
    .context("acp client failed")
}
