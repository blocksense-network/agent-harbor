// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use anyhow::{Context, Result, bail};
use blake3::Hasher;
use clap::{Args, Parser, Subcommand, ValueEnum};
use libc::{self, pid_t};
use rand::{Rng, RngCore, SeedableRng, rngs::SmallRng, seq::IteratorRandom};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use walkdir::WalkDir;

fn main() -> Result<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).with_target(false).init();

    let cli = Cli::parse();
    match cli.command {
        Command::Run(args) => {
            let report = run_workload(args)?;
            if let Some(path) = report.json_output.clone() {
                serde_json::to_writer_pretty(File::create(&path)?, &report.report)?;
            }
            write_json_to_stdout(&report.report)?;
        }
        Command::Fingerprint(args) => {
            let fp = compute_tree_fingerprint(&args.path)?;
            write_json_to_stdout(&fp)?;
        }
        Command::Resource(args) => {
            let report = run_resource_workload(args)?;
            if let Some(path) = report.json_output.clone() {
                serde_json::to_writer_pretty(File::create(&path)?, &report.report)?;
            }
            write_json_to_stdout(&report.report)?;
        }
        Command::Crash(args) => {
            let report = run_crash_workload(args)?;
            if let Some(path) = report.json_output.clone() {
                serde_json::to_writer_pretty(File::create(&path)?, &report.report)?;
            }
            write_json_to_stdout(&report.report)?;
        }
    }
    Ok(())
}

fn write_json_to_stdout<T: serde::Serialize>(value: &T) -> Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "{}", serde_json::to_string_pretty(value)?)?;
    out.flush()?;
    Ok(())
}

#[derive(Parser)]
#[command(author, version, about = "AgentFS FUSE stress workload runner")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Run(RunArgs),
    Fingerprint(FingerprintArgs),
    Resource(ResourceArgs),
    Crash(CrashArgs),
}

#[derive(Args, Clone)]
struct RunArgs {
    /// Mount point for the AgentFS FUSE filesystem
    #[arg(long)]
    mount: PathBuf,

    /// Working directory under the mount for stress test files
    #[arg(long)]
    workdir: Option<PathBuf>,

    /// Number of worker threads to spawn
    #[arg(long, default_value_t = 16)]
    threads: usize,

    /// Duration of the workload in seconds
    #[arg(long, default_value_t = 120)]
    duration_sec: u64,

    /// Maximum number of files to keep in the working set
    #[arg(long, default_value_t = 4096)]
    max_files: usize,

    /// Maximum file size in KiB
    #[arg(long, default_value_t = 4096)]
    max_file_size_kib: u64,

    /// Optional path for writing the JSON report
    #[arg(long)]
    json_output: Option<PathBuf>,
}

#[derive(Args)]
struct FingerprintArgs {
    #[arg(long)]
    path: PathBuf,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum ResourceMode {
    #[value(name = "fd_exhaust")]
    FdExhaust,
}

#[derive(Args)]
struct ResourceArgs {
    /// Mount point for the AgentFS FUSE filesystem
    #[arg(long)]
    mount: PathBuf,

    /// Working directory for resource stress files
    #[arg(long)]
    workdir: Option<PathBuf>,

    /// Resource scenario to execute
    #[arg(long, value_enum, default_value_t = ResourceMode::FdExhaust)]
    mode: ResourceMode,

    /// Maximum number of files to attempt before declaring PASS/INCOMPLETE
    #[arg(long, default_value_t = 4096)]
    max_open_files: u64,

    /// Optional path for writing the JSON report
    #[arg(long)]
    json_output: Option<PathBuf>,
}

#[derive(Args, Clone)]
struct CrashArgs {
    /// Mount point for the AgentFS FUSE filesystem
    #[arg(long)]
    mount: PathBuf,

    /// Working directory for crash workload files
    #[arg(long)]
    workdir: Option<PathBuf>,

    /// Number of files to create before crashing the host
    #[arg(long, default_value_t = 256)]
    files: usize,

    /// File size (KiB) per crash workload file
    #[arg(long, default_value_t = 1024)]
    file_size_kib: u64,

    /// Optional PID of the agentfs-fuse-host process to kill
    #[arg(long)]
    host_pid: Option<i32>,

