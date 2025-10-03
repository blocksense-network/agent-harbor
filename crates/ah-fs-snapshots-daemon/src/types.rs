use ssz_derive::{Decode, Encode};

// SSZ Union-based request/response types for type-safe daemon communication
// Using Vec<u8> for strings as SSZ supports variable-length byte vectors

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum Request {
    Ping(Vec<u8>),                     // empty vec for ping
    CloneZfs((Vec<u8>, Vec<u8>)),      // (snapshot, clone)
    SnapshotZfs((Vec<u8>, Vec<u8>)),   // (source, snapshot)
    DeleteZfs(Vec<u8>),                // target
    CloneBtrfs((Vec<u8>, Vec<u8>)),    // (source, destination)
    SnapshotBtrfs((Vec<u8>, Vec<u8>)), // (source, destination)
    DeleteBtrfs(Vec<u8>),              // target
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "union")]
pub enum Response {
    Success(Vec<u8>),               // empty vec for success
    SuccessWithMountpoint(Vec<u8>), // mountpoint
    SuccessWithPath(Vec<u8>),       // path
    Error(Vec<u8>),                 // message
}

// Constructors for SSZ union variants (convert String to Vec<u8>)
#[allow(dead_code)]
impl Request {
    pub fn ping() -> Self {
        Self::Ping(vec![])
    }

    pub fn clone_zfs(snapshot: String, clone: String) -> Self {
        Self::CloneZfs((snapshot.into_bytes(), clone.into_bytes()))
    }

    pub fn snapshot_zfs(source: String, snapshot: String) -> Self {
        Self::SnapshotZfs((source.into_bytes(), snapshot.into_bytes()))
    }

    pub fn delete_zfs(target: String) -> Self {
        Self::DeleteZfs(target.into_bytes())
    }

    pub fn clone_btrfs(source: String, destination: String) -> Self {
        Self::CloneBtrfs((source.into_bytes(), destination.into_bytes()))
    }

    pub fn snapshot_btrfs(source: String, destination: String) -> Self {
        Self::SnapshotBtrfs((source.into_bytes(), destination.into_bytes()))
    }

    pub fn delete_btrfs(target: String) -> Self {
        Self::DeleteBtrfs(target.into_bytes())
    }
}

impl Response {
    pub fn success() -> Self {
        Self::Success(vec![])
    }

    pub fn success_with_mountpoint(mountpoint: String) -> Self {
        Self::SuccessWithMountpoint(mountpoint.into_bytes())
    }

    pub fn success_with_path(path: String) -> Self {
        Self::SuccessWithPath(path.into_bytes())
    }

    pub fn error(message: String) -> Self {
        Self::Error(message.into_bytes())
    }
}
