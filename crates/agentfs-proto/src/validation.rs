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
        | Request::DirOpen((version, _))
        | Request::DirRead((version, _))
        | Request::DirClose((version, _))
        | Request::Readlink((version, _))
        | Request::Stat((version, _))
        | Request::Lstat((version, _))
        | Request::Fstat((version, _))
        | Request::Fstatat((version, _))
        | Request::Chmod((version, _))
        | Request::Fchmod((version, _))
        | Request::Fchmodat((version, _))
        | Request::Chown((version, _))
        | Request::Lchown((version, _))
        | Request::Fchown((version, _))
        | Request::Fchownat((version, _))
        | Request::Utimes((version, _))
        | Request::Futimes((version, _))
        | Request::Utimensat((version, _))
        | Request::Futimens((version, _))
        | Request::Truncate((version, _))
        | Request::Ftruncate((version, _))
        | Request::Statfs((version, _))
        | Request::Fstatfs((version, _))
        | Request::DaemonStateProcesses(DaemonStateProcessesRequest { data: version })
        | Request::DaemonStateStats(DaemonStateStatsRequest { data: version })
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
        Request::DaemonStateFilesystem(DaemonStateFilesystemRequest { query: _ }) => {
            // No version validation for filesystem queries (testing only)
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
        | Response::DirOpen(_)
        | Response::DirRead(_)
        | Response::DirClose(_)
        | Response::Readlink(_)
        | Response::Stat(_)
        | Response::Lstat(_)
        | Response::Fstat(_)
        | Response::Fstatat(_)
        | Response::Chmod(_)
        | Response::Fchmod(_)
        | Response::Fchmodat(_)
        | Response::Chown(_)
        | Response::Lchown(_)
        | Response::Fchown(_)
        | Response::Fchownat(_)
        | Response::Utimes(_)
        | Response::Futimes(_)
        | Response::Utimensat(_)
        | Response::Futimens(_)
        | Response::Truncate(_)
        | Response::Ftruncate(_)
        | Response::Statfs(_)
        | Response::Fstatfs(_)
        | Response::DaemonState(_)
        | Response::PathOp(_)
        | Response::InterposeSetGet(_)
        | Response::Error(_) => Ok(()),
    }
}
