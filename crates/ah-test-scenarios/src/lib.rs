//! Scenario model and loader for TUI tests

use ah_rest_api_contract::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioTerminal {
    pub width: Option<u16>,
    pub height: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Step {
    AdvanceMs {
        ms: u64,
    },
    Key {
        key: String,
    },
    Sse {
        event: SessionEvent,
    },
    AssertVm {
        focus: String,
        selected: Option<usize>,
    },
    Snapshot {
        name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub name: String,
    pub terminal: Option<ScenarioTerminal>,
    pub steps: Vec<Step>,
}

impl Scenario {
    pub fn from_str(json: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}
