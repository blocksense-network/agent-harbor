## AgentFS Core — Testing Strategy

### Goals

- Verify functional correctness of the Rust core across all features: files/dirs ops, metadata, xattrs/ADS, locks, snapshots, branches, and process‑scoped bindings.
- Validate platform semantics parity: POSIX (Linux/macOS) and NTFS (Windows) behaviors via glue‑agnostic unit tests and glue‑backed integration tests.
- Ensure performance characteristics (latency/throughput) meet targets under representative workloads.
- Prove robustness under concurrency, failure injection, and resource pressure (memory spill, ENOSPC).

### I/O path characterization

**T1. Lower pass-through read:** Read a 1 GiB file never touched by upper; assert no upper node exists post-read; measure throughput.
**T2. Copy-up metadata seeding:** Touch for write; assert upper mode/uid/gid/ACL/xattrs cloned from lower per policy.
**T3. KBP cache-warm throughput:** Re-read same 1 GiB; expect ≥ 90% of host-FS direct read.
**T4. Interpose zero-overhead:** Under shim, open→write→read→mmap large file; assert **no adapter read/write callbacks**; contents correct.
**T5. Snapshot paths:** With ramdisk native snapshots, snapshot latency ≤ 300 ms for ≤5k active upper files; with copy-active fallback ≤ 2 s.
**T6. Windows reparse (experimental):** When enabled, open returns NTFS handle; throughput near native; verify path-level ops still enforced via control plane and document forbidden sequences.
**T7. Interpose RO first, then writer:** Open RO via interpose, then open RW via interpose and write. The RO reader must observe new data (reads/mmap) without reopening.
**T8. Large file no-reflink:** Interpose open on big file where no reflink exists → `FORWARDING_UNAVAILABLE`; same app can still open via mount and read; later writer creates upper; subsequent interpose opens (on same path) now succeed (size no longer matters).

### Step-by-step test plans (normative)

The following plans are the authoritative, step-by-step procedures. Sections below provide background and matrices.

#### U1. Create/Read/Write round-trip (unit)

1. Initialize `FsCore` with default `FsConfig` (in-memory backend; writeback cache off).
2. Call `mkdir("/dir", 0o755)`.
3. Call `create("/dir/a.txt", OpenOptions{ create:true, write:true, truncate:true, .. })` → keep `HandleId`.
4. Call `write(h, 0, b"hello")` and `close(h)`.
5. Call `open("/dir/a.txt", OpenOptions{ read:true })` → `h2`.
6. Call `read(h2, 0, buf[0..5])`; expect 5 bytes, contents "hello"; `close(h2)`.

#### U2. Delete-on-close semantics (unit)

1. Create `/x` and write non-empty content; keep `HandleId` open.
2. Call `unlink("/x")`.
3. Call `read(h, 0, buf)`; expect success while handle open.
4. Call `close(h)`.
5. Call `open("/x", OpenOptions{ read:true })`; expect `Err(NotFound)`.

#### U3. Snapshot immutability vs branch writes (unit)

1. Create `/f` and write "base"; `close`.
2. Call `snapshot_create(name="base")` → `snap`.
3. Call `branch_create_from_snapshot(snap, name="b")` → `branch`.
4. Call `bind_process_to_branch(branch)`.
5. `open("/f", OpenOptions{ read:true, write:true })` and overwrite with "branch"; `close`.
6. Read `/f` through the snapshot context (helper resolving by `snap`) and assert content is still "base".

#### U4. Branch process isolation (unit/scenario)

1. Create `snap = snapshot_create("clean")`.
2. `b1 = branch_create_from_snapshot(snap, "p1")`; `b2 = branch_create_from_snapshot(snap, "p2")`.
3. Bind current process to `b1`; write `/same.txt` → "one".
4. In a spawned thread (or separate test process), bind to `b2`; write `/same.txt` → "two".
5. From `b1` context, read `/same.txt` and expect "one".
6. From `b2` context, read `/same.txt` and expect "two".

#### U5. Xattrs and ADS basics (unit)

1. Create `/x`.
2. Call `xattr_set("/x", "user.test", b"v")`; then `xattr_get` → expect `b"v"`.
3. If Windows ADS is enabled in config, open `/x:meta` with `OpenOptions{ create:true, write:true, stream:Some("meta") }` and write "s"; `streams_list("/x")` should include `meta`.

