// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Scenario helpers that exercise snapshot providers exactly as our legacy
//! integration tests did. These routines are used both by the external harness
//! driver binary and by Rust tests so we keep coverage identical across both
//! code paths.

use ah_fs_snapshots_traits::{FsSnapshotProvider, WorkingCopyMode};
use anyhow::{Context, Result, anyhow, bail, ensure};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(all(unix, any(feature = "btrfs", feature = "zfs")))]
use std::ffi::CString;
#[cfg(all(unix, any(feature = "btrfs", feature = "zfs")))]
use std::os::unix::ffi::OsStrExt;

#[cfg(any(feature = "btrfs", feature = "zfs"))]
use libc::{EDQUOT, ENOSPC};

#[cfg(any(feature = "btrfs", feature = "zfs"))]
use std::process::Command;

#[cfg(feature = "git")]
use ah_fs_snapshots_git::GitProvider;

#[cfg(feature = "git")]
use ah_repo::test_helpers::{git_available, initialize_git_repo};

#[cfg(feature = "zfs")]
use crate::{ZfsHarnessEnvironment, zfs_available, zfs_is_root};

#[cfg(feature = "zfs")]
use ah_fs_snapshots_traits::SnapshotProviderKind;

#[cfg(feature = "zfs")]
use ah_fs_snapshots_zfs::ZfsProvider;

#[cfg(feature = "btrfs")]
use crate::{BtrfsHarnessEnvironment, btrfs_available, btrfs_is_root};

#[cfg(feature = "btrfs")]
use ah_fs_snapshots_btrfs::BtrfsProvider;

#[cfg(feature = "agentfs")]
use ah_fs_snapshots::AgentFsProvider;

struct MatrixOutcome {
    creation_time: Duration,
    cleanup_time: Duration,
    repo_path: PathBuf,
}

