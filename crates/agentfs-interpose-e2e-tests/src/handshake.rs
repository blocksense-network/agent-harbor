//! Handshake structures for AgentFS interpose communication

use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};

// Handshake structures copied from the shim crate
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum HandshakeMessage {
    Handshake(HandshakeData),
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct HandshakeData {
    pub version: Vec<u8>,
    pub shim: ShimInfo,
    pub process: ProcessInfo,
    pub allowlist: AllowlistInfo,
    pub timestamp: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct ShimInfo {
    pub name: Vec<u8>,
    pub crate_version: Vec<u8>,
    pub features: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub uid: u32,
    pub gid: u32,
    pub exe_path: Vec<u8>,
    pub exe_name: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct AllowlistInfo {
    pub matched_entry: Option<Vec<u8>>,
    pub configured_entries: Option<Vec<Vec<u8>>>,
}
