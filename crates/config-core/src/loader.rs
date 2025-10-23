// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! TOML loading and JSON validation functionality

use anyhow::{Context, Result};
use serde_json::Value as J;
use std::path::Path;

/// Parse TOML string to JSON value for schema validation
pub fn parse_toml_to_json(toml_str: &str) -> Result<J> {
    let toml: toml::Value = toml_str.parse::<toml::Value>()?;
    Ok(toml_to_json(toml))
}

/// Convert TOML value to JSON value for schema validation
fn toml_to_json(t: toml::Value) -> J {
    // Round-trip TOML -> JSON for schema validation + merging.
    // toml::Value implements Serialize; convert via serde_json.
    serde_json::to_value(t).expect("TOML->JSON conversion should not fail")
}

/// Validate JSON against the configuration schema
pub fn validate_against_schema(v: &J) -> Result<()> {
    use jsonschema::{Draft, JSONSchema};
    use std::sync::OnceLock;

    static SCHEMA: OnceLock<J> = OnceLock::new();
    let schema = SCHEMA.get_or_init(|| {
        let rs = schemars::schema_for!(crate::schema::SchemaRoot);
        serde_json::to_value(rs).unwrap()
    });

    static VALIDATOR: OnceLock<JSONSchema> = OnceLock::new();
    let validator = VALIDATOR.get_or_init(|| {
        JSONSchema::options()
            .with_draft(Draft::Draft202012)
            .compile(schema)
            .expect("Schema compilation should not fail")
    });

    let validation_result = validator.validate(v);
    if let Err(errors) = validation_result {
        let error_msg = errors.map(|e| e.to_string()).collect::<Vec<_>>().join("\n  - ");
        anyhow::bail!("Config schema validation failed:\n  - {}", error_msg);
    }

    Ok(())
}

/// Represents a loaded configuration layer
#[derive(Debug, Clone)]
pub struct Layer {
    pub scope: crate::Scope,
    pub json: J,
}

/// Load and validate a configuration layer from file
pub fn read_layer_from_file(path: &Path, scope: crate::Scope) -> Result<Layer> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading config file {:?}", path))?;

    let json = parse_toml_to_json(&content)?;
    validate_against_schema(&json)?;

    Ok(Layer { scope, json })
}
