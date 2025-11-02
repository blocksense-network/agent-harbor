// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use agentfs_proto::*;
use ssz::{Decode, Encode};

/// Helper function to encode SSZ messages for testing
fn encode_ssz_message(data: &impl Encode) -> Vec<u8> {
    data.as_ssz_bytes()
}

/// Helper function to decode SSZ messages for testing
fn decode_ssz_message<T: Decode>(data: &[u8]) -> Result<T, ssz::DecodeError> {
    T::from_ssz_bytes(data)
}

#[ah_test_utils::logged_test]
fn test_valid_snapshot_create_request() {
    let request = Request::snapshot_create(Some("test-snapshot".to_string()));

    assert!(validate_request(&request).is_ok());
}

#[ah_test_utils::logged_test]
fn test_valid_snapshot_create_request_no_name() {
    let request = Request::snapshot_create(None);

    assert!(validate_request(&request).is_ok());
}

#[ah_test_utils::logged_test]
fn test_valid_snapshot_list_request() {
    let request = Request::snapshot_list();

    assert!(validate_request(&request).is_ok());
}

#[ah_test_utils::logged_test]
fn test_valid_branch_create_request() {
    let request = Request::branch_create(
        "01HXXXXXXXXXXXXXXXXXXXXX".to_string(),
        Some("test-branch".to_string()),
    );

    assert!(validate_request(&request).is_ok());
}

#[ah_test_utils::logged_test]
fn test_valid_branch_bind_request() {
    let request = Request::branch_bind("01HXXXXXXXXXXXXXXXXXXXXX".to_string(), Some(1234));

    assert!(validate_request(&request).is_ok());
}

#[ah_test_utils::logged_test]
fn test_invalid_version() {
    // Create a request with invalid version by manually constructing it
    let request = Request::SnapshotList(b"2".to_vec());

    assert!(validate_request(&request).is_err());
}

#[ah_test_utils::logged_test]
fn test_unknown_operation() {
    // The union types don't allow unknown operations - they're enforced at compile time
    // So this test doesn't apply to the new design
}

#[ah_test_utils::logged_test]
fn test_valid_success_response() {
    let response = Response::snapshot_create(SnapshotInfo {
        id: b"01HXXXXXXXXXXXXXXXXXXXXX".to_vec(),
        name: Some(b"test-snapshot".to_vec()),
    });

    assert!(validate_response(&response).is_ok());
}

#[ah_test_utils::logged_test]
fn test_valid_error_response() {
    let response = Response::error("Snapshot not found".to_string(), Some(2));

    assert!(validate_response(&response).is_ok());
}