pub fn provider_matrix(provider: &str) -> Result<()> {
    match provider {
        "git" => {
            #[cfg(feature = "git")]
            {
                let outcome = provider_matrix_git()?;
                verify_performance(
                    "Git",
                    &outcome,
                    Duration::from_secs_f32(2.0),
                    Duration::from_secs_f32(2.0),
                )?;
                run_concurrency_test(
                    "Git",
                    outcome.repo_path.as_path(),
                    WorkingCopyMode::Worktree,
                    1,
                    GitProvider::new,
                )?;
                run_error_handling_test("Git", WorkingCopyMode::Worktree, GitProvider::new)?;
                Ok(())
            }
            #[cfg(not(feature = "git"))]
            {
                tracing::info!("Skipping Git provider matrix: git feature disabled");
                Ok(())
            }
        }
        #[cfg(feature = "btrfs")]
        "btrfs" => {
            if !btrfs_is_root() {
                tracing::info!("Skipping Btrfs provider matrix: requires root privileges");
                return Ok(());
            }

            if !btrfs_available() {
                tracing::info!("Skipping Btrfs provider matrix: Btrfs tooling not available");
                return Ok(());
            }

            let mut env = BtrfsHarnessEnvironment::new()
                .context("failed to create Btrfs harness environment")?;
            let repo_path = env
                .create_btrfs_test_subvolume("btrfs_matrix", 256)
                .context("failed to create Btrfs test subvolume")?;

            populate_test_repo(&repo_path)?;

            let provider = BtrfsProvider::new();
            let outcome = run_matrix_common(
                "Btrfs",
                &provider,
                &repo_path,
                WorkingCopyMode::CowOverlay,
                Some(WorkingCopyMode::CowOverlay),
            )?;
            verify_performance(
                "Btrfs",
                &outcome,
                Duration::from_secs_f32(3.0),
                Duration::from_secs_f32(2.0),
            )?;
            run_concurrency_test(
                "Btrfs",
                outcome.repo_path.as_path(),
                WorkingCopyMode::CowOverlay,
                3,
                BtrfsProvider::new,
            )?;
            run_space_efficiency_test(
                "Btrfs",
                outcome.repo_path.as_path(),
                WorkingCopyMode::CowOverlay,
                512 * 1024,
                BtrfsProvider::new,
            )?;
            run_btrfs_quota_test(outcome.repo_path.as_path(), WorkingCopyMode::CowOverlay)?;
            run_error_handling_test("Btrfs", WorkingCopyMode::CowOverlay, BtrfsProvider::new)?;
            Ok(())
        }
        #[cfg(not(feature = "btrfs"))]
        "btrfs" => {
            tracing::info!("Skipping Btrfs provider matrix: btrfs feature disabled");
            Ok(())
        }
        #[cfg(feature = "zfs")]
        "zfs" => {
            if !zfs_is_root() {
                tracing::info!("Skipping ZFS provider matrix: requires root privileges");
                return Ok(());
            }

            if !zfs_available() {
                tracing::info!("Skipping ZFS provider matrix: ZFS tooling not available");
                return Ok(());
            }

            let mut env =
                ZfsHarnessEnvironment::new().context("failed to create ZFS harness environment")?;
            let mount_point = match env.create_zfs_test_pool("matrix_zfs_pool", 200) {
                Ok(path) => path,
                Err(err) => {
                    tracing::info!(error = ?err, "Skipping ZFS provider matrix: unable to create test pool");
                    return Ok(());
                }
            };

            populate_test_repo(&mount_point)?;

            let provider = ZfsProvider::new();
            let outcome = run_matrix_common(
                "ZFS",
                &provider,
                &mount_point,
                WorkingCopyMode::Worktree,
                Some(WorkingCopyMode::Worktree),
            )?;
            verify_performance(
                "ZFS",
                &outcome,
                Duration::from_secs_f32(5.0),
                Duration::from_secs_f32(3.0),
            )?;
            run_concurrency_test(
                "ZFS",
                outcome.repo_path.as_path(),
                WorkingCopyMode::Worktree,
                4,
                ZfsProvider::new,
            )?;
            run_space_efficiency_test(
                "ZFS",
                outcome.repo_path.as_path(),
                WorkingCopyMode::Worktree,
                1024 * 1024,
                ZfsProvider::new,
            )?;
            run_zfs_quota_test(outcome.repo_path.as_path(), WorkingCopyMode::Worktree)?;
            run_error_handling_test("ZFS", WorkingCopyMode::Worktree, ZfsProvider::new)?;
            Ok(())
        }
        #[cfg(not(feature = "zfs"))]
        "zfs" => {
            tracing::info!("Skipping ZFS provider matrix: zfs feature disabled");
            Ok(())
        }
        #[cfg(feature = "agentfs")]
        "agentfs" => {
            #[cfg(not(target_os = "macos"))]
            {
                tracing::info!("AgentFS provider matrix is only supported on macOS");
                return Ok(());
            }

            #[cfg(target_os = "macos")]
            {
                let temp_dir =
                    tempfile::tempdir().context("failed to create AgentFS matrix repository")?;
                populate_test_repo(temp_dir.path())?;

                let provider = AgentFsProvider::new();
                let outcome = run_matrix_common(
                    "AgentFS",
                    &provider,
                    temp_dir.path(),
                    WorkingCopyMode::CowOverlay,
                    Some(WorkingCopyMode::CowOverlay),
                )?;
                verify_performance(
                    "AgentFS",
                    &outcome,
                    Duration::from_secs_f32(5.0),
                    Duration::from_secs_f32(5.0),
                )?;
                run_concurrency_test(
                    "AgentFS",
                    outcome.repo_path.as_path(),
                    WorkingCopyMode::CowOverlay,
                    3,
                    AgentFsProvider::new,
                )?;
                run_error_handling_test(
                    "AgentFS",
                    WorkingCopyMode::CowOverlay,
                    AgentFsProvider::new,
                )?;
                Ok(())
            }
        }
        #[cfg(not(feature = "agentfs"))]
        "agentfs" => {
            tracing::info!("Skipping AgentFS provider matrix: agentfs feature disabled");
            Ok(())
        }
        other => bail!("unsupported provider '{}' for matrix run", other),
    }
}

#[cfg(feature = "git")]
fn provider_matrix_git() -> Result<MatrixOutcome> {
    if !git_available() {
        bail!("git command not available on PATH");
    }

    ensure_git_identity_env();

    let temp_dir = tempfile::tempdir().context("failed to create matrix repository directory")?;
    populate_test_repo(temp_dir.path())?;
    initialize_git_repo(temp_dir.path())
        .map_err(|err| anyhow::anyhow!("failed to initialise git repository: {err}"))?;

    let provider = GitProvider::new();
    run_matrix_common(
        "Git",
        &provider,
        temp_dir.path(),
        WorkingCopyMode::Worktree,
        Some(WorkingCopyMode::Worktree),
    )
}

