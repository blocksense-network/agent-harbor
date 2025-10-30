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
        | Request::Rename((version, _))
        | Request::Renameat((version, _))
        | Request::RenameatxNp((version, _))
        | Request::Link((version, _))
        | Request::Linkat((version, _))
        | Request::Symlink((version, _))
        | Request::Symlinkat((version, _))
        | Request::Unlink((version, _))
        | Request::Unlinkat((version, _))
        | Request::Remove((version, _))
        | Request::Mkdir((version, _))
        | Request::Mkdirat((version, _))
        | Request::Getxattr((version, _))
        | Request::Lgetxattr((version, _))
        | Request::Fgetxattr((version, _))
        | Request::Setxattr((version, _))
        | Request::Lsetxattr((version, _))
        | Request::Fsetxattr((version, _))
        | Request::Listxattr((version, _))
        | Request::Llistxattr((version, _))
        | Request::Flistxattr((version, _))
        | Request::Removexattr((version, _))
        | Request::Lremovexattr((version, _))
        | Request::Fremovexattr((version, _))
        | Request::AclGetFile((version, _))
        | Request::AclSetFile((version, _))
        | Request::AclGetFd((version, _))
        | Request::AclSetFd((version, _))
        | Request::AclDeleteDefFile((version, _))
        | Request::Chflags((version, _))
        | Request::Lchflags((version, _))
        | Request::Fchflags((version, _))
        | Request::Getattrlist((version, _))
        | Request::Setattrlist((version, _))
        | Request::Getattrlistbulk((version, _))
        | Request::Copyfile((version, _))
        | Request::Fcopyfile((version, _))
        | Request::Clonefile((version, _))
        | Request::Fclonefileat((version, _))
        | Request::DaemonStateProcesses(DaemonStateProcessesRequest { data: version })
        | Request::DaemonStateStats(DaemonStateStatsRequest { data: version })
        | Request::PathOp((version, _))
        | Request::InterposeSetGet((version, _))
        | Request::DirfdOpenDir((version, _))
        | Request::DirfdCloseFd((version, _))
        | Request::DirfdDupFd((version, _))
        | Request::DirfdSetCwd((version, _))
        | Request::DirfdResolvePath((version, _))
        | Request::FSEventsTranslatePaths((version, _))
        | Request::WatchRegisterKqueue((version, _))
        | Request::WatchRegisterFSEvents((version, _))
        | Request::WatchUnregister((version, _))
        | Request::WatchDoorbell((version, _))
        | Request::FsEventBroadcast((version, _)) => {
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
        | Response::Rename(_)
        | Response::Renameat(_)
        | Response::RenameatxNp(_)
        | Response::Link(_)
        | Response::Linkat(_)
        | Response::Symlink(_)
        | Response::Symlinkat(_)
        | Response::Unlink(_)
        | Response::Unlinkat(_)
        | Response::Remove(_)
        | Response::Mkdir(_)
        | Response::Mkdirat(_)
        | Response::Getxattr(_)
        | Response::Lgetxattr(_)
        | Response::Fgetxattr(_)
        | Response::Setxattr(_)
        | Response::Lsetxattr(_)
        | Response::Fsetxattr(_)
        | Response::Listxattr(_)
        | Response::Llistxattr(_)
        | Response::Flistxattr(_)
        | Response::Removexattr(_)
        | Response::Lremovexattr(_)
        | Response::Fremovexattr(_)
        | Response::AclGetFile(_)
        | Response::AclSetFile(_)
        | Response::AclGetFd(_)
        | Response::AclSetFd(_)
        | Response::AclDeleteDefFile(_)
        | Response::Chflags(_)
        | Response::Lchflags(_)
        | Response::Fchflags(_)
        | Response::Getattrlist(_)
        | Response::Setattrlist(_)
        | Response::Getattrlistbulk(_)
        | Response::Copyfile(_)
        | Response::Fcopyfile(_)
        | Response::Clonefile(_)
        | Response::Fclonefileat(_)
        | Response::DaemonState(_)
        | Response::PathOp(_)
        | Response::InterposeSetGet(_)
        | Response::DirfdOpenDir(_)
        | Response::DirfdCloseFd(_)
        | Response::DirfdDupFd(_)
        | Response::DirfdSetCwd(_)
        | Response::DirfdResolvePath(_)
        | Response::FSEventsTranslatePaths(_)
        | Response::WatchRegisterKqueue(_)
        | Response::WatchRegisterFSEvents(_)
        | Response::WatchUnregister(_)
        | Response::WatchDoorbell(_)
        | Response::FsEventBroadcast(_)
        | Response::Error(_) => Ok(()),
    }
}
