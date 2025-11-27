// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only
#![allow(dead_code)]

use std::net::TcpListener;
use std::path::PathBuf;

use ah_rest_server::{
    Server, ServerConfig,
    config::AcpConfig,
    mock_dependencies::{MockServerDependencies, ScenarioPlaybackOptions},
};
use tokio::task::JoinHandle;

/// Default test configuration shared by ACP integration tests.
fn base_config() -> (ServerConfig, std::net::SocketAddr, std::net::SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let acp_listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let acp_addr = acp_listener.local_addr().unwrap();
    drop(acp_listener);

    let acp_cfg = AcpConfig {
        enabled: true,
        bind_addr: acp_addr,
        ..AcpConfig::default()
    };

    let config = ServerConfig {
        bind_addr: addr,
        enable_cors: true,
        api_key: Some("secret".into()),
        acp: acp_cfg,
        ..ServerConfig::default()
    };

    (config, addr, acp_addr)
}

/// Spawn an ACP-enabled server with optional config mutation and optional scenario playback.
pub async fn spawn_acp_server(
    configure: impl FnOnce(&mut ServerConfig),
    scenario: Option<ScenarioPlaybackOptions>,
) -> (String, JoinHandle<()>) {
    let (mut config, _rest_addr, acp_addr) = base_config();
    configure(&mut config);

    let deps = match scenario {
        Some(options) => MockServerDependencies::with_options(config.clone(), options)
            .await
            .expect("deps"),
        None => MockServerDependencies::new(config.clone()).await.expect("deps"),
    };

    let server = Server::with_state(config, deps.into_state()).await.expect("server");
    let handle = tokio::spawn(async move {
        server.run().await.expect("server run");
    });
    let acp_url = format!("ws://{}/acp/v1/connect", acp_addr);
    (acp_url, handle)
}

/// Convenience: spawn with defaults.
pub async fn spawn_acp_server_basic() -> (String, JoinHandle<()>) {
    spawn_acp_server(|_| {}, None).await
}

/// Convenience: spawn with connection limit.
pub async fn spawn_acp_server_with_limit(limit: usize) -> (String, JoinHandle<()>) {
    spawn_acp_server(|cfg| cfg.acp.connection_limit = limit, None).await
}

/// Convenience: spawn with a single scenario fixture.
pub async fn spawn_acp_server_with_scenario(fixture: PathBuf) -> (String, JoinHandle<()>) {
    // Allow scenario playback to stay connected briefly after timeline for assertions.
    spawn_acp_server(
        |cfg| {
            cfg.acp.connection_limit = 10; // sane default for scenario playback
        },
        Some(ScenarioPlaybackOptions {
            scenario_files: vec![fixture],
            speed_multiplier: 0.1,
            linger_after_timeline_secs: Some(1.0),
        }),
    )
    .await
}