fn run_matrix_common<P>(
    provider_name: &str,
    provider: &P,
    repo_path: &Path,
    workspace_mode: WorkingCopyMode,
    branch_mode: Option<WorkingCopyMode>,
) -> Result<MatrixOutcome>
where
    P: FsSnapshotProvider,
{
    let capabilities = provider.detect_capabilities(repo_path);
    tracing::info!(
        "{} matrix capabilities: kind={:?}, score={}, supports_cow={}",
        provider_name,
        capabilities.kind,
        capabilities.score,
        capabilities.supports_cow_overlay
    );
    ensure!(
        capabilities.score > 0,
        "{} provider reported zero capability score for repo {}",
        provider_name,
        repo_path.display()
    );

    let creation_start = Instant::now();
    let workspace = provider
        .prepare_writable_workspace(repo_path, workspace_mode)
        .with_context(|| format!("failed to prepare {} writable workspace", provider_name))?;
    let creation_time = creation_start.elapsed();
    tracing::info!(
        "{} matrix workspace: {}",
        provider_name,
        workspace.exec_path.display()
    );
    if std::env::var("FS_SNAPSHOTS_HARNESS_DEBUG").is_ok() {
        tracing::info!(
            "{} matrix env AGENTFS_INTERPOSE_SOCKET={:?}",
            provider_name,
            std::env::var_os("AGENTFS_INTERPOSE_SOCKET")
        );
    }
    ensure!(
        workspace.exec_path.exists(),
        "{} workspace path should exist",
        provider_name
    );

    ensure!(
        workspace.exec_path.join("README.md").exists(),
        "{} workspace missing README.md",
        provider_name
    );

    let marker_name = format!("{}_matrix_marker.txt", provider_name.to_lowercase());
    let marker_path = workspace.exec_path.join(&marker_name);
    fs::write(
        &marker_path,
        format!("Matrix marker generated by {}", provider_name),
    )
    .context("failed to write matrix marker into workspace")?;
    ensure!(
        marker_path.exists(),
        "matrix marker not created inside workspace"
    );
    if provider_name != "AgentFS" {
        ensure!(
            !repo_path.join(&marker_name).exists(),
            "matrix marker should not appear in the base repository"
        );
    } else {
        tracing::info!(
            "AgentFS base repo marker present? {}",
            repo_path.join(&marker_name).exists()
        );
    }

    let snapshot = provider
        .snapshot_now(&workspace, Some("matrix-snapshot"))
        .with_context(|| format!("failed to create {} matrix snapshot", provider_name))?;
    tracing::info!("{} matrix snapshot created: {}", provider_name, snapshot.id);

    let mut readonly_export: Option<PathBuf> = None;
    match provider.mount_readonly(&snapshot) {
        Ok(readonly_path) => {
            tracing::info!(
                "{} matrix readonly mount: {}",
                provider_name,
                readonly_path.display()
            );
            ensure!(
                readonly_path.join(&marker_name).exists(),
                "matrix readonly mount missing workspace marker"
            );
            readonly_export = Some(readonly_path);
        }
        Err(err) => {
            tracing::info!(
                "{} matrix readonly mount unavailable: {}",
                provider_name,
                err
            );
        }
    }
    tracing::info!(
        "{} matrix recorded readonly export: {}",
        provider_name,
        readonly_export.is_some()
    );

    if let Some(mode) = branch_mode {
        match provider.branch_from_snapshot(&snapshot, mode) {
            Ok(branch_ws) => {
                tracing::info!(
                    "{} matrix branch workspace: {}",
                    provider_name,
                    branch_ws.exec_path.display()
                );
                let branch_cleanup = branch_ws.cleanup_token.clone();
                let branch_result = (|| -> Result<()> {
                    ensure!(
                        branch_ws.exec_path.join(&marker_name).exists(),
                        "matrix branch workspace missing workspace marker"
                    );
                    Ok(())
                })();

                provider.cleanup(&branch_cleanup).with_context(|| {
                    format!(
                        "failed to cleanup {} matrix branch workspace",
                        provider_name
                    )
                })?;

                branch_result?;
            }
            Err(err) => {
                tracing::info!(
                    "{} matrix branch creation unavailable: {}",
                    provider_name,
                    err
                );
            }
        }
    } else {
        tracing::info!(
            "{} matrix branch step skipped because branch mode is not supported",
            provider_name
        );
    }

    let workspace_cleanup = workspace.cleanup_token.clone();
    let cleanup_start = Instant::now();
    provider.cleanup(&workspace_cleanup).with_context(|| {
        format!(
            "failed to cleanup {} matrix writable workspace",
            provider_name
        )
    })?;
    let cleanup_time = cleanup_start.elapsed();
    if let Some(readonly_path) = readonly_export {
        if readonly_path.exists() {
            ensure!(
                is_directory_empty(&readonly_path)?,
                "{} readonly export at {} should be removed or empty after cleanup",
                provider_name,
                readonly_path.display()
            );
            tracing::info!(
                "{} matrix readonly export cleaned: {}",
                provider_name,
                readonly_path.display()
            );
        } else {
            tracing::info!(
                "{} matrix readonly export removed: {}",
                provider_name,
                readonly_path.display()
            );
        }
    } else {
        tracing::info!(
            "{} matrix readonly export not produced; skipping cleanup assertion",
            provider_name
        );
    }
    tracing::info!("{} provider matrix completed successfully", provider_name);
    Ok(MatrixOutcome {
        creation_time,
        cleanup_time,
        repo_path: repo_path.to_path_buf(),
    })
}

