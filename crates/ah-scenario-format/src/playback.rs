// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{
    Result,
    model::{
        AssertionData, AssistantStep, ErrorData, FileEditData, ProgressStep, ResponseElement,
        Scenario, ThinkingStep, TimelineEvent, ToolUseData,
    },
};
use serde::Deserialize;
use serde_yaml::{Mapping, Value};
use std::collections::HashMap;

/// Configuration for scenario playback.
#[derive(Debug, Clone, Copy)]
pub struct PlaybackOptions {
    /// Multiplier applied to all delays. Values > 1 slow down playback, < 1 speed it up.
    pub speed_multiplier: f64,
}

impl Default for PlaybackOptions {
    fn default() -> Self {
        Self {
            speed_multiplier: 1.0,
        }
    }
}

/// Iterator that yields scheduled playback events with absolute timestamps.
pub struct PlaybackIterator {
    events: std::vec::IntoIter<PlaybackEvent>,
}

impl PlaybackIterator {
    pub fn new(scenario: &Scenario, options: PlaybackOptions) -> Result<Self> {
        let mut builder = TimelineBuilder::new(options);
        builder.push_event(PlaybackEventKind::Status {
            value: "queued".into(),
        });
        builder.push_event(PlaybackEventKind::Status {
            value: "running".into(),
        });

        for event in &scenario.timeline {
            builder.process_event(event)?;
        }

        if !builder.completed {
            builder.push_event(PlaybackEventKind::Status {
                value: "completed".into(),
            });
        }

        Ok(Self {
            events: builder.events.into_iter(),
        })
    }
}

impl Iterator for PlaybackIterator {
    type Item = PlaybackEvent;

    fn next(&mut self) -> Option<Self::Item> {
        self.events.next()
    }
}

/// Position within the scenario timeline.
#[derive(Debug, Clone, Copy)]
pub struct TimelinePosition {
    pub timeline_ms: u64,
    pub scaled_ms: u64,
}

/// Event scheduled at a particular time.
#[derive(Debug, Clone)]
pub struct PlaybackEvent {
    pub at_ms: u64,
    pub position: TimelinePosition,
    pub kind: PlaybackEventKind,
}

/// High-level playback events that consumers can map to logs/SSE/etc.
#[derive(Debug, Clone)]
pub enum PlaybackEventKind {
    Status {
        value: String,
    },
    Thinking {
        text: String,
    },
    Assistant {
        text: String,
    },
    Log {
        message: String,
    },
    ToolStart {
        tool_name: String,
        args: serde_yaml::Value,
    },
    ToolProgress {
        tool_name: String,
        message: String,
    },
    ToolResult {
        tool_name: String,
        status: String,
        output: Option<serde_yaml::Value>,
    },
    FileEdit(FileEditData),
    UserInput {
        target: Option<String>,
        value: String,
    },
    UserCommand {
        cmd: String,
        cwd: Option<String>,
    },
    Screenshot {
        label: String,
    },
    Assert(AssertionData),
    Merge,
    Complete,
    Error(ErrorData),
}

struct TimelineBuilder {
    events: Vec<PlaybackEvent>,
    options: PlaybackOptions,
    timeline_ms: u64,
    completed: bool,
}

impl TimelineBuilder {
    fn new(options: PlaybackOptions) -> Self {
        Self {
            events: Vec::new(),
            options,
            timeline_ms: 0,
            completed: false,
        }
    }

    fn process_event(&mut self, event: &TimelineEvent) -> Result<()> {
        match event {
            TimelineEvent::LlmResponse { llm_response } => {
                for element in llm_response {
                    self.process_response_element(element)?;
                }
            }
            TimelineEvent::Legacy(map) => {
                self.process_legacy(map)?;
            }
            TimelineEvent::Assert { assert } => {
                self.push_event(PlaybackEventKind::Assert(assert.clone()));
            }
            TimelineEvent::Control { event_type, data } => {
                if event_type == "advanceMs" {
                    if let Some(ms) = data.get("ms").and_then(value_to_u64) {
                        self.advance(ms);
                    }
                }
            }
        }
        Ok(())
    }

    fn process_response_element(&mut self, element: &ResponseElement) -> Result<()> {
        match element {
            ResponseElement::Think { think } => {
                for ThinkingStep(delay, text) in think {
                    self.advance(*delay);
                    self.push_event(PlaybackEventKind::Thinking { text: text.clone() });
                }
            }
            ResponseElement::Assistant { assistant } => {
                for AssistantStep(delay, text) in assistant {
                    self.advance(*delay);
                    self.push_event(PlaybackEventKind::Assistant { text: text.clone() });
                }
            }
            ResponseElement::AgentToolUse { agent_tool_use } => {
                self.handle_tool_use(agent_tool_use)?;
            }
            ResponseElement::AgentEdits { agent_edits } => {
                self.push_event(PlaybackEventKind::FileEdit(agent_edits.clone()));
            }
            ResponseElement::ToolResult { tool_result } => {
                self.push_event(PlaybackEventKind::Log {
                    message: format!(
                        "Tool result {} (error={}): {:?}",
                        tool_result.tool_call_id, tool_result.is_error, tool_result.content
                    ),
                });
            }
            ResponseElement::Error { error } => {
                self.push_event(PlaybackEventKind::Error(error.clone()));
            }
        }
        Ok(())
    }

