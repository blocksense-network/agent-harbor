// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Schema validation for AgentFS control messages

use crate::messages::*;
use thiserror::Error;

/// Validation error
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("schema validation failed: {0}")]
    Schema(String),
    #[error("SSZ decoding failed: {0}")]
    SszDecode(String),
}

/// Validate a decoded request against its logical schema
pub fn validate_request(request: &Request) -> Result<(), ValidationError> {
    match request {
        Request::SnapshotCreate((version, _))
        | Request::BranchCreate((version, _))
        | Request::BranchBind((version, _))
        | Request::FdOpen((version, _))
        | Request::FdDup((version, _))
        | Request::PathOp((version, _))
        | Request::InterposeSetGet((version, _)) => {
            if version != b"1" {
                return Err(ValidationError::Schema("version must be '1'".to_string()));
            }
            Ok(())
        }
        Request::SnapshotList(version) => {
            if version != b"1" {
                return Err(ValidationError::Schema("version must be '1'".to_string()));
            }
            Ok(())
        }
    }
}

/// Validate a decoded response against its logical schema
pub fn validate_response(response: &Response) -> Result<(), ValidationError> {
    // For union responses, the structure is validated by the SSZ decoding itself
    // Error responses are always valid, success responses have their structure enforced by the union
    match response {
        Response::SnapshotCreate(_)
        | Response::SnapshotList(_)
        | Response::BranchCreate(_)
        | Response::BranchBind(_)
        | Response::FdOpen(_)
        | Response::FdDup(_)
        | Response::PathOp(_)
        | Response::InterposeSetGet(_)
        | Response::Error(_) => Ok(()),
    }
}