fn is_directory_empty(path: &Path) -> Result<bool> {
    match fs::read_dir(path) {
        Ok(mut entries) => Ok(entries.next().is_none()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(err) => Err(err.into()),
    }
}

fn verify_performance(
    provider_name: &str,
    outcome: &MatrixOutcome,
    max_creation: Duration,
    max_cleanup: Duration,
) -> Result<()> {
    tracing::info!(
        "{} performance metrics: creation {:?}, cleanup {:?}",
        provider_name,
        outcome.creation_time,
        outcome.cleanup_time
    );
    ensure!(
        outcome.creation_time <= max_creation,
        "{} workspace creation exceeded budget {:?} (observed {:?})",
        provider_name,
        max_creation,
        outcome.creation_time
    );
    ensure!(
        outcome.cleanup_time <= max_cleanup,
        "{} workspace cleanup exceeded budget {:?} (observed {:?})",
        provider_name,
        max_cleanup,
        outcome.cleanup_time
    );
    Ok(())
}

fn run_concurrency_test<P, F>(
    provider_name: &str,
    repo_path: &Path,
    mode: WorkingCopyMode,
    concurrency: usize,
    factory: F,
) -> Result<()>
where
    P: FsSnapshotProvider + Send + 'static,
    F: Fn() -> P + Send + Sync + Clone + 'static,
{
    if concurrency <= 1 {
        return Ok(());
    }
    tracing::info!(
        "{} concurrency test: launching {} workers",
        provider_name,
        concurrency
    );

    let mut handles = Vec::with_capacity(concurrency);
    for index in 0..concurrency {
        let repo = repo_path.to_path_buf();
        let factory = factory.clone();
        handles.push(thread::spawn(move || -> Result<()> {
            let provider = factory();
            let workspace = provider
                .prepare_writable_workspace(&repo, mode)
                .context("concurrency worker failed to prepare workspace")?;
            let thread_file = workspace.exec_path.join(format!("thread_{index}.txt"));
            fs::write(&thread_file, format!("content from worker {index}"))
                .context("failed to write thread file inside workspace")?;
            ensure!(
                workspace.exec_path.join("README.md").exists(),
                "workspace missing README after concurrency preparation"
            );
            provider
                .cleanup(&workspace.cleanup_token)
                .context("concurrency worker failed to cleanup workspace")?;
            Ok(())
        }));
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow!("{} concurrency worker panicked", provider_name))??;
    }
    tracing::info!("{} concurrency test completed", provider_name);
    Ok(())
}

#[cfg(any(feature = "btrfs", feature = "zfs"))]
fn run_space_efficiency_test<P, F>(
    provider_name: &str,
    repo_path: &Path,
    mode: WorkingCopyMode,
    max_delta_bytes: u64,
    factory: F,
) -> Result<()>
where
    P: FsSnapshotProvider,
    F: Fn() -> P,
{
    tracing::info!(
        "{} space efficiency test: max additional usage {} bytes",
        provider_name,
        max_delta_bytes
    );

    let baseline = filesystem_used_bytes(repo_path)
        .with_context(|| format!("failed to measure baseline usage for {}", provider_name))?;

    let provider = factory();
    let workspace = provider.prepare_writable_workspace(repo_path, mode).with_context(|| {
        format!(
            "{} failed to prepare workspace for space test",
            provider_name
        )
    })?;

    let after = filesystem_used_bytes(repo_path).with_context(|| {
        format!(
            "failed to measure post-creation usage for {}",
            provider_name
        )
    })?;
    let delta = after.saturating_sub(baseline);
    tracing::info!(
        "{} space efficiency delta: {} bytes (baseline {}, after {})",
        provider_name,
        delta,
        baseline,
        after
    );

    let cleanup_token = workspace.cleanup_token.clone();
    provider
        .cleanup(&cleanup_token)
        .with_context(|| format!("{} failed to cleanup space test workspace", provider_name))?;

    ensure!(
        delta <= max_delta_bytes,
        "{} workspace inflated backing store by {} bytes (budget {})",
        provider_name,
        delta,
        max_delta_bytes
    );

    Ok(())
}

