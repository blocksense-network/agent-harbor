# cspell:words automatable,nextest,Miri,fstypename,ENOTSUP,passwordless,EBUSY,RAII

Below is a pragmatic, milestone-driven plan that will take the macOS AgentFS backstore from "zero lines of code" to "production-ready, CI-gated feature" in small, **fully automatable** steps.  
Every milestone is **self-contained**, ships **only Rust code**, and is **verifiable** by `cargo nextest` in the repository’s existing test harness.  
No manual steps, no root privileges, no hardware assumptions.

---

## Legend
- **Code delta** ≈ lines of **new** Rust (excluding tests).  
- **Test classes** are exhaustive; if a bullet is not checked by CI the milestone is red.  
- **Mock** = in-memory or temp-dir fake; **Real** = hits actual APFS / `diskutil` / `clonefile`.  

---

## M0 – Project skeleton & CI gate  (≈ 30 Δ) - COMPLETED
**Goal**
A crate `agentfs-backstore-macos` compiles and its tests run in CI.

**Tasks**
1. `cargo new --lib agentfs-backstore-macos` inside `crates/`.
2. Add to root `Cargo.toml` workspace.
3. Create `.github/workflows/backstore-macos.yml` that runs:
   ```yaml
   cargo nextest -p agentfs-backstore-macos
   ```
4. Add empty `src/lib.rs` with `#![cfg(target_os = "macos")]` and a dummy `#[test] fn ci_gate() {}`.

**Automated tests**
- [x] CI job passes on `macos-latest` runner.
- [x] Crate is **not** built on Linux/Windows runners (guard cfg).

**Implementation Details**
The milestone established the foundational structure for the macOS backstore implementation. The crate uses a platform-specific cfg guard (`#[cfg(target_os = "macos")]`) to ensure compilation only occurs on macOS targets, preventing cross-platform build issues. The CI workflow follows the established pattern in the repository, using nextest for testing and caching Cargo dependencies for efficiency.

**Key Source Files**
- `crates/agentfs-backstore-macos/src/lib.rs` - Main library file with platform guard and test
- `crates/agentfs-backstore-macos/Cargo.toml` - Generated crate manifest
- `.github/workflows/backstore-macos.yml` - CI workflow for automated testing
- `Cargo.toml` - Workspace configuration including the new crate

**Outstanding Tasks**
None - milestone completed successfully.  

---

## M1 – Backstore trait impl “MockAPFS”  (≈ 120 Δ)
**Goal**  
Fulfill the `Backstore` trait with an **in-memory** simulation that behaves *exactly* like a future real APFS volume (same error codes, same latency model, same reflink semantics).

**Tasks**  
1. `struct MockApfsBackstore { root: TempDir, … }`  
2. Implement `Backstore` trait:
   - `supports_native_snapshots()` → `false`  
   - `supports_native_reflink()` → `true` (we fake `clonefile`)  
   - `reflink()` → hard-link + copy-on-write bitmap (in-RAM).  
   - `root_path()` → `root.path()`.  
3. `snapshot_native()` → `Err(FsError::Unsupported)` with correct error code.

**Automated tests**  
- [ ] Unit: `mock_reflink_same_inode_until_write()`.  
- [ ] Unit: `mock_reflink_preserves_xattrs()`.  
- [ ] Unit: `mock_snapshot_unsupported_error_code()`.  
- [ ] Property: `proptest_reflink_idempotent()`.  
- [ ] Memory-leak test under Miri for reflink loop.  

---

## M2 – Real APFS probe & capability negotiation  (≈ 150 Δ)
**Goal**
Detect at runtime whether the **host** directory lives on APFS, HFS+, or something else, and report correct capability bits. **Revamp HostFsBackstore** from placeholder to real filesystem-aware implementation.

**Tasks**
1. `fn probe_fs_type(path: &Path) -> FsType { … }`
   - Use `statfs` → `f_fstypename` (`"apfs"`, `"hfs"`, …).
2. `RealBackstore::new(root: PathBuf)` → probes once, caches `FsType`.
3. `supports_native_snapshots()` → `fs_type == APFS`.
4. `supports_native_reflink()` → `fs_type == APFS` (for now).
5. **Revamp HostFsBackstore**:
   - Replace placeholder `detect_native_snapshots()` with real `statfs`-based detection
   - Implement real snapshot support delegation (APFS → `diskutil apfs createSnapshot`)
   - Add actual reflink support detection (APFS → `clonefile()`)
   - Remove hardcoded `false` returns and copy fallbacks

