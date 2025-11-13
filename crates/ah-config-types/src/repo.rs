// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository-related configuration types

use serde::{Deserialize, Serialize};

// Alias for backward compatibility
pub type RepoConfig = RepoSection;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub struct RepoInit {
    /// Version control system to use
    pub vcs: Option<Vcs>,
    /// Development environment type
    pub devenv: Option<DevEnv>,
    /// Whether to use devcontainers
    #[serde(default)]
    pub devcontainer: bool,
    /// Whether to use direnv
    #[serde(default)]
    pub direnv: bool,
    /// Task runner to use
    pub task_runner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub struct RepoSection {
    /// Supported agents - "all" or explicit array of agent names
    pub supported_agents: Option<SupportedAgents>,
    /// Repository initialization settings
    #[serde(default)]
    pub init: RepoInit,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum SupportedAgents {
    All,
    List(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Vcs {
    Git,
    Hg,
    Fossil,
    Bzr,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum DevEnv {
    Nix,
    Spack,
    Bazel,
    Custom,
    #[serde(rename = "no")]
    No,
}