#[cfg(any(feature = "btrfs", feature = "zfs"))]
fn filesystem_used_bytes(path: &Path) -> Result<u64> {
    #[cfg(unix)]
    {
        let c_path = CString::new(path.as_os_str().as_bytes())
            .context("failed to convert path to C string")?;

        let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::statfs(c_path.as_ptr(), &mut stat) };
        if rc != 0 {
            return Err(io::Error::last_os_error().into());
        }

        let block_size = stat.f_bsize as u128;
        let used_blocks = (stat.f_blocks - stat.f_bfree) as u128;
        let bytes_used = block_size * used_blocks;
        Ok(bytes_used as u64)
    }

    #[cfg(not(unix))]
    {
        anyhow::bail!("filesystem usage measurement not implemented on this platform");
    }
}

#[cfg(feature = "btrfs")]
fn run_btrfs_quota_test(repo_path: &Path, mode: WorkingCopyMode) -> Result<()> {
    if !btrfs_available() || !btrfs_is_root() {
        return Ok(());
    }
    tracing::info!("Btrfs quota enforcement test starting");
    let enable_status = Command::new("btrfs")
        .args(["quota", "enable"])
        .arg(repo_path)
        .status()
        .context("failed to enable btrfs quota")?;
    if !enable_status.success() {
        tracing::info!("Btrfs quota enable failed, skipping quota test");
        return Ok(());
    }

    let provider = BtrfsProvider::new();
    let workspace = provider.prepare_writable_workspace(repo_path, mode)?;
    let quota_limit = "10M";
    let limit_status = Command::new("btrfs")
        .args(["qgroup", "limit", quota_limit])
        .arg(&workspace.exec_path)
        .status()
        .context("failed to set btrfs qgroup limit")?;
    if !limit_status.success() {
        tracing::info!("Btrfs qgroup limit failed, skipping quota test");
        provider.cleanup(&workspace.cleanup_token)?;
        return Ok(());
    }

    let large_file = workspace.exec_path.join("quota_stress.bin");
    let payload = vec![0u8; 15 * 1024 * 1024];
    let write_result = fs::write(&large_file, payload);
    let quota_enforced = matches!(&write_result, Err(err) if matches!(err.raw_os_error(), Some(code) if code == ENOSPC || code == EDQUOT));

    let _ = fs::remove_file(&large_file);
    let _ = Command::new("btrfs")
        .args(["qgroup", "limit", "none"])
        .arg(&workspace.exec_path)
        .status();
    provider.cleanup(&workspace.cleanup_token)?;

    ensure!(
        quota_enforced,
        "Btrfs quota test expected ENOSPC/EDQUOT but got {:?}",
        write_result
    );
    tracing::info!("Btrfs quota enforcement test completed");
    Ok(())
}

#[cfg(feature = "zfs")]
fn run_zfs_quota_test(repo_path: &Path, mode: WorkingCopyMode) -> Result<()> {
    tracing::info!("ZFS quota enforcement test starting");

    let provider = ZfsProvider::new();
    let workspace = provider.prepare_writable_workspace(repo_path, mode)?;
    let dataset = match find_zfs_dataset_for_path(&workspace.exec_path)? {
        Some(name) => name,
        None => {
            tracing::info!(
                "ZFS quota test skipping: no dataset found for {}",
                workspace.exec_path.display()
            );
            provider.cleanup(&workspace.cleanup_token)?;
            return Ok(());
        }
    };

    let limit_status = Command::new("zfs")
        .args(["set", "quota=10M", &dataset])
        .status()
        .context("failed to set ZFS quota")?;
    if !limit_status.success() {
        tracing::info!("ZFS quota set failed, skipping quota test");
        provider.cleanup(&workspace.cleanup_token)?;
        return Ok(());
    }

    let large_file = workspace.exec_path.join("quota_stress.bin");
    let payload = vec![0u8; 15 * 1024 * 1024];
    let write_result = fs::write(&large_file, payload);
    let quota_enforced = matches!(&write_result, Err(err) if matches!(err.raw_os_error(), Some(code) if code == ENOSPC || code == EDQUOT));

    let _ = fs::remove_file(&large_file);
    let _ = Command::new("zfs").args(["set", "quota=none", &dataset]).status();
    provider.cleanup(&workspace.cleanup_token)?;
    ensure!(
        quota_enforced,
        "ZFS quota test expected ENOSPC/EDQUOT but got {:?}",
        write_result
    );
    tracing::info!("ZFS quota enforcement test completed");
    Ok(())
}

