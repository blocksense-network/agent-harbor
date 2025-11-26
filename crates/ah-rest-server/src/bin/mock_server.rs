// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Dedicated binary that wires the REST server to the mock TaskManager backend

use ah_domain_types::LogLevel;
use ah_logging::{Level, LogFormat, init};
use ah_rest_server::{
    Server, ServerConfig,
    mock_dependencies::{MockServerDependencies, ScenarioPlaybackOptions},
};
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "Agent Harbor REST server (mock backend)")]
struct Args {
    /// Bind address for the server
    #[arg(short, long, default_value = "127.0.0.1:38180")]
    bind: SocketAddr,

    /// Enable CORS for development
    #[arg(long)]
    cors: bool,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: LogLevel,

    /// Scenario files or directories to load (repeat for multiple). If omitted,
    /// the server will try to load ./test_scenarios when present.
    #[arg(long = "scenario", value_name = "PATH", action = clap::ArgAction::Append)]
    scenarios: Vec<PathBuf>,

    /// Playback speed multiplier (e.g. 0.2 for 5× faster, 2.0 for slower)
    #[arg(long = "scenario-speed", default_value_t = 1.0)]
    scenario_speed: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize logging
    let default_level: Level = args.log_level.into();
    init("ah-rest-server-mock", default_level, LogFormat::Plaintext)?;

    tracing::info!("Starting Agent Harbor REST API mock server");

    let config = ServerConfig {
        bind_addr: args.bind,
        enable_cors: args.cors,
        ..Default::default()
    };

    let mut playback = ScenarioPlaybackOptions {
        speed_multiplier: args.scenario_speed.max(0.01),
        ..Default::default()
    };
    if !args.scenarios.is_empty() {
        playback.scenario_files = args.scenarios.clone();
    } else if let Ok(env_path) = std::env::var("AH_SCENARIO_DIR") {
        playback.scenario_files.push(PathBuf::from(env_path));
    } else if let Ok(cwd) = std::env::current_dir() {
        let default_dir = cwd.join("test_scenarios");
        if default_dir.exists() {
            playback.scenario_files.push(default_dir);
        }
    }

    let deps = MockServerDependencies::with_options(config.clone(), playback).await?;
    let server = Server::with_state(config, deps.into_state()).await?;
    server.run().await?;

    Ok(())
}