**Automated tests**
- [ ] Unit: `probe_apfs_volume()` (CI runner is APFS → must pass).
- [ ] Unit: `probe_hfs_volume()` (create small HFS+ dmg, mount ro, assert `HFS`).
- [ ] Unit: `probe_tmpfs()` (ramdisk) → `Other`.
- [ ] Integration: `RealBackstore::new("/")` succeeds and reports `APFS`.
- [ ] Golden-file test: SSZ-encoded capability response is stable.
- [ ] Unit: `hostfs_backstore_apfs_detection()` (actually detects APFS vs HFS).
- [ ] Unit: `hostfs_backstore_reflink_capability()` (reports true on APFS).

---

## M3 – Native reflink via `clonefile(2)`  (≈ 180 Δ)
**Goal**  
Replace the fake reflink with the real `clonefile()` syscall.

**Tasks**  
1. Bind `clonefile(src, dst, 0)` via `libc::clonefile`.  
2. Fallback path: if `errno == ENOTSUP` (HFS+) → return `FsError::Unsupported`.  
3. Preserve **all** metadata (mode, times, xattrs, ACLs) – APFS does this atomically, but add a test to prove it.

**Automated tests**  
- [ ] Unit: `clonefile_creates_no_new_blocks()` (parse `du` before/after).  
- [ ] Unit: `clonefile_preserves_birth_time()`.  
- [ ] Unit: `clonefile_preserves_xattr_user_test`.  
- [ ] Unit: `clonefile_enospc_fallback()` (inject ENOSPC with `fault_inject` crate).  
- [ ] Property: `proptest_clonefile_then_modify_does_not_affect_src()`.  
- [ ] Benchmark regression test: `clonefile_1gb < 50 ms` on CI hardware.

---

## M4 – Native snapshot creation (`apfs-snapshot` wrapper)  (≈ 220 Δ)
**Goal**  
Implement `snapshot_native()` by invoking `diskutil apfs createSnapshot`.

**Tasks**  
1. `fn apfs_create_snapshot(volume: &Path, name: &str) -> Result<String, FsError>`  
   - Spawn `diskutil apfs createSnapshot <volume> <name> -readonly`.  
   - Parse stdout for snapshot UUID.  
   - Map UUID → internal `SnapshotId`.  
2. Store mapping in `RealBackstore::snapshots: HashMap<SnapshotId, String>` (UUID).  
3. Rollback helper: `fn apfs_delete_snapshot(uuid: &str)`.

**Automated tests**  
- [ ] Unit: `create_snapshot_succeeds_on_apfs()` (needs `sudo` in CI → use `passwordless sudo` runner image).  
- [ ] Unit: `create_snapshot_fails_on_hfs()` → returns `Unsupported`.  
- [ ] Unit: `snapshot_name_sanitization()` (spaces, unicode, max 255 chars).  
- [ ] Integration: `snapshot_create_then_mount_ro()` (mount snapshot to temp mount-point, read file, unmount).  
- [ ] Fault-inject `diskutil` failure → correct SSZ error returned.  
- [ ] Concurrent test: `100 parallel snapshot_create` all succeed and UUIDs unique.

---

## M5 – Snapshot deletion & reference counting  (≈ 160 Δ)
**Goal**  
Allow safe deletion of snapshots when no branch depends on them.

**Tasks**  
1. `drop_snapshot(uuid: &str)` → `diskutil apfs deleteSnapshot <uuid>`.  
2. Keep `Arc<SnapshotMeta>` inside `Branch` → only delete when `strong_count == 0`.  
3. Expose `delete_snapshot()` in public API with `EBUSY` if branches exist.

**Automated tests**  
- [ ] Unit: `delete_snapshot_ok_when_unused()`.  
- [ ] Unit: `delete_snapshot_busy_when_branch_exists()`.  
- [ ] Unit: `delete_snapshot_releases_disk_space()` (compare `df` before/after).  
- [ ] Race test: `create_branch → delete_snapshot → create_branch` (must fail fast).  
- [ ] Property: `proptest_snapshot_lifecycle_refcount()`.

---

## M6 – Ram-disk provisioning helper  (≈ 200 Δ)
**Goal**
Provide a **programmatic** way to create a **temporary APFS volume** for testing or ephemeral backstores. Integrate ramdisk creation into `create_backstore()` for `BackstoreMode::RamDisk`.

**Tasks**
1. Add `BackstoreMode::RamDisk { size_mb: u32 }` to core config.
2. `fn create_apfs_ramdisk(size_mb: u32) -> Result<PathBuf, FsError>`
   - `hdiutil attach -nomount ram://<size_mb*2048>` → get `/dev/diskX`.
   - `diskutil apfs createContainer diskX` → get `diskXsY`.
   - `diskutil apfs addVolume diskXsY APFS AgentFSTest` → mount point `/Volumes/AgentFSTest`.
   - Return mount path.
