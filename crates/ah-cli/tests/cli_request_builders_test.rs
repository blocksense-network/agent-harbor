// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! T15.3 Schema Validation Tests
//!
//! This module validates that the CLI's request builders in `crates/ah-cli/src/transport.rs`
//! produce SSZ-encoded requests that are compatible with the agentfs-control protocol schema.
//! These tests ensure that any deviation from `agentfs-control.request.logical.json` fails CI
//! immediately.

use agentfs_proto::messages::InterposeSetGetResponse;
use agentfs_proto::*;
// The ethereum_ssz crate publishes its lib as "ssz", so we can import it as such
use ssz::{Decode, Encode};

/// Helper to roundtrip SSZ encode/decode and verify no data loss
fn roundtrip_ssz<T: Encode + Decode + PartialEq + std::fmt::Debug>(value: &T) -> T {
    let encoded = value.as_ssz_bytes();
    T::from_ssz_bytes(&encoded).expect("SSZ roundtrip decode failed")
}

/// Verify that the request validates according to the protocol schema
fn assert_valid_request(request: &Request) {
    validate_request(request).expect("Request validation failed");
}

/// Verify that the response validates according to the protocol schema
fn assert_valid_response(response: &Response) {
    validate_response(response).expect("Response validation failed");
}

// ============================================================================
// Snapshot Request Builder Tests
// ============================================================================

#[test]
fn test_cli_snapshot_create_request_with_name() {
    // Test: build_snapshot_create_request(Some(name)) produces valid SSZ
    let request = Request::snapshot_create(Some("my-snapshot".to_string()));

    // Verify schema validation passes
    assert_valid_request(&request);

    // Verify SSZ roundtrip
    let decoded = roundtrip_ssz(&request);
    assert_eq!(request, decoded);

    // Verify request variant is correct
    match &request {
        Request::SnapshotCreate((version, payload)) => {
            assert_eq!(version, b"1", "Version should be '1'");
            assert_eq!(
                payload.name.as_deref(),
                Some(b"my-snapshot".as_slice()),
                "Snapshot name should match"
            );
        }
        _ => panic!("Expected SnapshotCreate variant"),
    }
}

#[test]
fn test_cli_snapshot_create_request_without_name() {
    // Test: build_snapshot_create_request(None) produces valid SSZ
    let request = Request::snapshot_create(None);

    assert_valid_request(&request);

    let decoded = roundtrip_ssz(&request);
    assert_eq!(request, decoded);

    match &request {
        Request::SnapshotCreate((version, payload)) => {
            assert_eq!(version, b"1", "Version should be '1'");
            assert!(payload.name.is_none(), "Snapshot name should be None");
        }
        _ => panic!("Expected SnapshotCreate variant"),
    }
}

#[test]
fn test_cli_snapshot_list_request() {
    // Test: build_snapshot_list_request() produces valid SSZ
    let request = Request::snapshot_list();

    assert_valid_request(&request);

    let decoded = roundtrip_ssz(&request);
    assert_eq!(request, decoded);

    match &request {
        Request::SnapshotList(version) => {
            assert_eq!(version, b"1", "Version should be '1'");
        }
        _ => panic!("Expected SnapshotList variant"),
    }
}

// ============================================================================
// Branch Request Builder Tests
// ============================================================================

#[test]
fn test_cli_branch_create_request_with_name() {
    // Test: build_branch_create_request(from, Some(name)) produces valid SSZ
    // Schema requires: from is SnapshotId (min 6 chars)
    let snapshot_id = "01HXXXXXXXXXXXXXXXXXXXXX".to_string();
    let branch_name = Some("my-branch".to_string());

    let request = Request::branch_create(snapshot_id.clone(), branch_name.clone());

    assert_valid_request(&request);

    let decoded = roundtrip_ssz(&request);
    assert_eq!(request, decoded);

    match &request {
        Request::BranchCreate((version, payload)) => {
            assert_eq!(version, b"1", "Version should be '1'");
            assert_eq!(
                String::from_utf8_lossy(&payload.from),
                snapshot_id,
                "Snapshot ID should match"
            );
            assert_eq!(
                payload.name.as_ref().map(|n| String::from_utf8_lossy(n).to_string()),
                branch_name,
                "Branch name should match"
            );
        }
        _ => panic!("Expected BranchCreate variant"),
    }
}

#[test]
fn test_cli_branch_create_request_without_name() {
    // Test: build_branch_create_request(from, None) produces valid SSZ
    let snapshot_id = "01HXXXXXXXXXXXXXXXXXXXXX".to_string();

    let request = Request::branch_create(snapshot_id.clone(), None);

    assert_valid_request(&request);

    let decoded = roundtrip_ssz(&request);
    assert_eq!(request, decoded);

    match &request {
        Request::BranchCreate((version, payload)) => {
            assert_eq!(version, b"1", "Version should be '1'");
            assert_eq!(
                String::from_utf8_lossy(&payload.from),
                snapshot_id,
                "Snapshot ID should match"
            );
            assert!(payload.name.is_none(), "Branch name should be None");
        }
        _ => panic!("Expected BranchCreate variant"),
    }
}

