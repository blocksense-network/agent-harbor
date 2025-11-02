// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Space measurement utilities for different filesystem types.
//!
//! This module provides utilities for measuring filesystem space usage across different
//! filesystem types, ported from the legacy Ruby filesystem_space_utils.rb.

use std::path::Path;
use std::process::Command;

/// Parse size string with units (B, K, KB, KiB, M, MB, MiB, etc.) to bytes.
///
/// # Arguments
/// * `size_string` - Size string with optional unit (e.g., "1.5GB", "512MB")
///
/// # Returns
/// Size in bytes as u64.
pub fn parse_size_to_bytes(size_string: &str) -> u64 {
    if size_string.is_empty() {
        return 0;
    }

    let re = regex::Regex::new(r"(\d+(?:\.\d+)?)\s*(\w*)").unwrap();
    let captures = match re.captures(size_string.trim()) {
        Some(caps) => caps,
        None => return 0,
    };

    let value: f64 = captures[1].parse().unwrap_or(0.0);
    let unit = captures[2].to_uppercase();

    match unit.as_str() {
        "" | "B" | "BYTES" => value as u64,
        "K" | "KB" | "KIB" => (value * 1024.0) as u64,
        "M" | "MB" | "MIB" => (value * 1024.0 * 1024.0) as u64,
        "G" | "GB" | "GIB" => (value * 1024.0 * 1024.0 * 1024.0) as u64,
        "T" | "TB" | "TIB" => (value * 1024.0 * 1024.0 * 1024.0 * 1024.0) as u64,
        _ => value as u64,
    }
}

/// Get Btrfs filesystem usage in bytes.
///
/// # Arguments
/// * `mount_point` - Path to the Btrfs mount point
///
/// # Returns
/// Used space in bytes, or 0 if measurement fails.
pub fn btrfs_filesystem_used_space(mount_point: &Path) -> u64 {
    let output = Command::new("btrfs").arg("filesystem").arg("usage").arg(mount_point).output();

    let output = match output {
        Ok(out) => out,
        Err(_) => return 0,
    };

    if !output.status.success() {
        return 0;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let re = regex::Regex::new(r"Used:\s+(\d+(?:\.\d+)?)\s*(\w+)").unwrap();

    if let Some(captures) = re.captures(&stdout) {
        let value: f64 = captures[1].parse().unwrap_or(0.0);
        let unit = captures[2].to_string();
        parse_size_to_bytes(&format!("{}{}", value, unit))
    } else {
        0
    }
}

/// Get ZFS pool usage in bytes.
///
/// # Arguments
/// * `pool_name` - Name of the ZFS pool
///
/// # Returns
/// Used space in bytes, or 0 if measurement fails.
pub fn zfs_pool_used_space(pool_name: &str) -> u64 {
    let output = Command::new("zpool")
        .arg("list")
        .arg("-H")
        .arg("-o")
        .arg("used")
        .arg(pool_name)
        .output();

    let output = match output {
        Ok(out) => out,
        Err(_) => return 0,
    };

    if !output.status.success() {
        return 0;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_size_to_bytes(stdout.trim())
}

/// Get general filesystem usage using df in 1-byte blocks.
///
/// # Arguments
/// * `mount_point` - Path to the filesystem mount point
///
/// # Returns
/// Used space in bytes, or 0 if measurement fails.
pub fn df_filesystem_used_space(mount_point: &Path) -> u64 {
    let output = Command::new("df").arg("-B1").arg(mount_point).output();

    let output = match output {
        Ok(out) => out,
        Err(_) => return 0,
    };

    if !output.status.success() {
        return 0;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() < 2 {
        return 0;
    }

    let fields: Vec<&str> = lines[1].split_whitespace().collect();
    if fields.len() < 3 {
        return 0;
    }

    // Used space is in field 2 (1B-blocks format)
    fields[2].parse().unwrap_or(0)
}

/// Generic method that tries to determine the best space measurement approach.
///
/// # Arguments
/// * `path_or_pool` - Either a mount point path or pool name
/// * `filesystem_type` - Optional filesystem type hint
///
/// # Returns
/// Used space in bytes.
pub fn measure_filesystem_space(path_or_pool: &str, filesystem_type: Option<&str>) -> u64 {
    match filesystem_type.map(|s| s.to_lowercase()).as_deref() {
        Some("btrfs") => btrfs_filesystem_used_space(Path::new(path_or_pool)),
        Some("zfs") => zfs_pool_used_space(path_or_pool),
        _ => {
            // Try to auto-detect or fall back to df
            let path = Path::new(path_or_pool);
            if path.exists() && path.is_dir() {
                df_filesystem_used_space(path)
            } else {
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ah_test_utils::logged_test]
    fn test_parse_size_to_bytes() {
        assert_eq!(parse_size_to_bytes("0"), 0);
        assert_eq!(parse_size_to_bytes("512"), 512);
        assert_eq!(parse_size_to_bytes("1K"), 1024);
        assert_eq!(parse_size_to_bytes("1KB"), 1024);
        assert_eq!(parse_size_to_bytes("1.5MB"), 1_572_864);
        assert_eq!(parse_size_to_bytes("2GB"), 2_147_483_648);
        assert_eq!(parse_size_to_bytes("1TB"), 1_099_511_627_776);
    }

    #[ah_test_utils::logged_test]
    fn test_parse_size_edge_cases() {
        assert_eq!(parse_size_to_bytes(""), 0);
        assert_eq!(parse_size_to_bytes("invalid"), 0);
        assert_eq!(parse_size_to_bytes("1.5INVALID"), 1); // Invalid unit defaults to bytes
    }
}