#[cfg(feature = "zfs")]
fn find_zfs_dataset_for_path(path: &Path) -> Result<Option<String>> {
    let output = Command::new("zfs")
        .args(["list", "-H", "-o", "name,mountpoint"])
        .output()
        .context("failed to invoke zfs list")?;
    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8(output.stdout).context("zfs list output not utf-8")?;
    let wanted = path.canonicalize().unwrap_or_else(|_| path.to_path_buf()).display().to_string();
    for line in stdout.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(name), Some(mount)) = (parts.next(), parts.next()) {
            if mount == wanted {
                return Ok(Some(name.to_string()));
            }
        }
    }
    Ok(None)
}

fn run_error_handling_test<P, F>(
    provider_name: &str,
    mode: WorkingCopyMode,
    factory: F,
) -> Result<()>
where
    P: FsSnapshotProvider,
    F: Fn() -> P,
{
    let invalid_repo = invalid_repo_path();
    let provider = factory();
    let result = provider.prepare_writable_workspace(invalid_repo, mode);
    ensure!(
        result.is_err(),
        "{} provider unexpectedly succeeded when given invalid repository path {}",
        provider_name,
        invalid_repo.display()
    );
    tracing::info!(
        "{} error handling test confirmed invalid path is rejected",
        provider_name
    );
    Ok(())
}

fn invalid_repo_path() -> &'static Path {
    Path::new("/nonexistent/provider-matrix-invalid-repo")
}

/// Run the Git snapshot scenario mirroring the original integration test.
#[cfg(feature = "git")]
pub fn git_snapshot_scenario() -> Result<()> {
    if !git_available() {
        bail!("git command not available on PATH");
    }

    ensure_git_identity_env();

    let temp_dir =
        tempfile::tempdir().context("failed to create temporary repository directory")?;
    populate_test_repo(temp_dir.path())?;
    initialize_git_repo(temp_dir.path())
        .map_err(|err| anyhow::anyhow!("failed to initialise git repository: {err}"))?;

    let provider = GitProvider::new();
    let capabilities = provider.detect_capabilities(temp_dir.path());
    tracing::info!(
        "Provider: {:?}, capability score: {}",
        capabilities.kind,
        capabilities.score
    );
    tracing::info!(
        "Supports CoW overlay: {}",
        capabilities.supports_cow_overlay
    );
    ensure!(
        capabilities.score > 0,
        "Git provider should be available for git repositories"
    );

    let workspace = provider
        .prepare_writable_workspace(temp_dir.path(), WorkingCopyMode::Worktree)
        .context("failed to prepare writable workspace")?;
    tracing::info!("Git workspace created: {}", workspace.exec_path.display());
    ensure!(
        workspace.exec_path.exists(),
        "workspace directory must exist"
    );

    let test_file = workspace.exec_path.join("test_file.txt");
    fs::write(&test_file, "Modified content for snapshot")
        .context("failed to update test file inside workspace")?;

    let snapshot = provider
        .snapshot_now(&workspace, Some("integration_test"))
        .context("failed to create snapshot")?;
    tracing::info!("Git snapshot created: {}", snapshot.id);

    let readonly_path = provider
        .mount_readonly(&snapshot)
        .context("failed to mount snapshot readonly")?;
    tracing::info!("Readonly mount: {}", readonly_path.display());
    ensure!(
        readonly_path.join("README.md").exists(),
        "readonly mount missing README.md"
    );
    ensure!(
        readonly_path.join("test_file.txt").exists(),
        "readonly mount missing modified test file"
    );

    let readonly_content = fs::read_to_string(readonly_path.join("test_file.txt"))
        .context("failed to read readonly test file")?;
    ensure!(
        readonly_content == "Modified content for snapshot",
        "readonly snapshot did not contain expected test file content"
    );

    let branch_ws = provider
        .branch_from_snapshot(&snapshot, WorkingCopyMode::Worktree)
        .context("failed to create branch from snapshot")?;
    tracing::info!("Git branch workspace: {}", branch_ws.exec_path.display());

    let branch_content = fs::read_to_string(branch_ws.exec_path.join("test_file.txt"))
        .context("failed to read test file from branch workspace")?;
    ensure!(
        branch_content == "Modified content for snapshot",
        "branch workspace missing expected file contents"
    );

    provider
        .cleanup(&branch_ws.cleanup_token)
        .context("failed to cleanup Git branch workspace")?;
    provider
        .cleanup(&workspace.cleanup_token)
        .context("failed to cleanup Git workspace")?;

    tracing::info!("Git snapshot scenario completed successfully");
    Ok(())
}

