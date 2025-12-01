// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Run the Agent Activity TUI against a mock-agent scenario.
//!
//! This binary stitches together the mock-agent scenario loader with the
//! milestone 0.5 Agent Activity TUI. It is intentionally lightweight: it loads
//! a scenario, converts the ACP transcript into AgentActivity rows, and hands
//! them to the new Agent Activity view model/render loop.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use agent_client_protocol_schema::{ContentBlock, SessionUpdate, TextContent, ToolCallStatus};
use ah_recorder::TerminalState;
use ah_scenario_format::{ScenarioLoader, ScenarioSource};
use ah_tui::{
    AgentSessionUiMode, ViewerConfig, agent_session_loop::run_session_viewer,
    session_viewer_deps::AgentSessionDependencies as SessionDeps,
};
use anyhow::Context;
use clap::Parser;
use mock_agent::executor::{ScenarioExecutor, TimedNotification};

#[derive(Debug, Parser)]
struct Args {
    /// Scenario file or directory
    #[arg(short, long)]
    scenario: Vec<PathBuf>,

    /// Optional session id to select a specific scenario
    #[arg(long)]
    session_id: Option<String>,

    /// Optional scenario name to select
    #[arg(long)]
    scenario_name: Option<String>,

    /// Select scenario by matching initial prompt (Levenshtein best match)
    #[arg(long)]
    match_prompt: Option<String>,

    /// Working directory to advertise/override
    #[arg(long)]
    cwd: Option<PathBuf>,

    /// MCP servers JSON array to advertise
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

    /// Playback speed multiplier (e.g. 2.0 for 2x faster)
    #[arg(long, default_value = "1.0")]
    speed: f64,

    /// Logging options (see Logging-Guidelines)
    #[command(flatten)]
    logging: ah_logging::CliLoggingArgs,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    args.logging.clone().init("mock-agent-session", true)?;

    if args.scenario.is_empty() {
        anyhow::bail!("--scenario is required");
    }

    // Validate optional MCP servers JSON early (even if unused downstream).
    let parsed_mcp_servers: Option<serde_json::Value> = if let Some(raw) = &args.mcp_servers {
        Some(serde_json::from_str(raw).context("invalid JSON for --mcp-servers")?)
    } else {
        None
    };
    // Silence unused warnings for capability override flags (kept for CLI parity).
    let _capability_flags = (
        args.load_session,
        args.image_support,
        args.audio_support,
        args.embedded_context,
        args.mcp_http,
        args.mcp_sse,
        args.protocol_version,
        args.cwd.as_ref(),
        parsed_mcp_servers.as_ref(),
    );

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
    let executor = ScenarioExecutor::new(scenario.clone());
    let (_session_id, hist, live) = executor.timed_notifications(args.session_id.as_deref());
    let activity = notifications_to_activity(&hist, &live, args.speed);

    // Build DI container
    let terminal_state = Rc::new(RefCell::new(TerminalState::new_with_scrollback(
        100, 120, 1_000_000,
    )));

    let deps = SessionDeps {
        recording_terminal_state: terminal_state,
        viewer_config: ViewerConfig {
            terminal_cols: 120,
            terminal_rows: 40,
            scrollback: 1_000_000,
            gutter: ah_tui::view_model::session_viewer_model::GutterConfig {
                position: ah_tui::view_model::session_viewer_model::GutterPosition::None,
                show_line_numbers: false,
            },
            is_replay_mode: true,
        },
        task_manager: ah_core::create_session_viewer_task_manager().expect("task manager"),
        autocomplete: None,
        settings: ah_tui::settings::Settings::default(),
        theme: ah_tui::theme::Theme::default(),
        terminal_config: ah_tui::terminal::TerminalConfig::minimal(),
        ui_mode: AgentSessionUiMode::AgentActivity,
        activity_entries: activity,
    };

    run_session_viewer(deps).await
}

