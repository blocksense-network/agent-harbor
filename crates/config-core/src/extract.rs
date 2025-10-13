//! Typed extraction utilities for distributed configuration access

use serde::de::DeserializeOwned;
use serde_json::Value as J;

/// Extract the entire root configuration as a typed value
pub fn get<T: DeserializeOwned>(root: &J) -> anyhow::Result<T> {
    serde_path_to_error::deserialize(root.clone())
        .map_err(|e| anyhow::anyhow!("Root extraction failed: {}", e))
}

/// Extract a subsection of configuration at a dotted path
pub fn get_at<T: DeserializeOwned>(root: &J, dotted: &str) -> anyhow::Result<T> {
    let mut cur = root;
    for p in dotted.split('.') {
        cur = cur.get(p).ok_or_else(|| anyhow::anyhow!("missing path: {}", dotted))?;
    }
    serde_path_to_error::deserialize(cur.clone())
        .map_err(|e| anyhow::anyhow!("Path '{}' extraction failed: {}", dotted, e))
}
