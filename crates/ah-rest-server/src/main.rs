// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent Harbor REST API server binary

use ah_rest_server::{Server, ServerConfig};
use clap::Parser;
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Bind address for the server
    #[arg(short, long, default_value = "127.0.0.1:3001")]
    bind: SocketAddr,

    /// Database path (SQLite)
    #[arg(short, long, default_value = ":memory:")]
    database: String,

    /// Enable CORS for development
    #[arg(long)]
    cors: bool,

    /// Additional configuration file to load
    #[arg(long)]
    config: Option<String>,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_env("AH_LOG")
                .unwrap_or_else(|_| format!("ah_rest_server={}", args.log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Agent Harbor REST API server");

    // Create server configuration
    let config = ServerConfig {
        bind_addr: args.bind,
        database_path: args.database,
        enable_cors: args.cors,
        config_file: args.config,
        ..Default::default()
    };

    // Create and start server
    let server = Server::new(config).await?;
    server.run().await?;

    Ok(())
}