fn select_scenario(
    loader: &ScenarioLoader,
    args: &Args,
) -> anyhow::Result<Arc<ah_scenario_format::Scenario>> {
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
        let matches: Vec<_> = loader
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
        if let Some(rec) = matches.first() {
            return Ok(Arc::new(rec.scenario.clone()));
        }
    }

    if let Some(prompt) = &args.match_prompt {
        if let Some((rec, dist)) = best_prompt_match(loader, prompt) {
            if dist > 64 {
                anyhow::bail!("no scenario prompt is similar enough to '{}'", prompt);
            }
            return Ok(Arc::new(rec.scenario.clone()));
        }
    }

    Ok(Arc::new(loader.scenarios()[0].scenario.clone()))
}

fn notifications_to_activity(
    historical: &[TimedNotification],
    live: &[TimedNotification],
    speed: f64,
) -> Vec<(u64, ah_tui::view_model::task_execution::AgentActivityRow)> {
    use ah_domain_types::task::ToolStatus;
    use ah_tui::view_model::task_execution::AgentActivityRow;
    let mut rows = Vec::new();

    for note in historical.iter().chain(live.iter()) {
        let push = |row, at_ms, rows: &mut Vec<_>| {
            let scaled = if speed <= 0.0 {
                at_ms
            } else {
                (at_ms as f64 / speed).round() as u64
            };
            rows.push((scaled, row));
        };
        match &note.notification.update {
            SessionUpdate::AgentThoughtChunk {
                content: ContentBlock::Text(TextContent { text, .. }),
            } => {
                push(
                    AgentActivityRow::AgentThought {
                        thought: text.clone(),
                    },
                    note.at_ms,
                    &mut rows,
                );
            }
            SessionUpdate::AgentMessageChunk { content }
            | SessionUpdate::UserMessageChunk { content } => {
                if let ContentBlock::Text(TextContent { text, .. }) = content {
                    push(
                        AgentActivityRow::AgentThought {
                            thought: text.clone(),
                        },
                        note.at_ms,
                        &mut rows,
                    );
                }
            }
            SessionUpdate::ToolCall(call) => {
                push(
                    AgentActivityRow::ToolUse {
                        tool_name: call.title.clone(),
                        tool_execution_id: call.id.0.to_string(),
                        last_line: None,
                        completed: false,
                        status: ToolStatus::Started,
                        pipeline: None,
                    },
                    note.at_ms,
                    &mut rows,
                );
            }
            SessionUpdate::ToolCallUpdate(update) => {
                let status = match update.fields.status.unwrap_or(ToolCallStatus::Pending) {
                    ToolCallStatus::Completed => ToolStatus::Completed,
                    ToolCallStatus::Failed => ToolStatus::Failed,
                    _ => ToolStatus::Started,
                };
                let last_line = update
                    .fields
                    .raw_output
                    .as_ref()
                    .and_then(|v| v.as_str().map(|s| s.to_string()));
                push(
                    AgentActivityRow::ToolUse {
                        tool_name: update.fields.title.clone().unwrap_or_else(|| "tool".into()),
                        tool_execution_id: update.id.0.to_string(),
                        last_line,
                        completed: matches!(status, ToolStatus::Completed),
                        status,
                        pipeline: None,
                    },
                    note.at_ms,
                    &mut rows,
                );
            }
            SessionUpdate::CurrentModeUpdate { current_mode_id } => {
                push(
                    AgentActivityRow::AgentThought {
                        thought: format!("Mode switched to {}", current_mode_id),
                    },
                    note.at_ms,
                    &mut rows,
                );
            }
            SessionUpdate::Plan(plan) => {
                let entry_count = plan.entries.len();
                push(
                    AgentActivityRow::AgentThought {
                        thought: format!("Plan updated ({entry_count} entries)"),
                    },
                    note.at_ms,
                    &mut rows,
                );
            }
            _ => {}
        }
    }

    rows
}

fn enforce_loadsession_capability(scenario: &ah_scenario_format::Scenario) -> anyhow::Result<()> {
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

fn best_prompt_match<'a>(
    loader: &'a ScenarioLoader,
    prompt: &str,
) -> Option<(&'a ah_scenario_format::ScenarioRecord, usize)> {
    let mut best: Option<(&ah_scenario_format::ScenarioRecord, usize)> = None;
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
