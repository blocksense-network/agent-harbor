// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{
    Result, ScenarioError,
    model::{Rules, Scenario, SymbolTable, evaluate_rules},
};
use serde_yaml::Value;
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
    symbols: SymbolTable,
}

impl ScenarioLoader {
    pub fn from_sources<S>(sources: S) -> Result<Self>
    where
        S: IntoIterator<Item = ScenarioSource>,
    {
        let mut loader = Self {
            scenarios: Vec::new(),
            symbols: SymbolTable::new(),
        };
        loader.load_sources(sources)?;
        Ok(loader)
    }

    pub fn from_sources_with_symbols<S>(sources: S, symbols: SymbolTable) -> Result<Self>
    where
        S: IntoIterator<Item = ScenarioSource>,
    {
        let mut loader = Self {
            scenarios: Vec::new(),
            symbols,
        };
        loader.load_sources(sources)?;
        Ok(loader)
    }

    fn load_sources<S>(&mut self, sources: S) -> Result<()>
    where
        S: IntoIterator<Item = ScenarioSource>,
    {
        for source in sources {
            self.load_source(source)?;
        }
        if self.scenarios.is_empty() {
            return Err(ScenarioError::Empty);
        }
        Ok(())
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
                let scenario = Scenario::from_file_with_symbols(&path, &self.symbols)?;
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
                    let scenario = Scenario::from_file_with_symbols(&path, &self.symbols)?;
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
        let symbols = SymbolTable::new();
        Self::from_file_with_symbols(path, &symbols)
    }

    /// Load a scenario with an explicit symbol table for rule evaluation.
    pub fn from_file_with_symbols(path: impl AsRef<Path>, symbols: &SymbolTable) -> Result<Self> {
        let contents = fs::read_to_string(path.as_ref())?;
        let raw: Value = serde_yaml::from_str(&contents)?;
        let resolved = resolve_rules_recursive(raw, symbols)?;
        validate_modern_timeline_shape(&resolved, path.as_ref())?;
        let scenario: Scenario = serde_yaml::from_value(resolved)?;
        if scenario.name.trim().is_empty() {
            return Err(ScenarioError::Validation(format!(
                "Scenario {:?} is missing 'name'",
                path.as_ref()
            )));
        }
        validate_timeline(&scenario, path.as_ref())?;
        // Enforce ACP-specific validation at load time
        if let Err(msg) = scenario.validate_acp_requirements_with_base(path.as_ref().parent()) {
            return Err(ScenarioError::Validation(format!(
                "Scenario {:?} failed ACP validation: {}",
                path.as_ref(),
                msg
            )));
        }
        Ok(scenario)
    }
}

/// Resolve `rules` blocks recursively, returning a merged YAML value.
pub(crate) fn resolve_rules_recursive(value: Value, symbols: &SymbolTable) -> Result<Value> {
    match value {
        Value::Mapping(mut map) => {
            // First resolve child values
            let keys: Vec<_> = map.keys().cloned().collect();
            for k in keys {
                if let Some(v) = map.get_mut(&k) {
                    let new_v = resolve_rules_recursive(v.clone(), symbols)?;
                    *v = new_v;
                }
            }

            // Apply rules at this level if present
            if let Some(rules_value) = map.remove(Value::String("rules".to_string())) {
                if let Ok(rules) = serde_yaml::from_value::<Rules>(rules_value.clone()) {
                    let merged = evaluate_rules(&rules, symbols).map_err(|e| {
                        ScenarioError::Validation(format!("Rule evaluation error: {}", e))
                    })?;
                    map = merge_into_mapping(map, merged)?;
                } else {
                    return Err(ScenarioError::Validation("Invalid rules block".into()));
                }
            }
            Ok(Value::Mapping(map))
        }
        Value::Sequence(seq) => {
            let mut out = Vec::with_capacity(seq.len());
            for item in seq {
                out.push(resolve_rules_recursive(item, symbols)?);
            }
            Ok(Value::Sequence(out))
        }
        other => Ok(other),
    }
}

fn merge_into_mapping(
    mut base: serde_yaml::Mapping,
    overlay: Value,
) -> Result<serde_yaml::Mapping> {
    match overlay {
        Value::Null => Ok(base),
        Value::Mapping(overlay_map) => {
            for (k, v) in overlay_map {
                base.insert(k, v);
            }
            Ok(base)
        }
        other => Err(ScenarioError::Validation(format!(
            "Rules config must be a mapping, got {:?}",
            other
        ))),
    }
}

/// Reject obvious legacy timeline shapes before deserialization.
fn validate_modern_timeline_shape(doc: &Value, path: &Path) -> Result<()> {
    if let Value::Mapping(map) = doc {
        if map.contains_key(Value::String("events".to_string()))
            || map.contains_key(Value::String("assertions".to_string()))
        {
            return Err(ScenarioError::Validation(format!(
                "Scenario {:?} uses legacy keys 'events'/'assertions'; only 'timeline' is supported",
                path
            )));
        }
        if let Some(timeline) = map.get(Value::String("timeline".to_string())) {
            if let Value::Sequence(seq) = timeline {
                for (idx, item) in seq.iter().enumerate() {
                    if !item.is_mapping() {
                        return Err(ScenarioError::Validation(format!(
                            "Scenario {:?} timeline item {} is not a mapping; legacy shapes are unsupported",
                            path, idx
                        )));
                    }
                }
            } else {
                return Err(ScenarioError::Validation(format!(
                    "Scenario {:?} has non-sequence timeline; expected list of events",
                    path
                )));
            }
        }
    }
    Ok(())
}

fn validate_timeline(scenario: &Scenario, path: &Path) -> Result<()> {
    let mut timeline_ms: u64 = 0;
    for event in &scenario.timeline {
        match event {
            crate::model::TimelineEvent::LlmResponse { llm_response, .. } => {
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
                for entry in user_inputs {
                    if entry.relative_time < timeline_ms {
                        return Err(ScenarioError::Validation(format!(
                            "Scenario {:?} has userInputs relativeTime {} earlier than current timeline {}",
                            path, entry.relative_time, timeline_ms
                        )));
                    }
                }
                ensure_monotonic(
                    "userInputs",
                    user_inputs.iter().map(|s| s.relative_time),
                    path,
                )?;
                if let Some(last) = user_inputs.last() {
                    timeline_ms = last.relative_time;
                }
            }
            crate::model::TimelineEvent::Status { .. } => {}
            crate::model::TimelineEvent::AdvanceMs { base_time_delta } => {
                timeline_ms = timeline_ms.saturating_add(*base_time_delta);
            }
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
