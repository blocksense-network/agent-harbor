// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Environment variable overlay functionality

use anyhow::Result;
use serde_json::Value as J;

/// Create JSON overlay from AH_* environment variables
pub fn env_overlay() -> Result<J> {
    // Use config-rs to get AH_* with AH_REMOTE_SERVER -> "remote-server"
    let built = config::Config::builder()
        .add_source(
            config::Environment::with_prefix("AH")
                .separator("_")
                .convert_case(config::Case::Kebab),
        )
        .build()?;

    // Deserialize to JSON; this reflects nested structures via kebab segments
    Ok(serde_json::to_value(
        built.try_deserialize::<serde_json::Map<String, J>>()?,
    )?)
}

/// Create JSON overlay from CLI flag key=value pairs
pub fn flags_overlay(kv_pairs: &[(&str, &str)]) -> J {
    let mut root = serde_json::json!({});
    for (k, v) in kv_pairs {
        crate::merge::insert_dotted(&mut root, k, serde_json::Value::String(v.to_string()));
    }
    root
}