#### U6. POSIX locks and Windows share modes (unit/component)

1. Open `/lock` twice with read/write allowed; keep `h1`, `h2`.
2. Call `lock(h1, LockRange { 0..100, Exclusive })`; expect success.
3. On `h2`, attempt `lock(..., Exclusive)` overlapping range; expect conflict error.
4. For share modes (component test via adapter): open first handle with deny-write; attempt second open with write access; expect adapter to reject per share admission.

#### U7. Overlay pass-through read (unit)

1. Configure `FsConfig.lower.root` to a temp directory with file `/x` containing "L".
2. In a fresh branch with no upper entries, `read("/x")` returns "L" without creating an upper entry.
3. `stat("/x")` reflects lower metadata.

#### U8. Copy-up on first write (unit)

1. Same setup as U7. `write("/x", "U")` triggers `copy_up("/x")`.
2. Subsequent `read("/x")` returns "U"; lower `/x` remains "L".
3. Upper metadata initialized from lower, then mtime updated per write.

#### U9. Metadata overlay without data copy (unit)

1. Call `chmod("/x", 0600)` on an unmodified `/x` → creates metadata overlay only.
2. `stat("/x")` reports `0600`, but `read("/x")` continues to pass through to lower until first data write.

#### C1. C ABI round-trip (component)

1. Build the C test harness linking to the core’s C ABI symbols.
2. Call `af_fs_create` with JSON config; expect `AF_OK`.
3. Call `af_snapshot_create` and `af_branch_create_from_snapshot` and `af_bind_process_to_branch`.
4. Call `af_open` and `af_write`/`af_read`; validate transfer counts.
5. Call `af_close` and `af_fs_destroy`.

#### I1. FUSE host basic operations (integration)

Prerequisite: libfuse available; adapter binary built.

1. Start the FUSE host mounting to a temporary directory (e.g., `/tmp/ahfs`).
2. `mkdir /tmp/ahfs/dir` → expect success; `ls` shows `dir`.
3. `sh -lc 'echo hello > /tmp/ahfs/dir/a.txt'`.
4. `cat /tmp/ahfs/dir/a.txt` → expect `hello`.
5. `mv /tmp/ahfs/dir/a.txt /tmp/ahfs/dir/b.txt`; `stat` reflects new name.
6. Unmount cleanly; no panics or leaks in logs.

#### I2. FUSE control plane (integration)

1. Open `<MOUNT>/.agentfs/control` and issue ioctl for `snapshot.create(name="clean")`.
2. Issue `branch.create(from=<id>, name="test")`.
3. Issue `branch.bind(branch=<id>, pid=<self>)`.
4. Write `/x` and verify content exists; after unbinding and re-binding a fresh branch, `/x` should be absent.

#### I3. WinFsp host basics (integration, Windows)

Prerequisite: WinFsp installed; adapter built.

1. Start the host; mount to `X:`.
2. `cmd /c echo hello > X:\a.txt`; `type X:\a.txt` → expect `hello`.
3. Create ADS: `cmd /c echo meta > X:\a.txt:meta`; enumerate streams via adapter API or `GetStreamInfo`; expect `meta` present.
4. Attempt delete-on-close: open `X:\t.txt`, mark for delete, close last handle; subsequent `dir` should not list `t.txt`.
5. Use DeviceIoControl to send `snapshot.create/list` and `branch.create/bind`; verify behavior per IDs.

#### I4. Adapter passthrough (integration)

1. Mount with `FsConfig.lower.root`.
2. Read large file without prior writes; verify throughput ≈ reading the same file from the lower path directly.
3. Write; verify copy-up occurs and subsequent I/O hits backstore.

#### I4. FSKit adapter basics (integration, macOS 15+)

Prerequisite: FSKit extension built and signed; XPC client available.

1. Activate the volume via the extension; root item present.
2. Create directory/file; enumerate; read/write succeed.
3. Send XPC control messages for snapshot/branch/bind; verify behavior.

#### S1. AH workflow scenario: branch-per-task

1. Mount an adapter (FUSE/WinFsp/FSKit) to a test mount.
2. Create `snapshot.create(name="clean")`.
3. Create branch `task-1` from `clean` and bind current PID.
4. Run `git init` and create `/project/README.md` with content "task1".
5. In a second shell bound to a new branch `task-2`, write "task2" to the same path.
6. Verify each branch sees its own content; `snapshot.list` shows both parent and new branches; discard `task-2` and verify `task-1` unaffected.