    fn process_legacy(&mut self, map: &HashMap<String, Value>) -> Result<()> {
        if let Some(value) = map.get("advanceMs").and_then(value_to_u64) {
            self.advance(value);
            return Ok(());
        }

        if let Some(value) = map.get("think") {
            let steps: Vec<ThinkingStep> = serde_yaml::from_value(value.clone())?;
            for ThinkingStep(delay, text) in steps {
                self.advance(delay);
                self.push_event(PlaybackEventKind::Thinking { text });
            }
            return Ok(());
        }

        if let Some(value) = map.get("assistant") {
            let steps: Vec<AssistantStep> = serde_yaml::from_value(value.clone())?;
            for AssistantStep(delay, text) in steps {
                self.advance(delay);
                self.push_event(PlaybackEventKind::Assistant { text });
            }
            return Ok(());
        }

        if let Some(value) = map.get("agentToolUse") {
            let data: ToolUseData = serde_yaml::from_value(value.clone())?;
            self.handle_tool_use(&data)?;
            return Ok(());
        }

        if let Some(value) = map.get("agentEdits") {
            let edit: FileEditData = serde_yaml::from_value(value.clone())?;
            self.push_event(PlaybackEventKind::FileEdit(edit));
            return Ok(());
        }

        if let Some(value) = map.get("userInputs") {
            let inputs: Vec<(u64, String)> = serde_yaml::from_value(value.clone())?;
            let target =
                map.get("target").and_then(|t| serde_yaml::from_value::<String>(t.clone()).ok());
            for (delay, text) in inputs {
                self.advance(delay);
                self.push_event(PlaybackEventKind::UserInput {
                    target: target.clone(),
                    value: text,
                });
            }
            return Ok(());
        }

        if let Some(value) = map.get("userCommand") {
            #[derive(Deserialize)]
            struct Helper {
                cmd: String,
                cwd: Option<String>,
            }
            let helper: Helper = serde_yaml::from_value(value.clone())?;
            self.push_event(PlaybackEventKind::UserCommand {
                cmd: helper.cmd,
                cwd: helper.cwd,
            });
            return Ok(());
        }

        if let Some(value) = map.get("screenshot") {
            if let Some(label) = value.as_str() {
                self.push_event(PlaybackEventKind::Screenshot {
                    label: label.to_string(),
                });
            }
            return Ok(());
        }

        if let Some(value) = map.get("assert") {
            let assertion: AssertionData = serde_yaml::from_value(value.clone())?;
            self.push_event(PlaybackEventKind::Assert(assertion));
            return Ok(());
        }

        if map.get("merge").is_some() {
            self.push_event(PlaybackEventKind::Merge);
            return Ok(());
        }

        if map.get("complete").is_some() {
            self.completed = true;
            self.push_event(PlaybackEventKind::Complete);
            self.push_event(PlaybackEventKind::Status {
                value: "completed".into(),
            });
            return Ok(());
        }

        Ok(())
    }

    fn handle_tool_use(&mut self, data: &ToolUseData) -> Result<()> {
        self.push_event(PlaybackEventKind::ToolStart {
            tool_name: data.tool_name.clone(),
            args: serde_yaml::to_value(&data.args).unwrap_or(Value::Null),
        });

        if let Some(progress) = &data.progress {
            for ProgressStep(delay, message) in progress {
                self.advance(*delay);
                self.push_event(PlaybackEventKind::ToolProgress {
                    tool_name: data.tool_name.clone(),
                    message: message.clone(),
                });
            }
        }

        if let Some(exec) = &data.tool_execution {
            let mut last_time = 0;
            for event in &exec.events {
                let delta = event.time_ms.unwrap_or(0).saturating_sub(last_time);
                self.advance(delta);
                last_time = event.time_ms.unwrap_or(last_time);
                match event.kind.as_str() {
                    "stdout" | "stderr" | "progress" => {
                        if let Some(content) = &event.content {
                            self.push_event(PlaybackEventKind::ToolProgress {
                                tool_name: data.tool_name.clone(),
                                message: content.clone(),
                            });
                        }
                    }
                    "completion" => {
                        self.push_event(PlaybackEventKind::ToolResult {
                            tool_name: data.tool_name.clone(),
                            status: if event.exit_code.unwrap_or(0) == 0 {
                                "success".into()
                            } else {
                                "error".into()
                            },
                            output: data.result.clone(),
                        });
                    }
                    _ => {}
                }
            }
        } else {
            self.push_event(PlaybackEventKind::ToolResult {
                tool_name: data.tool_name.clone(),
                status: data.status.clone().unwrap_or_else(|| "ok".into()),
                output: data.result.clone(),
            });
        }

        Ok(())
    }

    fn push_event(&mut self, kind: PlaybackEventKind) {
        let scaled = (self.timeline_ms as f64 * self.options.speed_multiplier).round() as u64;
        self.events.push(PlaybackEvent {
            at_ms: scaled,
            position: TimelinePosition {
                timeline_ms: self.timeline_ms,
                scaled_ms: scaled,
            },
            kind,
        });
    }

    fn advance(&mut self, delta: u64) {
        self.timeline_ms = self.timeline_ms.saturating_add(delta);
    }
}

fn value_to_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(num) => num.as_u64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

// helper to convert map into serde_yaml::Value if needed in future
#[allow(dead_code)]
fn map_to_value(map: &HashMap<String, Value>) -> Value {
    let mut mapping = Mapping::new();
    for (k, v) in map {
        mapping.insert(Value::String(k.clone()), v.clone());
    }
    Value::Mapping(mapping)
}