    /// Signal to use when terminating the host
    #[arg(long, default_value_t = libc::SIGKILL as i32)]
    kill_signal: i32,

    /// Timeout in seconds when waiting for the host process to exit
    #[arg(long, default_value_t = 20)]
    wait_timeout_sec: u64,

    /// Optional path for writing the JSON report
    #[arg(long)]
    json_output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    Create,
    Write,
    Read,
    Rename,
    Delete,
}

impl Operation {
    fn label(self) -> &'static str {
        match self {
            Operation::Create => "create",
            Operation::Write => "write",
            Operation::Read => "read",
            Operation::Rename => "rename",
            Operation::Delete => "delete",
        }
    }
}

#[derive(Default, Serialize, Clone)]
struct OperationStats {
    create: u64,
    write: u64,
    read: u64,
    rename: u64,
    delete: u64,
}

impl OperationStats {
    fn increment(&mut self, op: Operation) {
        match op {
            Operation::Create => self.create += 1,
            Operation::Write => self.write += 1,
            Operation::Read => self.read += 1,
            Operation::Rename => self.rename += 1,
            Operation::Delete => self.delete += 1,
        }
    }

    fn total(&self) -> u64 {
        self.create + self.write + self.read + self.rename + self.delete
    }
}

impl std::ops::AddAssign<&OperationStats> for OperationStats {
    fn add_assign(&mut self, other: &OperationStats) {
        self.create += other.create;
        self.write += other.write;
        self.read += other.read;
        self.rename += other.rename;
        self.delete += other.delete;
    }
}

#[derive(Serialize, Clone)]
struct IntegritySummary {
    workdir: PathBuf,
    before: TreeFingerprint,
    after: TreeFingerprint,
}

#[derive(Debug, Serialize, Clone)]
struct TreeFingerprint {
    digest: String,
    file_count: u64,
}

#[derive(Serialize, Clone)]
struct RunReport {
    phase: String,
    threads: usize,
    duration_sec: u64,
    max_files: usize,
    max_file_size_kib: u64,
    start_time: String,
    end_time: String,
    operations: OperationStats,
    total_ops: u64,
    benign_errors: HashMap<String, u64>,
    fatal_errors: HashMap<String, u64>,
    integrity: IntegritySummary,
    status: String,
}

struct RunContext {
    report: RunReport,
    json_output: Option<PathBuf>,
}

fn run_workload(args: RunArgs) -> Result<RunContext> {
    let mount = args
        .mount
        .canonicalize()
        .with_context(|| format!("failed to resolve mount path {}", args.mount.display()))?;
    if !mount.is_dir() {
        bail!("mount path {} is not a directory", mount.display());
    }

    let workdir = args.workdir.unwrap_or_else(|| mount.join(".agentfs-stress"));

    fs::create_dir_all(&workdir)
        .with_context(|| format!("failed to create workdir {}", workdir.display()))?;

    let before_fp = compute_tree_fingerprint(&workdir)?;
    let files = Arc::new(Mutex::new(discover_existing_files(&workdir)?));
    let initial_count = files.lock().map(|g| g.len()).unwrap_or(0);

    info!(
        "T7.1 concurrency workload: threads={}, duration={}s, workdir={}, existing_files={}",
        args.threads,
        args.duration_sec,
        workdir.display(),
        initial_count
    );

    let start_time = chrono::Utc::now();
    let stop_at = Instant::now() + Duration::from_secs(args.duration_sec);

    let mut handles = Vec::with_capacity(args.threads);
    for worker_id in 0..args.threads {
        let worker = Worker::new(
            worker_id,
            workdir.clone(),
            stop_at,
            args.max_file_size_kib,
            args.max_files,
            files.clone(),
        );
        handles.push(std::thread::spawn(move || worker.run()));
    }

    let mut aggregate_stats = OperationStats::default();
    let mut benign_errors: HashMap<String, u64> = HashMap::new();
    let mut fatal_errors: HashMap<String, u64> = HashMap::new();

    for handle in handles {
        match handle.join() {
            Ok(result) => {
                aggregate_stats += &result.stats;
                merge_counts(&mut benign_errors, &result.benign_errors);
                merge_counts(&mut fatal_errors, &result.fatal_errors);
            }
            Err(panic) => {
                let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                *fatal_errors.entry("thread_panic".to_string()).or_insert(0) += 1;
                warn!("worker thread panicked: {}", msg);
            }
        }
    }

    let after_fp = compute_tree_fingerprint(&workdir)?;
    let end_time = chrono::Utc::now();

    let status = if fatal_errors.is_empty() {
        "passed".to_string()
    } else {
        "failed".to_string()
    };

    let report = RunReport {
        phase: "concurrency".to_string(),
        threads: args.threads,
        duration_sec: args.duration_sec,
        max_files: args.max_files,
        max_file_size_kib: args.max_file_size_kib,
        start_time: start_time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        end_time: end_time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        operations: aggregate_stats.clone(),
        total_ops: aggregate_stats.total(),
        benign_errors,
        fatal_errors,
        integrity: IntegritySummary {
            workdir,
            before: before_fp,
            after: after_fp,
        },
        status,
    };

    Ok(RunContext {
        report,
        json_output: args.json_output,
    })
}

