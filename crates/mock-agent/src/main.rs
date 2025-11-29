// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Minimal ACP-mode entrypoint for mock-agent

use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::AgentSideConnection;
use ah_scenario_format::{
    AcpCapabilities, AcpMcpCapabilities, AcpPromptCapabilities, Scenario, ScenarioLoader,
    ScenarioSource,
};
use anyhow::Context;
use clap::Parser;
use mock_agent::executor::{ScenarioAgent, ScenarioExecutor};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[derive(Debug, Parser)]
struct Args {
    /// Scenario file or directory
    #[arg(short, long)]
    scenario: Vec<PathBuf>,

    /// Select scenario by name
    #[arg(long)]
    scenario_name: Option<String>,

    /// Select scenario by sessionId (from sessionStart)
    #[arg(long)]
    session_id: Option<String>,

    /// Select scenario by matching initial prompt (Levenshtein best match)
    #[arg(long)]
    match_prompt: Option<String>,

    /// Working directory to advertise/override (passed through new_session)
    #[arg(long)]
    cwd: Option<PathBuf>,

    /// MCP servers JSON array to advertise (overrides scenario acp.mcpServers)
    #[arg(long)]
    mcp_servers: Option<String>,

    /// Protocol version
    #[arg(long, default_value = "1")]
    protocol_version: u32,

    /// Override capability: loadSession
    #[arg(long)]
    load_session: Option<bool>,
    /// Override capability: prompt image support
    #[arg(long)]
    image_support: Option<bool>,
    /// Override capability: prompt audio support
    #[arg(long)]
    audio_support: Option<bool>,
    /// Override capability: prompt embedded context support
    #[arg(long)]
    embedded_context: Option<bool>,
    /// Override capability: MCP HTTP transport support
    #[arg(long)]
    mcp_http: Option<bool>,
    /// Override capability: MCP SSE transport support
    #[arg(long)]
    mcp_sse: Option<bool>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    if args.scenario.is_empty() {
        anyhow::bail!("--scenario is required");
    }

    // Best-effort parse MCP servers upfront to fail fast on bad JSON.
    let parsed_mcp_servers: Option<serde_json::Value> = if let Some(raw) = &args.mcp_servers {
        Some(
            serde_json::from_str(raw).context(
                "--mcp-servers must be valid JSON array (e.g. [{\"name\":\"fs\",\"command\":\"mcp-server-filesystem\"}])",
            )?,
        )
    } else {
        None
    };

    let sources: Vec<_> = args
        .scenario
        .iter()
        .map(|p| {
            if p.is_dir() {
                ScenarioSource::Directory(p.clone())
            } else {
                ScenarioSource::File(p.clone())
            }
        })
        .collect();

    let loader = ScenarioLoader::from_sources(sources)?;
    let scenario = select_scenario(&loader, &args)?;
    enforce_loadsession_capability(&scenario)?;
    let cap_override = build_capability_override(&args);
    let executor = ScenarioExecutor::new(scenario.clone());
    let transcript = executor.to_acp_transcript(
        args.session_id.as_deref(),
        cap_override,
        args.cwd.clone(),
        parsed_mcp_servers,
    );
    let agent = ScenarioAgent::new(transcript);

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let stdout = tokio::io::stdout();
            let stdin = tokio::io::stdin();
            let (agent_conn, agent_io) = AgentSideConnection::new(
                agent.clone(),
                stdout.compat_write(),
                stdin.compat(),
                |fut| {
                    tokio::task::spawn_local(fut);
                },
            );
            let agent_conn = Arc::new(agent_conn);
            agent.attach_client_connection(agent_conn.clone()).await;
            agent_io.await?;
            Ok::<(), anyhow::Error>(())
        })
        .await?;

    Ok(())
}

