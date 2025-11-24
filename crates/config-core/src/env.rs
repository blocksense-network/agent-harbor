// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Environment variable overlay functionality

use anyhow::Result;
use serde_json::Value as J;

/// Create JSON overlay from AH_* environment variables
pub fn env_overlay() -> Result<J> {
    // Use config-rs to get AH_* with AH_REMOTE_SERVER -> "remote-server"
    let built = config::Config::builder()
        .add_source(config::Environment::with_prefix("AH").convert_case(config::Case::Kebab))
        .build()?;

    // Deserialize to JSON; this creates nested structures, but we need flat keys
    let nested = built.try_deserialize::<serde_json::Map<String, J>>()?;

    // Flatten the nested structure to match TOML flat keys
    let mut flat = serde_json::Map::new();
    flatten_keys(&nested, &mut flat, String::new());

    Ok(J::Object(flat))
}

/// Recursively flatten nested JSON structures into dotted keys
fn flatten_keys(
    nested: &serde_json::Map<String, J>,
    flat: &mut serde_json::Map<String, J>,
    prefix: String,
) {
    for (key, value) in nested {
        let new_key = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{}.{}", prefix, key)
        };

        match value {
            J::Object(obj) => flatten_keys(obj, flat, new_key),
            _ => {
                flat.insert(new_key, value.clone());
            }
        }
    }
}

/// Create JSON overlay from CLI flag key=value pairs
pub fn flags_overlay(kv_pairs: &[(&str, &str)]) -> J {
    let mut root = serde_json::json!({});
    for (k, v) in kv_pairs {
        crate::merge::insert_dotted(&mut root, k, serde_json::Value::String(v.to_string()));
    }
    root
}
