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
                let scenario = load_scenario_from_path(&path)?;
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
                    let scenario = load_scenario_from_path(&path)?;
                    self.scenarios.push(ScenarioRecord { scenario, path });
                }
            }
        }
        Ok(())
    }
}

/// Load a single scenario from a YAML file.
pub fn load_scenario_from_path(path: &Path) -> Result<Scenario> {
    let contents = fs::read_to_string(path)?;
    let scenario: Scenario = serde_yaml::from_str(&contents)?;
    if scenario.name.trim().is_empty() {
        return Err(ScenarioError::Validation(format!(
            "Scenario {:?} is missing 'name'",
            path
        )));
    }
    Ok(scenario)
}