fn select_scenario(loader: &ScenarioLoader, args: &Args) -> anyhow::Result<Arc<Scenario>> {
    if loader.scenarios().is_empty() {
        anyhow::bail!("no scenarios found");
    }

    if let Some(name) = &args.scenario_name {
        if let Some(rec) = loader.scenarios().iter().find(|r| r.scenario.name == *name) {
            return Ok(Arc::new(rec.scenario.clone()));
        }
        anyhow::bail!("no scenario matching name {}", name);
    }

    if let Some(sess) = &args.session_id {
        let mut matches: Vec<&ah_scenario_format::ScenarioRecord> = loader
            .scenarios()
            .iter()
            .filter(|r| {
                r.scenario
                    .partition_by_session_start()
                    .session_start
                    .as_ref()
                    .and_then(|s| s.session_id.as_ref())
                    .map(|id| id == sess)
                    .unwrap_or(false)
            })
            .collect();
        if matches.is_empty() {
            anyhow::bail!("no scenario with sessionStart.sessionId matching {}", sess);
        }
        matches.sort_by_key(|r| r.scenario.name.clone());
        return Ok(Arc::new(matches[0].scenario.clone()));
    }

    if let Some(prompt) = &args.match_prompt {
        if let Some((rec, dist)) = best_prompt_match(loader, prompt) {
            // Simple threshold: require some similarity to avoid wild picks.
            if dist > 64 {
                anyhow::bail!("no scenario prompt is similar enough to '{}'", prompt);
            }
            return Ok(Arc::new(rec.scenario.clone()));
        }
    }

    Ok(Arc::new(loader.scenarios()[0].scenario.clone()))
}

fn enforce_loadsession_capability(scenario: &Scenario) -> anyhow::Result<()> {
    let has_session_start = scenario.partition_by_session_start().session_start.is_some();
    if has_session_start {
        let load_cap = scenario
            .acp
            .as_ref()
            .and_then(|a| a.capabilities.as_ref())
            .and_then(|c| c.load_session)
            .unwrap_or(false);
        if !load_cap {
            anyhow::bail!(
                "scenario '{}' includes sessionStart but acp.capabilities.loadSession is false/absent",
                scenario.name
            );
        }
    }
    Ok(())
}

fn build_capability_override(args: &Args) -> Option<AcpCapabilities> {
    let mut caps = AcpCapabilities {
        load_session: args.load_session,
        prompt_capabilities: None,
        mcp_capabilities: None,
    };

    if args.image_support.is_some()
        || args.audio_support.is_some()
        || args.embedded_context.is_some()
    {
        caps.prompt_capabilities = Some(AcpPromptCapabilities {
            image: args.image_support,
            audio: args.audio_support,
            embedded_context: args.embedded_context,
        });
    }

    if args.mcp_http.is_some() || args.mcp_sse.is_some() {
        caps.mcp_capabilities = Some(AcpMcpCapabilities {
            http: args.mcp_http,
            sse: args.mcp_sse,
        });
    }

    if caps.load_session.is_some()
        || caps.prompt_capabilities.is_some()
        || caps.mcp_capabilities.is_some()
    {
        Some(caps)
    } else {
        None
    }
}

fn best_prompt_match<'a>(
    loader: &'a ScenarioLoader,
    prompt: &str,
) -> Option<(&'a ah_scenario_format::ScenarioRecord, usize)> {
    let mut best: Option<(&ah_scenario_format::ScenarioRecord, usize)> = None;
    // Prefer scenarios tagged "acp" when distances tie.
    for rec in loader.scenarios() {
        if let Some(sprompt) = rec.scenario.effective_initial_prompt() {
            let dist = levenshtein(prompt, &sprompt);
            match &mut best {
                Some((best_rec, cur)) => {
                    if dist < *cur
                        || (dist == *cur
                            && rec.scenario.tags.contains(&"acp".to_string())
                            && !best_rec.scenario.tags.contains(&"acp".to_string()))
                    {
                        *cur = dist;
                        *best.as_mut().unwrap() = (rec, dist);
                    }
                }
                None => best = Some((rec, dist)),
            }
        }
    }
    best
}

fn levenshtein(a: &str, b: &str) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut curr: Vec<usize> = Vec::with_capacity(b.len() + 1);
        curr.push(i + 1);
        for (j, cb) in b.chars().enumerate() {
            let substitution = if ca == cb { prev[j] } else { prev[j] + 1 };
            let insertion = curr[j] + 1;
            let deletion = prev[j + 1] + 1;
            curr.push(substitution.min(insertion).min(deletion));
        }
        prev = curr;
    }
    prev[b.len()]
}