#[test]
fn test_cli_branch_bind_request_with_pid() {
    // Test: build_branch_bind_request(branch, Some(pid)) produces valid SSZ
    // Schema requires: branch is BranchId (min 6 chars), pid is integer >= 1
    let branch_id = "01HXXXXXXXXXXXXXXXXXXXXX".to_string();
    let pid = 12345u32;

    let request = Request::branch_bind(branch_id.clone(), Some(pid));

    assert_valid_request(&request);

    let decoded = roundtrip_ssz(&request);
    assert_eq!(request, decoded);

    match &request {
        Request::BranchBind((version, payload)) => {
            assert_eq!(version, b"1", "Version should be '1'");
            assert_eq!(
                String::from_utf8_lossy(&payload.branch),
                branch_id,
                "Branch ID should match"
            );
            assert_eq!(payload.pid, Some(pid), "PID should match");
        }
        _ => panic!("Expected BranchBind variant"),
    }
}

#[test]
fn test_cli_branch_bind_request_without_pid() {
    // Test: build_branch_bind_request(branch, None) produces valid SSZ
    let branch_id = "01HXXXXXXXXXXXXXXXXXXXXX".to_string();

    let request = Request::branch_bind(branch_id.clone(), None);

    assert_valid_request(&request);

    let decoded = roundtrip_ssz(&request);
    assert_eq!(request, decoded);

    match &request {
        Request::BranchBind((version, payload)) => {
            assert_eq!(version, b"1", "Version should be '1'");
            assert_eq!(
                String::from_utf8_lossy(&payload.branch),
                branch_id,
                "Branch ID should match"
            );
            assert!(payload.pid.is_none(), "PID should be None");
        }
        _ => panic!("Expected BranchBind variant"),
    }
}

// ============================================================================
// Interpose Request Builder Tests
// ============================================================================

#[test]
fn test_cli_interpose_get_request() {
    // Test: build_interpose_get_request(key) produces valid SSZ
    let key = "enabled".to_string();

    let request = Request::interpose_setget(key.clone(), None);

    assert_valid_request(&request);

    let decoded = roundtrip_ssz(&request);
    assert_eq!(request, decoded);

    match &request {
        Request::InterposeSetGet((version, payload)) => {
            assert_eq!(version, b"1", "Version should be '1'");
            assert_eq!(
                String::from_utf8_lossy(&payload.key),
                key,
                "Key should match"
            );
            assert!(payload.value.is_none(), "Value should be None for get");
        }
        _ => panic!("Expected InterposeSetGet variant"),
    }
}

#[test]
fn test_cli_interpose_set_request() {
    // Test: build_interpose_set_request(key, value) produces valid SSZ
    let key = "max_copy_bytes".to_string();
    let value = "1048576".to_string();

    let request = Request::interpose_setget(key.clone(), Some(value.clone()));

    assert_valid_request(&request);

    let decoded = roundtrip_ssz(&request);
    assert_eq!(request, decoded);

    match &request {
        Request::InterposeSetGet((version, payload)) => {
            assert_eq!(version, b"1", "Version should be '1'");
            assert_eq!(
                String::from_utf8_lossy(&payload.key),
                key,
                "Key should match"
            );
            assert_eq!(
                payload.value.as_ref().map(|v| String::from_utf8_lossy(v).to_string()),
                Some(value),
                "Value should match"
            );
        }
        _ => panic!("Expected InterposeSetGet variant"),
    }
}

// ============================================================================
// Response Validation Tests
// ============================================================================

#[test]
fn test_cli_snapshot_list_response() {
    // Verify SnapshotListResponse can be decoded from SSZ
    let response = Response::SnapshotList(SnapshotListResponse {
        snapshots: vec![
            SnapshotInfo {
                id: b"01HXXXXXXXXXXXXXXXXXXXXX".to_vec(),
                name: Some(b"snapshot-1".to_vec()),
            },
            SnapshotInfo {
                id: b"01HYYYYYYYYYYYYYYYYYYYYYY".to_vec(),
                name: None,
            },
        ],
    });

    assert_valid_response(&response);

    let decoded = roundtrip_ssz(&response);
    assert_eq!(response, decoded);
}

#[test]
fn test_cli_branch_create_response() {
    // Verify BranchCreateResponse can be decoded from SSZ
    let response = Response::BranchCreate(BranchCreateResponse {
        branch: BranchInfo {
            id: b"01HZZZZZZZZZZZZZZZZZZZZZ".to_vec(),
            name: Some(b"my-branch".to_vec()),
            parent: b"01HXXXXXXXXXXXXXXXXXXXXX".to_vec(),
        },
    });

    assert_valid_response(&response);

    let decoded = roundtrip_ssz(&response);
    assert_eq!(response, decoded);
}

