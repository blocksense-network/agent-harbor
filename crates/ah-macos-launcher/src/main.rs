// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_macos_launcher::LauncherConfig;
use anyhow::Result;

#[cfg(feature = "binary")]
use ah_logging::{Level, LogFormat, init};
#[cfg(feature = "binary")]
use clap::Parser;

#[cfg(feature = "binary")]
#[derive(Parser, Debug)]
#[command(name = "ah-macos-launcher", about = "macOS sandbox launcher")]
struct Args {
    /// Path to use as the new root (already bound to AgentFS mount)
    #[arg(long)]
    root: Option<String>,

    /// Working directory inside the new root
    #[arg(long)]
    workdir: Option<String>,

    /// Allow read under path (repeatable)
    #[arg(long = "allow-read", action = clap::ArgAction::Append)]
    allow_read: Vec<String>,

    /// Allow write under path (repeatable)
    #[arg(long = "allow-write", action = clap::ArgAction::Append)]
    allow_write: Vec<String>,

    /// Allow exec under path (repeatable)
    #[arg(long = "allow-exec", action = clap::ArgAction::Append)]
    allow_exec: Vec<String>,

    /// Allow network egress (default: off per strategy)
    #[arg(long, default_value_t = false)]
    allow_network: bool,

    /// Harden process-info and limit signals to same-group
    #[arg(long, default_value_t = false)]
    harden_process: bool,

    /// Command to exec (first is program)
    #[arg(last = true, required = true)]
    command: Vec<String>,
}

#[cfg(feature = "binary")]
fn main() -> Result<()> {
    // Initialize logging
    init("ah-macos-launcher", Level::INFO, LogFormat::Plaintext)?;
    let args = Args::parse();

    // Build configuration from command line args
    let mut config = LauncherConfig::new(args.command);
    if let Some(root) = args.root {
        config = config.root(root);
    }
    if let Some(workdir) = args.workdir {
        config = config.workdir(workdir);
    }
    config = config.allow_network(args.allow_network).harden_process(args.harden_process);

    config = args.allow_read.into_iter().fold(config, |c, p| c.allow_read(p));
    config = args.allow_write.into_iter().fold(config, |c, p| c.allow_write(p));
    config = args.allow_exec.into_iter().fold(config, |c, p| c.allow_exec(p));

    // Launch the process in sandbox
    ah_macos_launcher::launch_in_sandbox(config)
}