/// Fallback when the `git` feature is disabled.
#[cfg(not(feature = "git"))]
pub fn git_snapshot_scenario() -> Result<()> {
    bail!(
        "fs-snapshots-test-harness built without `git` feature; enable it to run git provider scenarios"
    )
}

#[cfg(feature = "git")]
fn ensure_git_identity_env() {
    const NAME: &str = "AgentFS Snapshot Harness";
    const EMAIL: &str = "agentfs-snapshot@for.testing";
    const VARS: [(&str, &str); 4] = [
        ("GIT_AUTHOR_NAME", NAME),
        ("GIT_AUTHOR_EMAIL", EMAIL),
        ("GIT_COMMITTER_NAME", NAME),
        ("GIT_COMMITTER_EMAIL", EMAIL),
    ];
    for (key, value) in VARS {
        if std::env::var_os(key).is_none()
            || std::env::var(key).map(|v| v.is_empty()).unwrap_or(true)
        {
            std::env::set_var(key, value);
        }
    }
}

#[cfg(feature = "btrfs")]
pub fn btrfs_snapshot_scenario() -> Result<()> {
    if !btrfs_is_root() {
        tracing::info!("Skipping Btrfs snapshot scenario: requires root privileges");
        return Ok(());
    }

    if !btrfs_available() {
        tracing::info!("Skipping Btrfs snapshot scenario: Btrfs tooling not available");
        return Ok(());
    }

    let mut env =
        BtrfsHarnessEnvironment::new().context("failed to create Btrfs harness environment")?;
    let repo_path = env
        .create_btrfs_test_subvolume("btrfs_harness", 256)
        .context("failed to create Btrfs test subvolume")?;

    populate_test_repo(&repo_path)?;

    let provider = BtrfsProvider::new();
    let capabilities = provider.detect_capabilities(&repo_path);
    tracing::info!(
        "Provider: {:?}, capability score: {}",
        capabilities.kind,
        capabilities.score
    );
    tracing::info!(
        "Supports CoW overlay: {}",
        capabilities.supports_cow_overlay
    );
    ensure!(
        capabilities.score > 0,
        "Btrfs provider should be available for the test subvolume"
    );

    let workspace = provider
        .prepare_writable_workspace(&repo_path, WorkingCopyMode::CowOverlay)
        .context("failed to prepare Btrfs writable workspace")?;
    tracing::info!("Btrfs workspace created: {}", workspace.exec_path.display());
    ensure!(
        workspace.exec_path.exists(),
        "Btrfs workspace path should exist"
    );

    let workspace_only_file = workspace.exec_path.join("workspace_only.txt");
    fs::write(
        &workspace_only_file,
        "Btrfs workspace content written by harness",
    )
    .context("failed to write test data into Btrfs workspace")?;
    tracing::info!(
        "Btrfs workspace write succeeded: {}",
        workspace_only_file.display()
    );
    ensure!(
        workspace_only_file.exists(),
        "expected workspace-only file to exist"
    );
    ensure!(
        !repo_path.join("workspace_only.txt").exists(),
        "base repo should not contain workspace-only file"
    );

    let snapshot = provider
        .snapshot_now(&workspace, Some("harness_btrfs_snapshot"))
        .context("failed to create Btrfs snapshot")?;
    tracing::info!("Btrfs snapshot created: {}", snapshot.id);

    let readonly_path = provider
        .mount_readonly(&snapshot)
        .context("failed to mount Btrfs snapshot readonly")?;
    tracing::info!("Readonly mount: {}", readonly_path.display());
    for entry in ["README.md", "test_file.txt"] {
        ensure!(
            readonly_path.join(entry).exists(),
            format!("expected file {entry} in readonly Btrfs snapshot")
        );
    }
    ensure!(
        readonly_path.join("workspace_only.txt").exists(),
        "readonly snapshot missing workspace-only file"
    );

    match provider.branch_from_snapshot(&snapshot, WorkingCopyMode::CowOverlay) {
        Ok(branch_ws) => {
            tracing::info!("Btrfs branch workspace: {}", branch_ws.exec_path.display());
            ensure!(
                branch_ws.exec_path.join("test_file.txt").exists(),
                "Btrfs branch workspace missing expected file contents"
            );
            ensure!(
                branch_ws.exec_path.join("workspace_only.txt").exists(),
                "Btrfs branch workspace missing workspace-only file"
            );
            provider
                .cleanup(&branch_ws.cleanup_token)
                .context("failed to cleanup Btrfs branch workspace")?;
        }
        Err(err) => {
            tracing::info!("Branch creation unavailable for Btrfs snapshot: {err}");
        }
    }

    provider
        .cleanup(&workspace.cleanup_token)
        .context("failed to cleanup Btrfs workspace")?;

    tracing::info!("Btrfs snapshot scenario completed successfully");
    Ok(())
}