#### P1. Microbenchmark baseline (performance)

1. Run criterion benchmarks for small-file create/delete, sequential 64KB writes to 16MB file, random 4KB IO.
2. Record throughput and p99 latency.
3. Acceptance: within 1.5x memfs baseline for RAM-backed; document variance when spill enabled.

#### R1. Spill-to-disk and ENOSPC (reliability)

1. Configure `MemoryPolicy { max_bytes_in_memory: 64 MiB, spill_directory: <tmp> }`.
2. Write multiple files totaling >256 MiB; confirm `bytes_spilled > 0` in `stats()`.
3. Fill spill directory quota to force ENOSPC; ensure writes return `FsError::NoSpace` and no invariants are violated.

#### R2. Crash safety (reliability)

1. Start adapter host; begin sustained writes to a file.
2. Abruptly terminate the host process.
3. Restart host; mount again; run consistency checks (no panics; directory tree invariants hold; no partial API-visible corruption).

#### CI1. Cross-platform lanes (CI)

1. Linux: run unit + FUSE integration (privileged where required); ensure mounts/unmounts succeed and tests are green.
2. macOS: run unit + macFUSE integration; later add FSKit extension build and smoke tests.
3. Windows: run unit + WinFsp integration; run `winfstest`/`IfsTest` subsets.

### Test Layers

1. Unit tests (Rust, core‑only)

- Scope: Pure in‑process testing of `FsCore`, VFS operations, snapshot engine, storage backends, locking, xattrs, streams.
- Tools: `cargo test` with `#[cfg(test)]`, property tests (quickcheck/proptest) for path normalization and CoW invariants.
- Fault injection: simulate I/O failures from `StorageBackend`, forced allocation failures, artificial delays.

2. Component tests (Rust, minimal glue shims)

- Scope: Exercise the C ABI (FFI) surface to validate ABI stability and error mapping.
- Tools: `ctest` or a small C harness called from Rust (`cc` crate), round‑trip UTF‑8/UTF‑16 name mapping.

3. Integration tests (per‑platform glue)

- Linux/macOS (FUSE/libfuse or macFUSE for early tests):
  - Mount a temp volume backed by AgentFS core.
  - Reuse libfuse example patterns (hello/passthrough variants) to ensure compliance on getattr/readdir/open/read/write/rename/unlink.
  - Run pjdfstests / libfuse tests where feasible.
- Windows (WinFsp):
  - Mount a drive letter using a thin WinFsp host backed by AgentFS core.
  - Run WinFsp’s test batteries: winfstest, IfsTest, fsbench read/write tests, change notification tests.
  - Validate delete‑on‑close, share modes, ADS.

4. Scenario tests (AH workflows)

- Simulate AH ‘task session’ lifecycle: snapshot baseline → branch per task → writes → verify isolation → branch discard/keep.
- Codify per‑process view isolation: two processes bound to different branches operating on same absolute paths get divergent results.
- Browser automation artifacts: ensure xattrs (quarantine, FinderInfo) round‑trip on macOS.

### Test Matrix