3. `fn destroy_apfs_ramdisk(mount: &Path)` reverses the above.
4. Wrap both in a RAII `ApfsRamDisk { mount_point, .. }` impl `Drop`.
5. **Integrate into create_backstore()**: When `BackstoreMode::RamDisk` is selected, create ramdisk and return `ApfsBackstore` on it.

**Automated tests**
- [ ] Unit: `ramdisk_create_destroy_cycle()` (no leaks in `diskutil list`).
- [ ] Unit: `ramdisk_is_apfs()` → `probe_fs_type()` returns `APFS`.
- [ ] Unit: `ramdisk_survives_1000_snapshots()` (space check).
- [ ] Integration: entire milestone-4 test suite re-run **on ramdisk** (CI still passes).
- [ ] Unit: `create_backstore_ramdisk_mode()` creates and mounts APFS volume.
- [ ] Benchmark: create + mount + unmount < 2 s on CI.

---

## M7 – Integrated end-to-end "overlay on ramdisk"  (≈ 250 Δ)
**Goal**
Prove that **copy-up**, **snapshot**, **branch**, and **interpose fd_open** all work on a **real APFS volume** created on-the-fly via `BackstoreMode::RamDisk`.

**Tasks**
1. `FsCore::new()` with `BackstoreMode::RamDisk` instantiates backstore with ramdisk-backed APFS.
2. Provide `#[cfg(test)] FsCore::new_ephemeral() -> (FsCore, ApfsRamDisk)` helper for testing.
3. Ensure ramdisk cleanup happens properly on FsCore drop.

**Automated tests** (all use the ephemeral ramdisk)
- [ ] Integration: `overlay_copy_up_on_write_then_snapshot()` (M1→M4 chained).
- [ ] Integration: `branch_from_snapshot_clones_only_metadata()` (block usage unchanged).
- [ ] Integration: `interpose_fd_open_reflink_1gb_file()` (no data copy, verified with `du`).
- [ ] Stress: `100 concurrent writers → snapshot → read from snapshot` (checksum equality).
- [ ] Leak test: after test suite, `diskutil list | grep AgentFSTest` must be empty.

---

## M8 – Performance regression suite & CI gating  (≈ 180 Δ)
**Goal**  
Lock in performance so future changes can’t silently regress.

**Tasks**  
1. `cargo bench -p agentfs-backstore-macos` using `criterion.rs`.  
2. Benchmarks:  
   - `clonefile_100mb`  
   - `snapshot_create_10k_files`  
   - `ramdisk_mount_cycle`  
3. Store baseline json in repo; CI fails on regression > 5 %.  
4. Add `cargo bench -- --save-baseline main` to nightly CI.

**Automated tests**  
- [ ] Benchmarks compile and run < 5 min on `macos-latest`.  
- [ ] PR fails if any benchmark regresses beyond threshold.  
- [ ] Benchmark results uploaded as artifact for download.

---

## M9 – Documentation & consumer crate polish  (≈ 100 Δ)
**Goal**
Ship the crate to **crates.io** and provide ergonomic API for `agentfs-daemon`.

**Tasks**
1. `README.md` with usage example (10 lines).
2. `CHANGELOG.md` with semver tags.
3. `cargo doc --open` passes with zero warnings.
4. Publish `0.1.0` to crates.io from CI on git tag.
5. Add `agentfs-backstore-macos = "0.1"` to `agentfs-daemon` Cargo.toml and replace existing `HostFs` usage.

**Automated tests**
- [ ] `cargo package` lint passes.
- [ ] `cargo audit` clean.
- [ ] Minimal consumer crate in `crates/backstore-consumer-example` builds and passes its own tests.
- [ ] Cross-link test: `agentfs-daemon` unit tests still pass after switch.

---

## M10 – AgentFS Daemon backstore integration  (≈ 150 Δ)
**Goal**
Enable `agentfs-daemon` to consume and manage files from different backstore configurations (HostFs, RamDisk).

**Tasks**
1. Add backstore configuration options to daemon startup (CLI flags/env vars).
2. Update `AgentFsDaemon::new()` to configure `FsCore` with selected `BackstoreMode`.
3. Add ramdisk lifecycle management (cleanup on daemon shutdown).
4. Add backstore status reporting API for clients.
5. Handle backstore-specific error conditions (disk full, ramdisk creation failures).

**Automated tests**
- [ ] Unit: `daemon_backstore_config_parsing()` validates CLI/env backstore options.
- [ ] Integration: `daemon_with_hostfs_backstore()` creates daemon with HostFs mode.
- [ ] Integration: `daemon_with_ramdisk_backstore()` creates and cleans up ramdisk.
- [ ] Unit: `backstore_status_api()` reports backstore type and capacity.
- [ ] Stress: `daemon_ramdisk_lifecycle()` survives daemon restart cycles.

---
