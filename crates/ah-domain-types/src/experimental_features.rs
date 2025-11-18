// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Experimental features and their configuration
//!
//! This module defines experimental features that can be enabled/disabled
//! in the Agent Harbor system. Experimental features typically include
//! new agents, modes, or functionality that is not yet stable.

use serde::{Deserialize, Serialize};
use strum::EnumIter;

/// Experimental features that can be enabled/disabled
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, EnumIter,
)]
#[serde(rename_all = "kebab-case")]
pub enum ExperimentalFeature {
    /// Google Gemini agent
    Gemini,
    /// GitHub Copilot CLI agent
    Copilot,
    /// Cursor CLI agent
    CursorCli,
    /// Block's Goose agent
    Goose,
}

impl std::fmt::Display for ExperimentalFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExperimentalFeature::Gemini => write!(f, "gemini"),
            ExperimentalFeature::Copilot => write!(f, "copilot"),
            ExperimentalFeature::CursorCli => write!(f, "cursor-cli"),
            ExperimentalFeature::Goose => write!(f, "goose"),
        }
    }
}

impl std::str::FromStr for ExperimentalFeature {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gemini" => Ok(ExperimentalFeature::Gemini),
            "copilot" => Ok(ExperimentalFeature::Copilot),
            "cursor-cli" | "cursor_cli" => Ok(ExperimentalFeature::CursorCli),
            "goose" => Ok(ExperimentalFeature::Goose),
            _ => Err(format!("Unknown experimental feature: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn test_experimental_feature_iter() {
        let features: Vec<ExperimentalFeature> = ExperimentalFeature::iter().collect();
        assert_eq!(features.len(), 4);
        assert!(features.contains(&ExperimentalFeature::Gemini));
        assert!(features.contains(&ExperimentalFeature::Copilot));
        assert!(features.contains(&ExperimentalFeature::CursorCli));
        assert!(features.contains(&ExperimentalFeature::Goose));
    }
}
