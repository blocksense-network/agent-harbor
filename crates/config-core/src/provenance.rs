// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Provenance tracking for configuration values

use serde_json::Value as J;
use std::collections::{BTreeMap, BTreeSet};

/// Configuration scope precedence order
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Scope {
    System,
    User,
    Repo,
    RepoUser,
    Env,
    CliConfig,
    Flags,
}

/// Provenance information for configuration values
#[derive(Default, Clone, Debug)]
pub struct Provenance {
    /// Maps dotted key paths to the winning scope
    pub winner: BTreeMap<String, Scope>,
    /// Maps dotted key paths to change history [(scope, value)]
    pub changes: BTreeMap<String, Vec<(Scope, J)>>,
    /// Set of dotted key paths that are enforced
    pub enforced: BTreeSet<String>,
}

/// Record provenance changes between two JSON configurations
pub fn record_diff(before: &J, after: &J, scope: Scope, out: &mut Provenance, prefix: &str) {
    use serde_json::Value::*;
    match (before, after) {
        (a, b) if a == b => (),
        (Object(ao), Object(bo)) => {
            let mut keys: Vec<&str> = ao.keys().chain(bo.keys()).map(|s| s.as_str()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let pfx = if prefix.is_empty() {
                    k.to_string()
                } else {
                    format!("{prefix}.{k}")
                };
                record_diff(
                    ao.get(k).unwrap_or(&Null),
                    bo.get(k).unwrap_or(&Null),
                    scope,
                    out,
                    &pfx,
                );
            }
        }
        (Array(_), Array(_)) => {
            // Only record array changes if they actually differ
            if before != after {
                out.winner.insert(prefix.to_string(), scope);
                out.changes.entry(prefix.to_string()).or_default().push((scope, after.clone()));
            }
        }
        _ => {
            // Only record scalar/null changes if they actually differ
            if before != after {
                out.winner.insert(prefix.to_string(), scope);
                out.changes.entry(prefix.to_string()).or_default().push((scope, after.clone()));
            }
        }
    }
}