fn merge_counts(target: &mut HashMap<String, u64>, source: &HashMap<String, u64>) {
    for (key, value) in source {
        *target.entry(key.clone()).or_insert(0) += value;
    }
}

struct Worker {
    id: usize,
    base_dir: PathBuf,
    run_until: Instant,
    max_file_size_kib: u64,
    max_files: usize,
    files: Arc<Mutex<HashSet<PathBuf>>>,
    rng_seed: u64,
}

struct WorkerResult {
    stats: OperationStats,
    benign_errors: HashMap<String, u64>,
    fatal_errors: HashMap<String, u64>,
}

enum OperationResult {
    Completed,
    Benign { label: String },
    Fatal { label: String, detail: String },
}

impl Worker {
    fn new(
        id: usize,
        base_dir: PathBuf,
        run_until: Instant,
        max_file_size_kib: u64,
        max_files: usize,
        files: Arc<Mutex<HashSet<PathBuf>>>,
    ) -> Self {
        let seed_base = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default() as u64;
        let pid_component = (std::process::id() as u64) << 32;
        let rng_seed = seed_base ^ pid_component ^ (id as u64);
        Self {
            id,
            base_dir,
            run_until,
            max_file_size_kib,
            max_files,
            files,
            rng_seed,
        }
    }

    fn run(self) -> WorkerResult {
        let mut rng = SmallRng::seed_from_u64(self.rng_seed);
        let mut stats = OperationStats::default();
        let mut benign_errors: HashMap<String, u64> = HashMap::new();
        let mut fatal_errors: HashMap<String, u64> = HashMap::new();
        let mut seq: u64 = 0;

        if let Err(err) = fs::create_dir_all(&self.base_dir) {
            fatal_errors.insert("worker_dir_create".into(), 1);
            warn!(
                "worker {} failed to create base dir {}: {}",
                self.id,
                self.base_dir.display(),
                err
            );
            return WorkerResult {
                stats,
                benign_errors,
                fatal_errors,
            };
        }

        while Instant::now() < self.run_until {
            let op = self.pick_operation(&mut rng);
            let result = match op {
                Operation::Create => self.create_file(&mut rng, &mut seq),
                Operation::Write => self.write_file(&mut rng),
                Operation::Read => self.read_file(&mut rng),
                Operation::Rename => self.rename_file(&mut rng),
                Operation::Delete => self.delete_file(&mut rng),
            };

            match result {
                OperationResult::Completed => stats.increment(op),
                OperationResult::Benign { label } => {
                    *benign_errors.entry(label).or_insert(0) += 1;
                }
                OperationResult::Fatal { label, detail } => {
                    *fatal_errors.entry(label.clone()).or_insert(0) += 1;
                    debug!("worker {} fatal {}: {}", self.id, label, detail);
                }
            }
        }

        WorkerResult {
            stats,
            benign_errors,
            fatal_errors,
        }
    }

    fn pick_operation(&self, rng: &mut SmallRng) -> Operation {
        let bucket = rng.gen_range(0..100);
        match bucket {
            0..=24 => Operation::Create,
            25..=54 => Operation::Write,
            55..=79 => Operation::Read,
            80..=89 => Operation::Rename,
            _ => Operation::Delete,
        }
    }