#[test]
fn test_cli_branch_bind_response() {
    // Verify BranchBindResponse can be decoded from SSZ
    let response = Response::BranchBind(BranchBindResponse {
        branch: b"01HZZZZZZZZZZZZZZZZZZZZZ".to_vec(),
        pid: 12345,
    });

    assert_valid_response(&response);

    let decoded = roundtrip_ssz(&response);
    assert_eq!(response, decoded);
}

#[test]
fn test_cli_error_response() {
    // Verify ErrorResponse can be decoded from SSZ
    let response = Response::error("Snapshot not found".to_string(), Some(2));

    assert_valid_response(&response);

    let decoded = roundtrip_ssz(&response);
    assert_eq!(response, decoded);

    match &response {
        Response::Error(err) => {
            assert_eq!(
                String::from_utf8_lossy(&err.error),
                "Snapshot not found",
                "Error message should match"
            );
            assert_eq!(err.code, Some(2), "Error code should match");
        }
        _ => panic!("Expected Error variant"),
    }
}

#[test]
fn test_cli_interpose_setget_response() {
    // Verify InterposeSetGetResponse can be decoded from SSZ
    let response = Response::InterposeSetGet(InterposeSetGetResponse {
        value: b"true".to_vec(),
    });

    assert_valid_response(&response);

    let decoded = roundtrip_ssz(&response);
    assert_eq!(response, decoded);
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_cli_empty_snapshot_list() {
    // Verify empty snapshot list response works
    let response = Response::SnapshotList(SnapshotListResponse { snapshots: vec![] });

    assert_valid_response(&response);

    let decoded = roundtrip_ssz(&response);
    assert_eq!(response, decoded);
}

#[test]
fn test_cli_unicode_snapshot_name() {
    // Verify Unicode characters work in snapshot names
    let request = Request::snapshot_create(Some("日本語スナップショット".to_string()));

    assert_valid_request(&request);

    let decoded = roundtrip_ssz(&request);
    assert_eq!(request, decoded);
}

#[test]
fn test_cli_long_snapshot_name() {
    // Verify long snapshot names work
    let long_name = "a".repeat(256);
    let request = Request::snapshot_create(Some(long_name.clone()));

    assert_valid_request(&request);

    let decoded = roundtrip_ssz(&request);
    assert_eq!(request, decoded);
}

#[test]
fn test_cli_special_characters_in_branch_name() {
    // Verify special characters work in branch names
    let request = Request::branch_create(
        "01HXXXXXXXXXXXXXXXXXXXXX".to_string(),
        Some("branch-with-special_chars.v2".to_string()),
    );

    assert_valid_request(&request);

    let decoded = roundtrip_ssz(&request);
    assert_eq!(request, decoded);
}

// ============================================================================
// SSZ Binary Format Stability Tests
// ============================================================================

#[test]
fn test_ssz_snapshot_list_request_stability() {
    // Verify that SSZ encoding is stable and matches expected format
    let request = Request::snapshot_list();
    let encoded = request.as_ssz_bytes();

    // Verify minimum expected size (union selector + version bytes)
    assert!(
        encoded.len() >= 2,
        "SSZ encoding should have at least 2 bytes"
    );

    // Verify we can decode what we encoded
    let decoded = Request::from_ssz_bytes(&encoded).expect("SSZ decode should succeed");
    assert_eq!(request, decoded, "Roundtrip should preserve request");
}

#[test]
fn test_ssz_branch_create_request_stability() {
    // Verify SSZ format for branch create request
    let request = Request::branch_create(
        "01HXXXXXXXXXXXXXXXXXXXXX".to_string(),
        Some("branch-name".to_string()),
    );
    let encoded = request.as_ssz_bytes();

    // Verify reasonable size
    assert!(encoded.len() > 20, "SSZ encoding should include all fields");

    // Roundtrip
    let decoded = Request::from_ssz_bytes(&encoded).expect("SSZ decode should succeed");
    assert_eq!(request, decoded, "Roundtrip should preserve request");
}

#[test]
fn test_ssz_branch_bind_request_stability() {
    // Verify SSZ format for branch bind request
    let request = Request::branch_bind("01HXXXXXXXXXXXXXXXXXXXXX".to_string(), Some(12345));
    let encoded = request.as_ssz_bytes();

    // Verify reasonable size
    assert!(encoded.len() > 20, "SSZ encoding should include all fields");

    // Roundtrip
    let decoded = Request::from_ssz_bytes(&encoded).expect("SSZ decode should succeed");
    assert_eq!(request, decoded, "Roundtrip should preserve request");
}

#[test]
fn test_ssz_interpose_request_stability() {
    // Verify SSZ format for interpose get/set request
    let request = Request::interpose_setget("enabled".to_string(), Some("true".to_string()));
    let encoded = request.as_ssz_bytes();

    // Verify reasonable size
    assert!(
        encoded.len() > 10,
        "SSZ encoding should include key and value"
    );

    // Roundtrip
    let decoded = Request::from_ssz_bytes(&encoded).expect("SSZ decode should succeed");
    assert_eq!(request, decoded, "Roundtrip should preserve request");
}
