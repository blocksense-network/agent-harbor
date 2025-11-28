// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{Result, ScenarioError, model::Scenario};
use std::fs;
use std::path::{Path, PathBuf};

/// Source of scenarios to load.
#[derive(Debug, Clone)]
pub enum ScenarioSource {
    File(PathBuf),
    Directory(PathBuf),
}

/// Loaded scenario along with its origin path.
#[derive(Debug, Clone)]
pub struct ScenarioRecord {
    pub scenario: Scenario,
    pub path: PathBuf,
}

/// Helper that loads and stores scenarios from multiple locations.
#[derive(Debug, Default)]
pub struct ScenarioLoader {
    scenarios: Vec<ScenarioRecord>,
}

impl ScenarioLoader {
    pub fn from_sources<S>(sources: S) -> Result<Self>
    where
        S: IntoIterator<Item = ScenarioSource>,
    {
        let mut loader = Self::default();
        for source in sources {
            loader.load_source(source)?;
        }
        if loader.scenarios.is_empty() {
            return Err(ScenarioError::Empty);
        }
        Ok(loader)
    }

    pub fn scenarios(&self) -> &[ScenarioRecord] {
        &self.scenarios
    }

    pub fn into_records(self) -> Vec<ScenarioRecord> {
        self.scenarios
    }

    fn load_source(&mut self, source: ScenarioSource) -> Result<()> {
        match source {
            ScenarioSource::File(path) => {
                let scenario = Scenario::from_file(&path)?;
                self.scenarios.push(ScenarioRecord { scenario, path });
            }
            ScenarioSource::Directory(dir) => {
                self.load_directory(&dir)?;
            }
        }
        Ok(())
    }

    fn load_directory(&mut self, dir: &Path) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(ext, "yaml" | "yml") {
                    let scenario = Scenario::from_file(&path)?;
                    self.scenarios.push(ScenarioRecord { scenario, path });
                }
            }
        }
        Ok(())
    }
}

impl Scenario {
    /// Convenience helper to load a scenario directly from a file path.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let contents = fs::read_to_string(path.as_ref())?;
        let scenario: Scenario = serde_yaml::from_str(&contents)?;
        if scenario.name.trim().is_empty() {
            return Err(ScenarioError::Validation(format!(
                "Scenario {:?} is missing 'name'",
                path.as_ref()
            )));
        }
        validate_timeline(&scenario, path.as_ref())?;
        Ok(scenario)
    }
}

fn validate_timeline(scenario: &Scenario, path: &Path) -> Result<()> {
    for event in &scenario.timeline {
        match event {
            crate::model::TimelineEvent::LlmResponse { llm_response } => {
                for element in llm_response {
                    match element {
                        crate::model::ResponseElement::Think { think } => {
                            ensure_monotonic("think", think.iter().map(|s| s.relative_time), path)?;
                        }
                        crate::model::ResponseElement::Assistant { assistant } => {
                            ensure_monotonic(
                                "assistant",
                                assistant.iter().map(|s| s.relative_time),
                                path,
                            )?;
                        }
                        crate::model::ResponseElement::AgentToolUse { agent_tool_use } => {
                            if let Some(progress) = &agent_tool_use.progress {
                                ensure_monotonic(
                                    "progress",
                                    progress.iter().map(|s| s.relative_time),
                                    path,
                                )?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            crate::model::TimelineEvent::UserInputs { user_inputs } => {
                ensure_monotonic(
                    "userInputs",
                    user_inputs.iter().map(|s| s.relative_time),
                    path,
                )?;
            }
            crate::model::TimelineEvent::Status { .. } => {}
            _ => {}
        }
    }
    Ok(())
}

fn ensure_monotonic<I>(label: &str, times: I, path: &Path) -> Result<()>
where
    I: Iterator<Item = u64>,
{
    let mut prev: Option<u64> = None;
    for t in times {
        if let Some(p) = prev {
            if t < p {
                return Err(ScenarioError::Validation(format!(
                    "Scenario {:?} has non-monotonic {} relativeTime ({} < {})",
                    path, label, t, p
                )));
            }
        }
        prev = Some(t);
    }
    Ok(())
}