    fn create_file(&self, rng: &mut SmallRng, seq: &mut u64) -> OperationResult {
        if self.current_file_count() >= self.max_files {
            return OperationResult::Benign {
                label: "create_pool_full".into(),
            };
        }

        *seq += 1;
        let file_name = format!("worker{:02}-{:016x}.bin", self.id, seq);
        let path = self.base_dir.join(file_name);
        let size_kib = self.max_file_size_kib.max(4);
        let chunk = rng.gen_range(4..=size_kib);
        let size_bytes = (chunk as usize) * 1024;
        let mut data = vec![0u8; size_bytes];
        rng.fill_bytes(&mut data);

        let result = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .and_then(|mut file| file.write_all(&data));

        match result {
            Ok(()) => {
                if let Err(err) = self.add_path(path.clone()) {
                    return OperationResult::Fatal {
                        label: "hashset_insert".into(),
                        detail: err.to_string(),
                    };
                }
                OperationResult::Completed
            }
            Err(err) => classify_error(Operation::Create, err),
        }
    }

    fn write_file(&self, rng: &mut SmallRng) -> OperationResult {
        let Some(target) = self.pick_random_file(rng) else {
            return OperationResult::Benign {
                label: "write_no_target".into(),
            };
        };

        let mut file = match OpenOptions::new().write(true).open(&target) {
            Ok(f) => f,
            Err(err) => return classify_error(Operation::Write, err),
        };

        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let offset = if len == 0 { 0 } else { rng.gen_range(0..=len) };
        let chunk_kib = rng.gen_range(1..=self.max_file_size_kib.max(4));
        let bytes = ((chunk_kib as usize) * 1024).min(512 * 1024);
        let mut data = vec![0u8; bytes.max(4096)];
        rng.fill_bytes(&mut data);

        if let Err(err) = file.seek(SeekFrom::Start(offset)) {
            return classify_error(Operation::Write, err);
        }
        match file.write_all(&data) {
            Ok(()) => OperationResult::Completed,
            Err(err) => classify_error(Operation::Write, err),
        }
    }

    fn read_file(&self, rng: &mut SmallRng) -> OperationResult {
        let Some(target) = self.pick_random_file(rng) else {
            return OperationResult::Benign {
                label: "read_no_target".into(),
            };
        };

        let mut file = match File::open(&target) {
            Ok(f) => f,
            Err(err) => return classify_error(Operation::Read, err),
        };
        let mut buffer = Vec::with_capacity(4096);
        if let Err(err) = file.read_to_end(&mut buffer) {
            return classify_error(Operation::Read, err);
        }
        OperationResult::Completed
    }

    fn rename_file(&self, rng: &mut SmallRng) -> OperationResult {
        let Some(target) = self.pick_random_file(rng) else {
            return OperationResult::Benign {
                label: "rename_no_target".into(),
            };
        };
        let new_name = format!("worker{:02}-renamed-{:016x}.bin", self.id, rng.gen::<u64>());
        let dest = target.parent().map(|parent| parent.join(new_name)).unwrap_or(target.clone());
        match fs::rename(&target, &dest) {
            Ok(()) => {
                if let Err(err) = self.swap_path(&target, dest.clone()) {
                    return OperationResult::Fatal {
                        label: "rename_swap".into(),
                        detail: err.to_string(),
                    };
                }
                OperationResult::Completed
            }
            Err(err) => classify_error(Operation::Rename, err),
        }
    }

    fn delete_file(&self, rng: &mut SmallRng) -> OperationResult {
        let Some(target) = self.pick_random_file(rng) else {
            return OperationResult::Benign {
                label: "delete_no_target".into(),
            };
        };
        match fs::remove_file(&target) {
            Ok(()) => {
                if let Err(err) = self.remove_path(&target) {
                    return OperationResult::Fatal {
                        label: "delete_remove".into(),
                        detail: err.to_string(),
                    };
                }
                OperationResult::Completed
            }
            Err(err) => classify_error(Operation::Delete, err),
        }
    }

