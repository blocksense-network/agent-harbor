// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Enforcement functionality for masking enforced keys

use serde_json::Value as J;
use std::collections::BTreeSet;

/// Enforcement configuration
#[derive(Default, Clone)]
pub struct Enforcement {
    /// Set of dotted key paths that are enforced
    pub keys: BTreeSet<String>,
}

/// Mask enforced keys in a layer by removing them
pub fn mask_layer(layer: &mut J, enforcement: &Enforcement) {
    for key in &enforcement.keys {
        remove_dotted(layer, key);
    }
}

/// Remove a dotted key path from JSON
fn remove_dotted(v: &mut J, dotted: &str) {
    let parts: Vec<&str> = dotted.split('.').collect();
    fn rec(v: &mut J, parts: &[&str]) {
        if parts.is_empty() {
            return;
        }
        if let J::Object(map) = v {
            if parts.len() == 1 {
                map.remove(parts[0]);
            } else if let Some(next) = map.get_mut(parts[0]) {
                rec(next, &parts[1..]);
            }
        }
    }
    rec(v, &parts);
}
