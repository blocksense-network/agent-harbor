#![allow(clippy::disallowed_methods)] // Build scripts must emit Cargo directives via println!
// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::{env, fs, path::Path};

// Include minimal schema types directly in the build script
// (We can't depend on other crates in build scripts, so we duplicate the essentials)
mod schema {
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    /// Minimal UI enum for schema generation
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
    #[serde(rename_all = "kebab-case")]
    pub enum Ui {
        Tui,
        Webui,
    }

    /// Minimal UI root struct for schema generation
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "kebab-case")]
    pub struct UiRoot {
        pub ui: Option<Ui>,
        pub ui_default: Option<String>,
        pub browser_automation: Option<bool>,
        pub browser_profile: Option<String>,
        pub chatgpt_username: Option<String>,
        pub codex_workspace: Option<String>,
        pub remote_server: Option<String>,
        #[serde(rename = "tui-font-style")]
        pub tui_font_style: Option<String>,
        #[serde(rename = "tui-font")]
        pub tui_font: Option<String>,
        pub log_level: Option<String>,
        pub terminal_multiplexer: Option<String>,
        pub editor: Option<String>,
        pub service_base_url: Option<String>,
    }

    /// Minimal repo section for schema generation
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
    #[serde(rename_all = "kebab-case")]
    pub struct RepoSection {
        pub supported_agents: Option<SupportedAgents>,
        #[serde(default)]
        pub init: RepoInit,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
    #[serde(rename_all = "kebab-case")]
    pub struct RepoInit {
        pub vcs: Option<String>,
        pub devenv: Option<String>,
        #[serde(default)]
        pub devcontainer: bool,
        #[serde(default)]
        pub direnv: bool,
        pub task_runner: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
    #[serde(rename_all = "kebab-case")]
    pub enum SupportedAgents {
        All,
        List(Vec<String>),
    }

    /// Minimal server types for schema generation
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "kebab-case")]
    pub struct Server {
        pub name: Option<String>,
        pub url: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "kebab-case")]
    pub struct Fleet {
        pub name: Option<String>,
        #[serde(default)]
        pub member: Vec<FleetMember>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "kebab-case")]
    pub struct FleetMember {
        #[serde(rename = "type")]
        pub r#type: Option<String>,
        pub profile: Option<String>,
        pub url: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "kebab-case")]
    pub struct Sandbox {
        pub name: Option<String>,
        #[serde(rename = "type")]
        pub kind: Option<String>,
    }

    /// The root configuration schema
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "kebab-case")]
    #[serde(deny_unknown_fields)]
    pub struct SchemaRoot {
        #[serde(flatten)]
        pub ui: UiRoot,
        pub repo: Option<RepoSection>,
        #[serde(default)]
        pub server: Vec<Server>,
        #[serde(default)]
        pub fleet: Vec<Fleet>,
        #[serde(default)]
        pub sandbox: Vec<Sandbox>,
        #[serde(default)]
        pub enforced: Vec<String>,
    }
}

fn main() {
    // Generate schema from SchemaRoot with inlined subschemas for cleaner output
    let settings =
        schemars::r#gen::SchemaSettings::draft2019_09().with(|s| s.inline_subschemas = true);
    let r#gen = settings.into_generator();
    let root_schema = r#gen.into_root_schema_for::<schema::SchemaRoot>();
    let generated = serde_json::to_value(root_schema).unwrap();

    // Read expected schema if it exists
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let expected_path = Path::new(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("specs")
        .join("config.schema.expected.json");

    if expected_path.exists() {
        let expected_str =
            fs::read_to_string(&expected_path).expect("Failed to read expected schema file");
        let expected: serde_json::Value =
            serde_json::from_str(&expected_str).expect("Failed to parse expected schema JSON");

        // Canonicalize both for comparison (sort keys, etc.)
        let generated_canonical = canonicalize_json(generated);
        let expected_canonical = canonicalize_json(expected);

        if generated_canonical != expected_canonical {
            // Update the expected schema with the new flattened version
            let generated_pretty = serde_json::to_string_pretty(&generated_canonical).unwrap();
            fs::write(&expected_path, &generated_pretty).expect("Failed to update expected schema");

            // Informational output removed to keep build scripts quiet
        }
    } else {
        // Create expected schema file
        let expected_dir = expected_path.parent().unwrap();
        fs::create_dir_all(expected_dir).expect("Failed to create specs directory");

        let generated_pretty = serde_json::to_string_pretty(&generated).unwrap();
        fs::write(&expected_path, generated_pretty).expect("Failed to write expected schema");

        // Informational output removed to keep build scripts quiet
    }

    println!("cargo:rerun-if-changed=src/schema.rs");
    println!("cargo:rerun-if-changed={}", expected_path.display());
}

/// Canonicalize JSON for comparison by sorting object keys recursively
fn canonicalize_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(obj) => {
            let mut sorted: Vec<_> = obj.into_iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            let sorted_obj = sorted.into_iter().map(|(k, v)| (k, canonicalize_json(v))).collect();
            serde_json::Value::Object(sorted_obj)
        }
        serde_json::Value::Array(arr) => {
            let canonicalized = arr.into_iter().map(canonicalize_json).collect();
            serde_json::Value::Array(canonicalized)
        }
        _ => value,
    }
}