    fn pick_random_file(&self, rng: &mut SmallRng) -> Option<PathBuf> {
        let guard = self.files.lock().ok()?;
        guard.iter().choose(rng).cloned()
    }

    fn add_path(&self, path: PathBuf) -> Result<()> {
        let mut guard = self
            .files
            .lock()
            .map_err(|_| io::Error::other("file set poisoned during insert"))?;
        guard.insert(path);
        Ok(())
    }

    fn swap_path(&self, old: &Path, new_path: PathBuf) -> Result<()> {
        let mut guard = self
            .files
            .lock()
            .map_err(|_| io::Error::other("file set poisoned during rename"))?;
        guard.remove(old);
        guard.insert(new_path);
        Ok(())
    }

    fn remove_path(&self, target: &Path) -> Result<()> {
        let mut guard = self
            .files
            .lock()
            .map_err(|_| io::Error::other("file set poisoned during delete"))?;
        guard.remove(target);
        Ok(())
    }

    fn current_file_count(&self) -> usize {
        self.files.lock().map(|g| g.len()).unwrap_or(0)
    }
}

fn classify_error(op: Operation, err: io::Error) -> OperationResult {
    use std::io::ErrorKind::*;
    if let Some(errno) = err.raw_os_error() {
        match errno {
            libc::EMFILE | libc::ENFILE => {
                return OperationResult::Fatal {
                    label: format!("{}_emfile", op.label()),
                    detail: err.to_string(),
                };
            }
            libc::ENOSPC => {
                return OperationResult::Fatal {
                    label: format!("{}_enospace", op.label()),
                    detail: err.to_string(),
                };
            }
            libc::EACCES => {
                return OperationResult::Fatal {
                    label: format!("{}_eacces", op.label()),
                    detail: err.to_string(),
                };
            }
            _ => {}
        }
    }

    match err.kind() {
        NotFound => OperationResult::Benign {
            label: format!("{}_not_found", op.label()),
        },
        AlreadyExists => OperationResult::Benign {
            label: format!("{}_already_exists", op.label()),
        },
        _ => OperationResult::Fatal {
            label: format!(
                "{}_{}",
                op.label(),
                format!("{:?}", err.kind()).to_lowercase()
            ),
            detail: err.to_string(),
        },
    }
}

fn discover_existing_files(workdir: &Path) -> Result<HashSet<PathBuf>> {
    let mut set = HashSet::new();
    if !workdir.exists() {
        return Ok(set);
    }
    for entry in WalkDir::new(workdir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            set.insert(entry.path().to_path_buf());
        }
    }
    Ok(set)
}

fn compute_tree_fingerprint(root: &Path) -> Result<TreeFingerprint> {
    if !root.exists() {
        return Ok(TreeFingerprint {
            digest: "blake3:0".to_string(),
            file_count: 0,
        });
    }

    let mut hasher = Hasher::new();
    let mut file_count = 0u64;
    let mut buffer = vec![0u8; 32 * 1024];

    for entry in WalkDir::new(root)
        .sort_by(|a, b| a.file_name().cmp(b.file_name()))
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.into_path();
        if path.is_dir() {
            continue;
        }
        file_count += 1;
        if let Ok(rel) = path.strip_prefix(root) {
            let rel_str = rel.to_string_lossy();
            hasher.update(rel_str.as_bytes());
        }
        let mut file = File::open(&path)?;
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
    }

    Ok(TreeFingerprint {
        digest: format!("blake3:{}", hasher.finalize().to_hex()),
        file_count,
    })
}

#[derive(Serialize, Clone)]
struct ResourceReport {
    phase: String,
    scenario: String,
    start_time: String,
    end_time: String,
    max_open_files: u64,
    opened_files: u64,
    failure_errno: Option<i32>,
    failure_label: Option<String>,
    cleanup_ms: u128,
    fd_count_before: u64,
    fd_count_peak: u64,
    fd_count_after: u64,
    status: String,
}

struct ResourceContext {
    report: ResourceReport,
    json_output: Option<PathBuf>,
}

#[derive(Serialize, Clone)]
struct CrashReport {
    phase: String,
    scenario: String,
    workdir: PathBuf,
    files_created: usize,
    file_size_kib: u64,
    fingerprint: TreeFingerprint,
    killed_pid: i32,
    kill_signal: i32,
    wait_timeout_sec: u64,
    status: String,
}

