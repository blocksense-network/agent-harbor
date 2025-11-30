// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS harness helpers shared by the driver, tests, and scenarios.

#[cfg(all(feature = "agentfs", target_os = "linux"))]
use anyhow::{Context, Result, anyhow};
#[cfg(all(feature = "agentfs", target_os = "linux"))]
use std::path::{Path, PathBuf};
use tracing::info;

pub const ENV_TRANSPORT: &str = "AGENTFS_TRANSPORT";
const DEFAULT_TRANSPORT: &str = "interpose";

/// Requested AgentFS transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Interpose,
    Fuse,
}

pub fn requested_transport() -> Transport {
    let value = std::env::var(ENV_TRANSPORT)
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|_| DEFAULT_TRANSPORT.to_string());

    match value.as_str() {
        "fuse" => Transport::Fuse,
        "interpose" => Transport::Interpose,
        other => {
            info!(
                requested = other,
                "unknown AGENTFS_TRANSPORT value, defaulting to platform"
            );
            default_transport_for_platform()
        }
    }
}

fn default_transport_for_platform() -> Transport {
    #[cfg(target_os = "linux")]
    {
        Transport::Fuse
    }
    #[cfg(not(target_os = "linux"))]
    {
        Transport::Interpose
    }
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
use ah_fs_snapshots_daemon::client::{
    AgentfsFuseBackstore, AgentfsFuseMountRequest, AgentfsFuseState, AgentfsFuseStatusData,
    AgentfsHostFsBackstore, AgentfsRamDiskBackstore, DEFAULT_SOCKET_PATH, DaemonClient,
};
#[cfg(all(feature = "agentfs", target_os = "linux"))]
use uuid::Uuid;

#[cfg(all(feature = "agentfs", target_os = "linux"))]
const ENV_SOCKET_PATH: &str = "AH_FS_SNAPSHOTS_DAEMON_SOCKET";
#[cfg(all(feature = "agentfs", target_os = "linux"))]
const ENV_BACKSTORE_MATRIX: &str = "AGENTFS_BACKSTORE_MATRIX";
#[cfg(all(feature = "agentfs", target_os = "linux"))]
const ENV_HOSTFS_ROOT: &str = "AGENTFS_HOSTFS_ROOT";
#[cfg(all(feature = "agentfs", target_os = "linux"))]
const ENV_RAMDISK_SIZE_MB: &str = "AGENTFS_RAMDISK_SIZE_MB";
#[cfg(all(feature = "agentfs", target_os = "linux"))]
const ENV_REPO_PARENT: &str = "AGENTFS_FUSE_REPO_ROOT";
#[cfg(all(feature = "agentfs", target_os = "linux"))]
const ENV_MOUNT_POINT: &str = "AGENTFS_FUSE_MOUNT_POINT";

#[cfg(all(feature = "agentfs", target_os = "linux"))]
const DEFAULT_RAMDISK_MB: u32 = 1024;

#[cfg(all(feature = "agentfs", target_os = "linux"))]
#[derive(Debug, Clone)]
pub enum BackstoreSpec {
    InMemory,
    HostFs { root: PathBuf, prefer_native: bool },
    RamDisk { size_mb: u32 },
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
impl BackstoreSpec {
    pub fn label(&self) -> &'static str {
        match self {
            BackstoreSpec::InMemory => "inmemory",
            BackstoreSpec::HostFs { .. } => "hostfs",
            BackstoreSpec::RamDisk { .. } => "ramdisk",
        }
    }

    fn to_proto(&self) -> Result<AgentfsFuseBackstore> {
        Ok(match self {
            BackstoreSpec::InMemory => AgentfsFuseBackstore::InMemory(Vec::new()),
            BackstoreSpec::HostFs {
                root,
                prefer_native,
            } => AgentfsFuseBackstore::HostFs(AgentfsHostFsBackstore {
                root: root
                    .to_str()
                    .ok_or_else(|| anyhow!("hostfs root must be valid UTF-8"))?
                    .as_bytes()
                    .to_vec(),
                prefer_native_snapshots: *prefer_native,
            }),
            BackstoreSpec::RamDisk { size_mb } => {
                AgentfsFuseBackstore::RamDisk(AgentfsRamDiskBackstore { size_mb: *size_mb })
            }
        })
    }
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
pub fn parse_backstore_matrix() -> Vec<BackstoreSpec> {
    let raw =
        std::env::var(ENV_BACKSTORE_MATRIX).unwrap_or_else(|_| "inmemory,hostfs,ramdisk".into());
    raw.split(',')
        .filter_map(|entry| {
            let trimmed = entry.trim().to_lowercase();
            if trimmed.is_empty() {
                return None;
            }
            Some(match trimmed.as_str() {
                "inmemory" | "in-memory" => BackstoreSpec::InMemory,
                "hostfs" | "host" => BackstoreSpec::HostFs {
                    root: hostfs_root_path(),
                    prefer_native: false,
                },
                "ramdisk" | "ram" => BackstoreSpec::RamDisk {
                    size_mb: ramdisk_size(),
                },
                other => {
                    info!(
                        backstore = other,
                        "unknown AgentFS backstore entry, skipping"
                    );
                    return None;
                }
            })
        })
        .collect()
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
fn hostfs_root_path() -> PathBuf {
    std::env::var_os(ENV_HOSTFS_ROOT)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("agentfs-hostfs"))
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
fn ramdisk_size() -> u32 {
    std::env::var(ENV_RAMDISK_SIZE_MB)
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|size| *size > 0)
        .unwrap_or(DEFAULT_RAMDISK_MB)
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
pub struct FuseHarness {
    client: DaemonClient,
    socket_path: PathBuf,
    mount_point: PathBuf,
    repo_root: PathBuf,
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
impl FuseHarness {
    pub fn new() -> Result<Self> {
        let socket_path = std::env::var_os(ENV_SOCKET_PATH)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
        let mount_point = std::env::var_os(ENV_MOUNT_POINT)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp/agentfs"));
        let repo_root = std::env::var_os(ENV_REPO_PARENT)
            .map(PathBuf::from)
            .unwrap_or_else(|| mount_point.join("fs-snapshots"));
        Ok(Self {
            client: DaemonClient::with_socket_path(&socket_path),
            socket_path,
            mount_point,
            repo_root,
        })
    }

    pub fn mount_point(&self) -> &Path {
        &self.mount_point
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn ensure_mounted(&self, spec: &BackstoreSpec) -> Result<AgentfsFuseStatusData> {
        if self.client.socket_exists() {
            if let Ok(status) = self.client.status_agentfs_fuse() {
                if AgentfsFuseState::from_code(status.state) == AgentfsFuseState::Running {
                    info!("Unmounting existing AgentFS FUSE mount before reconfiguration");
                    let _ = self.client.unmount_agentfs_fuse();
                }
            }
        }

        let request = AgentfsFuseMountRequest {
            mount_point: self
                .mount_point
                .to_str()
                .ok_or_else(|| anyhow!("mount point must be valid UTF-8"))?
                .as_bytes()
                .to_vec(),
            uid: unsafe { libc::geteuid() },
            gid: unsafe { libc::getegid() },
            allow_other: true,
            allow_root: false,
            auto_unmount: true,
            writeback_cache: false,
            mount_timeout_ms: 15_000,
            backstore: spec.to_proto()?,
            materialization_mode: ah_fs_snapshots_daemon::types::AgentfsMaterializationMode::lazy(),
        };

        if !self.mount_point.exists() {
            create_dir_resilient(&self.mount_point).with_context(|| {
                format!(
                    "failed to create mount point {}",
                    self.mount_point.display()
                )
            })?;
        }

        if let BackstoreSpec::HostFs { root, .. } = spec {
            if !root.exists() {
                create_dir_resilient(root)
                    .with_context(|| format!("failed to create hostfs root {}", root.display()))?;
            }
        }

        let status = self.client.mount_agentfs_fuse(request).with_context(|| {
            format!(
                "failed to mount AgentFS FUSE via {}",
                self.socket_path.display()
            )
        })?;
        Ok(status)
    }

    pub fn prepare_repo(&self, label: &str) -> Result<FuseRepoGuard> {
        if !self.repo_root.exists() {
            create_dir_resilient(&self.repo_root).with_context(|| {
                format!(
                    "failed to create AgentFS repo root {}",
                    self.repo_root.display()
                )
            })?;
        }
        let slug = label.replace(' ', "-");
        let dir = self.repo_root.join(format!("{}-{}", slug, Uuid::new_v4()));
        create_dir_resilient(&dir)
            .with_context(|| format!("failed to create AgentFS workspace {}", dir.display()))?;
        Ok(FuseRepoGuard {
            path: dir,
            mount_point: self.mount_point.clone(),
        })
    }
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
fn create_dir_resilient(path: &Path) -> Result<()> {
    match std::fs::create_dir_all(path) {
        Ok(_) => Ok(()),
        Err(err) => Err(anyhow!(err)),
    }
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
pub struct FuseRepoGuard {
    path: PathBuf,
    mount_point: PathBuf,
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
impl FuseRepoGuard {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(all(feature = "agentfs", target_os = "linux"))]
impl Drop for FuseRepoGuard {
    fn drop(&mut self) {
        if self.path.starts_with(&self.mount_point) {
            if let Err(err) = std::fs::remove_dir_all(&self.path) {
                tracing::warn!(path = %self.path.display(), error = %err, "failed to cleanup AgentFS repo");
            }
        }
    }
}