- Case sensitivity: Sensitive vs Insensitive‑preserving (macOS/Windows default) trees; conflicting names ("Readme" vs "README").
- Streams & xattrs: presence/absence; large values; list ordering; Unicode names.
- Locks: POSIX record locks (overlapping ranges), BSD flock; Windows share modes and mandatory locks.
- Paths: deep hierarchies; long names; illegal name rejection on Windows (`<`, `>`, `:`, `"`, `/`, `\`, `|`, `?`, and `*` and reserved names); Unicode normalization.
- Symlinks & hardlinks: creation, traversal, unlink of one link preserving data when other exists.
- Concurrency: parallel readdir with concurrent creates/renames; lock contention; mixed readers/writers.
- Snapshots/branches:
  - snapshot immutability
  - writable clones diverge
  - whiteouts (deletes) are branch‑local
  - handle stability across snapshot creation
  - many branches (limits), GC of reference‑counted content when branches are deleted
- Storage backends: in‑memory vs temp‑file spill; ENOSPC on spill directory; restart behavior (ephemeral semantics).

### Representative Unit Tests (Rust)

```rust
#[test]
fn create_read_write_roundtrip() {
    let core = test_core();
    core.mkdir("/dir".as_ref(), 0o755).unwrap();
    let h = core.create("/dir/a.txt".as_ref(), &rw_create()).unwrap();
    core.write(h, 0, b"hello").unwrap();
    core.close(h).unwrap();
    let h2 = core.open("/dir/a.txt".as_ref(), &ro()).unwrap();
    let mut buf = [0u8; 5];
    let n = core.read(h2, 0, &mut buf).unwrap();
    assert_eq!(n, 5);
    assert_eq!(&buf, b"hello");
}

#[test]
fn snapshot_immutable_branch_writable() {
    let core = test_core();
    core.create("/f".as_ref(), &rw_create()).unwrap();
    let snap = core.snapshot_create(Some("base")).unwrap();
    let b = core.branch_create_from_snapshot(snap, Some("b")).unwrap();
    core.bind_process_to_branch(b).unwrap();
    let h = core.open("/f".as_ref(), &rw()).unwrap();
    core.write(h, 0, b"branch").unwrap();
    core.close(h).unwrap();
    // Reading via snapshot should still see empty/original
    // (accessed via a helper that resolves by snapshot id)
    assert_eq!(read_through_snapshot(&core, snap, "/f"), b"");
}

#[test]
fn unlink_delete_on_close_semantics() {
    let core = test_core();
    let h = core.create("/x".as_ref(), &rw_create()).unwrap();
    core.unlink("/x".as_ref()).unwrap();
    // Still readable until close
    let mut b = [0; 0];
    core.read(h, 0, &mut b).unwrap();
    core.close(h).unwrap();
    assert!(core.open("/x".as_ref(), &ro()).is_err());
}
```

### Adapter/Glue Integration Tests

- FUSE Host (Linux/macOS dev):
  - Build a minimal binary linking libfuse high‑level API and calling into `FsCore`.
  - Run libfuse `example/` ops and test suite: getattr/readdir/open ioctls; readdir+ path; cache toggles.

- WinFsp Host (Windows CI):
  - Build a host (similar to WinFsp MEMFS) delegating to `FsCore`.
  - Execute: `winfsp-tests`, `winfstest`, `IfsTest`, `fsbench`.
  - Validate ADS (`file:stream`), change notifications, delete‑on‑close.

### Performance Tests

- Microbenchmark suite (criterion): small file create/delete, sequential RW (1MB–1GB), random RW, stat-heavy operations.
- Macro tests (fsbench/fio): throughput and p99 latency under concurrency; with/without writeback cache; readdir+ on large directories.
- Memory pressure: tune `max_bytes_in_memory`; verify spill path IO; ensure bounded memory growth with steady workload.

### Reliability & Fault Injection

- Force `StorageBackend` to return transient and permanent errors; verify error mapping and recovery.
- Simulate thread preemption under lock contention; deadlock detection by timeouts on CI.
- Abrupt termination: kill host during sustained writes; on restart, verify no core invariants broken (ephemeral state).

### CI & Environments

- Linux (Ubuntu): run unit, component, and FUSE integration tests (with `--privileged` when needed for mount in CI).
- macOS: unit tests + macFUSE/libfuse integration (until FSKit host is available); later add FSKit extension tests with Xcode build.
- Windows: GitHub Actions or self‑hosted runner with WinFsp installed; run WinFsp test suites and fsbench.

### Coverage & Quality Gates

- Line and branch coverage thresholds for core modules (storage, vfs, snapshots, locking) ≥ 85%.
- Mutation testing for critical path (metadata ops, CoW layer).
- Lints: `clippy` (pedantic where reasonable), `rustfmt` stable; no unsafe in public API; unsafe internals reviewed.

### Artifacts and Debugging Aids

- Deterministic test seeds and reproducible fixtures.
- Structured tracing (feature‑gated) for snapshot/branch ops, lock decisions, and spill events.
- Dump utilities to serialize tree roots and refcounts for failure triage.

### Exit Criteria

- All unit/component/integration suites green across platforms.
- WinFsp core test battery passes (with allowed exceptions documented).
- libfuse example/behavioral tests show parity for expected operations; readdir+ validated.
- Performance targets met: comparable to memfs baselines for RAM‑backed workloads; bounded degradation when spilling.