struct CrashContext {
    report: CrashReport,
    json_output: Option<PathBuf>,
}

fn run_resource_workload(args: ResourceArgs) -> Result<ResourceContext> {
    let mount = args
        .mount
        .canonicalize()
        .with_context(|| format!("failed to resolve mount path {}", args.mount.display()))?;
    if !mount.is_dir() {
        bail!("mount path {} is not a directory", mount.display());
    }

    let workdir = args.workdir.unwrap_or_else(|| mount.join(".agentfs-stress").join("resource"));
    fs::create_dir_all(&workdir)
        .with_context(|| format!("failed to create resource workdir {}", workdir.display()))?;

    let start_time = chrono::Utc::now();
    let fd_before = count_open_fds().unwrap_or_default();
    let scenario = match args.mode {
        ResourceMode::FdExhaust => run_fd_exhaust(&workdir, args.max_open_files)?,
    };
    let cleanup_start = Instant::now();
    cleanup_resource_dir(&workdir)?;
    let cleanup_ms = cleanup_start.elapsed().as_millis();
    let fd_after = count_open_fds().unwrap_or_default();
    let end_time = chrono::Utc::now();

    let report = ResourceReport {
        phase: "resource".to_string(),
        scenario: scenario.scenario,
        start_time: start_time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        end_time: end_time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        max_open_files: args.max_open_files,
        opened_files: scenario.opened_files,
        failure_errno: scenario.failure_errno,
        failure_label: scenario.failure_label,
        cleanup_ms,
        fd_count_before: fd_before,
        fd_count_peak: scenario.fd_count_peak,
        fd_count_after: fd_after,
        status: scenario.status,
    };

    Ok(ResourceContext {
        report,
        json_output: args.json_output,
    })
}

struct ResourceScenarioResult {
    scenario: String,
    opened_files: u64,
    failure_errno: Option<i32>,
    failure_label: Option<String>,
    fd_count_peak: u64,
    status: String,
}

fn run_fd_exhaust(workdir: &Path, max_open: u64) -> Result<ResourceScenarioResult> {
    let mut handles: Vec<(PathBuf, File)> = Vec::new();
    let mut opened = 0u64;
    let mut failure_errno = None;
    let mut failure_label = None;
    let mut fd_peak = count_open_fds().unwrap_or_default();
    for idx in 0..max_open {
        let path = workdir.join(format!("fd-exhaust-{idx:016x}.bin"));
        match OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => {
                handles.push((path, file));
                opened += 1;
                fd_peak = fd_peak.max(count_open_fds().unwrap_or(fd_peak));
            }
            Err(err) => {
                failure_errno = err.raw_os_error();
                failure_label = if let Some(code) = failure_errno {
                    Some(format!("errno_{code}"))
                } else {
                    Some(format!("{:?}", err.kind()))
                };
                break;
            }
        }
    }
    drop(handles);
    let status = match failure_errno {
        Some(code) if code == libc::EMFILE || code == libc::ENFILE => "passed".to_string(),
        Some(_) => "failed".to_string(),
        None => "incomplete".to_string(),
    };
    Ok(ResourceScenarioResult {
        scenario: "fd_exhaust".to_string(),
        opened_files: opened,
        failure_errno,
        failure_label,
        fd_count_peak: fd_peak,
        status,
    })
}

fn cleanup_resource_dir(workdir: &Path) -> Result<()> {
    if workdir.exists() {
        for entry in fs::read_dir(workdir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let _ = fs::remove_file(path);
            }
        }
    }
    Ok(())
}

fn count_open_fds() -> Result<u64> {
    let fd_dir = PathBuf::from("/proc/self/fd");
    if !fd_dir.exists() {
        return Ok(0);
    }
    Ok(fs::read_dir(fd_dir)?.count() as u64)
}

