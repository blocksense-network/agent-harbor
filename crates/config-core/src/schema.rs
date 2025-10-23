// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Schema root definition for configuration validation.
//!
//! This module defines the single strongly-typed structure that represents the
//! canonical shape of the entire configuration. This type is used only for
//! schema generation and validation - actual configuration access is done
//! through distributed typed views.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The root configuration schema that defines the shape of all possible configuration.
/// This type is used for schema generation and validation only.
///
/// UI-related fields are flattened from UiRoot, while nested sections like repo
/// remain as direct fields. Collections stay at known top-level keys.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct SchemaRoot {
    /// UI-related configuration gets flattened into root properties
    #[serde(flatten)]
    pub ui: ah_config_types::ui::UiRoot,

    /// Keep path-local sections as fields (not flattened):
    pub repo: Option<ah_config_types::repo::RepoSection>,

    /// Collections stay at known top-level keys
    #[serde(default)]
    pub server: Vec<ah_config_types::server::Server>,
    #[serde(default)]
    pub fleet: Vec<ah_config_types::server::Fleet>,
    #[serde(default)]
    pub sandbox: Vec<ah_config_types::server::Sandbox>,

    /// Optional: a system-only key to declare enforced dotted keys
    #[serde(default)]
    pub enforced: Vec<String>,
}
