// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{
    Result, ScenarioError,
    model::{
        AssertionData, ErrorData, FileEditData, ResponseElement, Scenario, TimelineEvent,
        ToolUseData, UserInputEntry, extract_prompt_from_input_value,
        extract_text_from_content_block,
    },
};
use serde_yaml::Value;

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
            TimelineEvent::UserInputs { user_inputs } => {
                self.process_user_inputs(user_inputs)?;
            }
            TimelineEvent::AgentToolUse { agent_tool_use } => {
                self.handle_tool_use(agent_tool_use)?;
            }
            TimelineEvent::AgentEdits { agent_edits } => {
                self.push_event(PlaybackEventKind::FileEdit(agent_edits.clone()));
            }
            TimelineEvent::AdvanceMs { base_time_delta } => {
                self.advance(*base_time_delta);
            }
            TimelineEvent::Screenshot { screenshot } => {
                self.push_event(PlaybackEventKind::Screenshot {
                    label: screenshot.clone(),
                });
            }
            TimelineEvent::Complete { .. } => {
                self.completed = true;
                self.push_event(PlaybackEventKind::Complete);
                self.push_event(PlaybackEventKind::Status {
                    value: "completed".into(),
                });
            }
            TimelineEvent::Merge { .. } => {
                self.push_event(PlaybackEventKind::Merge);
            }
            TimelineEvent::Assert { assert } => {
                self.push_event(PlaybackEventKind::Assert(assert.clone()));
            }
            TimelineEvent::Status { status } => {
                self.push_event(PlaybackEventKind::Status {
                    value: status.clone(),
                });
            }
        }
        Ok(())
    }

    fn process_response_element(&mut self, element: &ResponseElement) -> Result<()> {
        let base = self.timeline_ms;
        match element {
            ResponseElement::Think { think } => {
                for step in think {
                    let target = base.saturating_add(step.relative_time);
                    let delta = target.saturating_sub(self.timeline_ms);
                    self.advance(delta);
                    self.push_event(PlaybackEventKind::Thinking {
                        text: step.content.clone(),
                    });
                }
            }
            ResponseElement::Assistant { assistant } => {
                for step in assistant {
                    let target = base.saturating_add(step.relative_time);
                    let delta = target.saturating_sub(self.timeline_ms);
                    self.advance(delta);
                    let text =
                        extract_text_from_content_block(&step.content).unwrap_or_else(|| {
                            serde_yaml::to_string(&step.content).unwrap_or_default()
                        });
                    self.push_event(PlaybackEventKind::Assistant { text });
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

    fn process_user_inputs(&mut self, entries: &[UserInputEntry]) -> Result<()> {
        for entry in entries {
            if entry.relative_time < self.timeline_ms {
                return Err(ScenarioError::Playback(format!(
                    "userInputs relativeTime {} is earlier than current timeline position {}",
                    entry.relative_time, self.timeline_ms
                )));
            }
            let target_time = entry.relative_time;
            let target = entry.target.clone();
            let prompt = extract_prompt_from_input_value(&entry.input)
                .or_else(|| extract_text_from_content_block(&entry.input))
                .unwrap_or_else(|| serde_yaml::to_string(&entry.input).unwrap_or_default());

            let delta = target_time.saturating_sub(self.timeline_ms);
            self.advance(delta);
            self.push_event(PlaybackEventKind::UserInput {
                target,
                value: prompt,
            });
        }
        Ok(())
    }

    fn handle_tool_use(&mut self, data: &ToolUseData) -> Result<()> {
        self.push_event(PlaybackEventKind::ToolStart {
            tool_name: data.tool_name.clone(),
            args: serde_yaml::to_value(&data.args).unwrap_or(Value::Null),
        });

        if let Some(progress) = &data.progress {
            for step in progress {
                let target = self.timeline_ms.saturating_add(step.relative_time);
                let delta = target.saturating_sub(self.timeline_ms);
                self.advance(delta);
                self.push_event(PlaybackEventKind::ToolProgress {
                    tool_name: data.tool_name.clone(),
                    message: step.message.clone(),
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