fn run_crash_workload(args: CrashArgs) -> Result<CrashContext> {
    let mount = args
        .mount
        .canonicalize()
        .with_context(|| format!("failed to resolve mount path {}", args.mount.display()))?;
    if !mount.is_dir() {
        bail!("mount path {} is not a directory", mount.display());
    }

    let workdir = args.workdir.unwrap_or_else(|| mount.join(".agentfs-stress").join("crash"));
    fs::create_dir_all(&workdir)
        .with_context(|| format!("failed to create crash workdir {}", workdir.display()))?;

    let mut files_created = 0usize;
    let mut rng = SmallRng::from_entropy();
    for idx in 0..args.files {
        let path = workdir.join(format!("crash-file-{idx:016x}.bin"));
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .with_context(|| format!("failed to create crash file {}", path.display()))?;
        let bytes = ((args.file_size_kib.max(4) as usize) * 1024).min(2 * 1024 * 1024);
        let mut data = vec![0u8; bytes];
        rng.fill_bytes(&mut data);
        file.write_all(&data)
            .with_context(|| format!("failed to write crash file {}", path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync crash file {}", path.display()))?;
        files_created += 1;
    }

    let fingerprint = compute_tree_fingerprint(&workdir)?;
    let host_pid = match args.host_pid {
        Some(pid) => pid,
        None => detect_host_pid(&mount)? as i32,
    };

    kill_host_process(host_pid, args.kill_signal, args.wait_timeout_sec)?;

    let report = CrashReport {
        phase: "crash".to_string(),
        scenario: "kill_host".to_string(),
        workdir,
        files_created,
        file_size_kib: args.file_size_kib,
        fingerprint,
        killed_pid: host_pid,
        kill_signal: args.kill_signal,
        wait_timeout_sec: args.wait_timeout_sec,
        status: "passed".to_string(),
    };

    Ok(CrashContext {
        report,
        json_output: args.json_output,
    })
}

fn detect_host_pid(mount: &Path) -> Result<pid_t> {
    let mut matches = Vec::new();
    let mount_str = mount.to_string_lossy();
    for entry in fs::read_dir("/proc").context("failed to read /proc")? {
        let entry = entry?;
        let file_name = entry.file_name();
        let pid: pid_t = match file_name.to_string_lossy().parse() {
            Ok(pid) => pid,
            Err(_) => continue,
        };
        let cmdline_path = entry.path().join("cmdline");
        let cmdline = match fs::read(&cmdline_path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        if cmdline.is_empty() {
            continue;
        }
        let cmdline_str = cmdline
            .split(|b| *b == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part))
            .collect::<Vec<_>>()
            .join(" ");
        if cmdline_str.contains("agentfs-fuse-host") && cmdline_str.contains(mount_str.as_ref()) {
            matches.push(pid);
        }
    }

    match matches.len() {
        0 => bail!("unable to detect agentfs-fuse-host for {}", mount.display()),
        1 => Ok(matches[0]),
        _ => bail!(
            "multiple agentfs-fuse-host processes match {}",
            mount.display()
        ),
    }
}

fn kill_host_process(pid: pid_t, signal: i32, timeout_sec: u64) -> Result<()> {
    let res = unsafe { libc::kill(pid, signal) };
    if res != 0 {
        return Err(io::Error::last_os_error())
            .with_context(|| format!("failed to signal pid {}", pid));
    }
    let deadline = Instant::now() + Duration::from_secs(timeout_sec);
    while process_exists(pid) {
        if Instant::now() > deadline {
            bail!("timed out waiting for pid {} to exit", pid);
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn process_exists(pid: pid_t) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn fingerprint_stable_for_same_contents() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("file.bin");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"hello").unwrap();
        file.flush().unwrap();

        let fp1 = compute_tree_fingerprint(dir.path()).unwrap();
        let fp2 = compute_tree_fingerprint(dir.path()).unwrap();
        assert_eq!(fp1.digest, fp2.digest);
        assert_eq!(fp1.file_count, 1);
    }

    #[test]
    fn fingerprint_changes_with_content() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("file.bin");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"hello").unwrap();
        file.flush().unwrap();

        let fp1 = compute_tree_fingerprint(dir.path()).unwrap();

        let mut file = File::create(&file_path).unwrap();
        file.write_all(b"world").unwrap();
        file.flush().unwrap();

        let fp2 = compute_tree_fingerprint(dir.path()).unwrap();
        assert_ne!(fp1.digest, fp2.digest);
    }
}
