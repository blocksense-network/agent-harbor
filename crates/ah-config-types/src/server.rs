//! Server, fleet, and sandbox-related configuration types

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Server {
    /// Server name for reference
    pub name: Option<String>,
    /// Server URL
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Fleet {
    /// Fleet name
    pub name: Option<String>,
    /// Fleet members - combination of local testing strategies and remote servers
    #[serde(default)]
    pub member: Vec<FleetMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct FleetMember {
    /// Member type - either a sandbox profile name or "remote"
    #[serde(rename = "type")]
    pub r#type: Option<String>,
    /// For sandbox members: the sandbox profile name. For remote members: server URL
    pub profile: Option<String>,
    /// For remote members: explicit server URL (alternative to server name reference)
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Sandbox {
    /// Sandbox profile name
    pub name: Option<String>,
    /// Sandbox type
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