/// Run the ZFS snapshot scenario mirroring the original integration test.
#[cfg(feature = "zfs")]
pub fn zfs_snapshot_scenario() -> Result<()> {
    if !zfs_is_root() {
        tracing::info!("Skipping ZFS snapshot scenario: requires root privileges");
        return Ok(());
    }

    if !zfs_available() {
        tracing::info!("Skipping ZFS snapshot scenario: ZFS tooling not available");
        return Ok(());
    }

    let mut env =
        ZfsHarnessEnvironment::new().context("failed to create ZFS harness environment")?;
    let mount_point = match env.create_zfs_test_pool("integration_zfs_pool", 200) {
        Ok(path) => path,
        Err(err) => {
            tracing::info!("Skipping ZFS snapshot scenario: unable to create test pool: {err}");
            return Ok(());
        }
    };
    tracing::info!("Successfully created ZFS pool at {}", mount_point.display());
    populate_test_repo(&mount_point)?;

    let provider = ZfsProvider::new();
    let capabilities = provider.detect_capabilities(&mount_point);
    tracing::info!(
        "ZFS provider capabilities: score={}, supports_cow={}",
        capabilities.score,
        capabilities.supports_cow_overlay
    );
    ensure!(
        matches!(capabilities.kind, SnapshotProviderKind::Zfs),
        "expected ZFS provider for ZFS dataset"
    );

    let workspace = provider
        .prepare_writable_workspace(&mount_point, WorkingCopyMode::Worktree)
        .context("failed to prepare ZFS writable workspace")?;
    tracing::info!("ZFS workspace created: {}", workspace.exec_path.display());
    ensure!(
        workspace.exec_path.exists(),
        "ZFS workspace should exist after preparation"
    );

    let workspace_file = workspace.exec_path.join("integration_test.txt");
    fs::write(
        &workspace_file,
        "ZFS integration content written by harness",
    )
    .context("failed to write test data into ZFS workspace")?;

    let snapshot = provider
        .snapshot_now(&workspace, Some("integration_test"))
        .context("failed to create ZFS snapshot")?;
    tracing::info!("ZFS snapshot created: {}", snapshot.id);

    match provider.mount_readonly(&snapshot) {
        Ok(readonly_path) => {
            tracing::info!("ZFS readonly mount: {}", readonly_path.display());
            for entry in ["README.md", "test_file.txt", "integration_test.txt"] {
                ensure!(
                    readonly_path.join(entry).exists(),
                    "readonly ZFS mount missing expected file: {entry}"
                );
            }
        }
        Err(err) => {
            tracing::info!("Readonly mount unavailable for ZFS snapshot: {err}");
        }
    }

    match provider.branch_from_snapshot(&snapshot, WorkingCopyMode::Worktree) {
        Ok(branch_ws) => {
            tracing::info!("ZFS branch workspace: {}", branch_ws.exec_path.display());
            ensure!(
                branch_ws.exec_path.join("integration_test.txt").exists(),
                "ZFS branch workspace missing integration test file"
            );
            provider
                .cleanup(&branch_ws.cleanup_token)
                .context("failed to cleanup ZFS branch workspace")?;
        }
        Err(err) => {
            tracing::info!("Branch creation unavailable for ZFS snapshot: {err}");
        }
    }

    let used_space = env.get_used_space(&mount_point).unwrap_or(0);
    tracing::info!("ZFS dataset used space (bytes): {}", used_space);

    provider
        .cleanup(&workspace.cleanup_token)
        .context("failed to cleanup ZFS workspace")?;

    tracing::info!("ZFS snapshot scenario completed successfully");
    Ok(())
}

/// Fallback when the `zfs` feature is disabled.
#[cfg(not(feature = "zfs"))]
pub fn zfs_snapshot_scenario() -> Result<()> {
    tracing::info!("Skipping ZFS snapshot scenario: zfs feature disabled");
    Ok(())
}

fn populate_test_repo(root: &Path) -> Result<()> {
    fs::write(root.join("README.md"), "Integration test repository")
        .context("failed to write README.md")?;
    fs::write(root.join("test_file.txt"), "Test content before snapshot")
        .context("failed to write test file")?;

    let subdir = root.join("subdir");
    fs::create_dir_all(&subdir).context("failed to create subdir")?;
    fs::write(subdir.join("nested_file.txt"), "Nested content")
        .context("failed to write nested file")?;

    Ok(())
}
