// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use common::acp::spawn_acp_server_with_scenario;
use common::acp_scenario::run_acp_scenario;

mod common;

#[tokio::test]
async fn scenario_terminal_follow_detach_replays_updates() {
    // Uses Scenario Format timeline in tests/acp_bridge/scenarios/terminal_follow_detach.yaml
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/acp_bridge/scenarios/terminal_follow_detach.yaml");

    let (acp_url, handle) = spawn_acp_server_with_scenario(fixture.clone()).await;
    let acp_url = format!("{}?api_key=secret", acp_url);

    run_acp_scenario(&acp_url, fixture).await.unwrap();

    handle.abort();
}
