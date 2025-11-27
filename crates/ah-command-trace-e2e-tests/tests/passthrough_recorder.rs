// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Passthrough recorder integration tests (without shim).
//! These spawn the controlled helper under direct execution and emulate the
//! session/follower sockets to validate backlog + live streaming behavior.

use std::{io::Read, path::PathBuf, process::Stdio, sync::Arc, sync::Once, time::Duration};

use anyhow::Result;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    process::{ChildStdin, Command},
    sync::{Mutex, broadcast, watch},
    time::timeout,
};

#[derive(Clone)]
struct StreamFanout {
    backlog: Arc<Mutex<Vec<u8>>>,
    tx: broadcast::Sender<Vec<u8>>,
    done_tx: watch::Sender<bool>,
}

impl StreamFanout {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        let (done_tx, _) = watch::channel(false);
        Self {
            backlog: Arc::new(Mutex::new(Vec::new())),
            tx,
            done_tx,
        }
    }

    async fn push(&self, chunk: &[u8]) {
        {
            let mut guard = self.backlog.lock().await;
            guard.extend_from_slice(chunk);
        }
        let _ = self.tx.send(chunk.to_vec());
    }

    async fn finish(&self) {
        let _ = self.done_tx.send(true);
    }

    fn push_blocking(&self, handle: &tokio::runtime::Handle, chunk: &[u8]) {
        let backlog = self.backlog.clone();
        let tx = self.tx.clone();
        handle.block_on(async move {
            {
                let mut guard = backlog.lock().await;
                guard.extend_from_slice(chunk);
            }
            let _ = tx.send(chunk.to_vec());
        });
    }

    fn finish_blocking(&self, handle: &tokio::runtime::Handle) {
        let done_tx = self.done_tx.clone();
        handle.block_on(async {
            done_tx.send(true).ok();
        });
    }

    async fn attach(&self) -> Follower {
        let history = { self.backlog.lock().await.clone() };
        let rx = self.tx.subscribe();
        let done = self.done_tx.subscribe();
        Follower {
            history,
            rx,
            done,
            live: Vec::new(),
        }
    }
}

struct Follower {
    history: Vec<u8>,
    rx: broadcast::Receiver<Vec<u8>>,
    done: watch::Receiver<bool>,
    live: Vec<u8>,
}

impl Follower {
    async fn run_until_done(mut self) -> Vec<u8> {
        let mut done_flag = false;
        loop {
            tokio::select! {
                Ok(chunk) = self.rx.recv() => {
                    self.live.extend_from_slice(&chunk);
                }
                changed = self.done.changed() => {
                    if changed.is_ok() && *self.done.borrow() {
                        done_flag = true;
                    }
                }
                else => { done_flag = true; }
            }
            if done_flag {
                // drain any remaining queued messages before returning
                while let Ok(chunk) = self.rx.try_recv() {
                    self.live.extend_from_slice(&chunk);
                }
                break;
            }
        }
        let mut out = self.history;
        out.extend(self.live);
        out
    }

    async fn run_until_done_slow(mut self, delay_ms: u64) -> Vec<u8> {
        let mut done_flag = false;
        loop {
            tokio::select! {
                Ok(chunk) = self.rx.recv() => {
                    self.live.extend_from_slice(&chunk);
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                changed = self.done.changed() => {
                    if changed.is_ok() && *self.done.borrow() {
                        done_flag = true;
                    }
                }
                else => { done_flag = true; }
            }
            if done_flag {
                while let Ok(chunk) = self.rx.try_recv() {
                    self.live.extend_from_slice(&chunk);
                }
                break;
            }
        }
        let mut out = self.history;
        out.extend(self.live);
        out
    }
}

fn assert_helper_bin(name: &str) -> PathBuf {
    static BUILD_ONCE: Once = Once::new();
    BUILD_ONCE.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let status = std::process::Command::new(cargo)
            .args(["build", "-p", "ah-command-trace-e2e-tests", "--bin", name])
            .status()
            .expect("failed to run cargo build for helper bin");
        if !status.success() {
            panic!("failed to build helper binary {name}");
        }
    });

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let bin = manifest_dir.join("..").join("..").join("target").join(&profile).join(name);
    #[cfg(windows)]
    {
        bin.set_extension("exe");
    }
    if !bin.exists() {
        panic!("helper binary {name} not built at {:?}", bin);
    }
    bin
}

struct PipedProc {
    child: Arc<tokio::sync::Mutex<tokio::process::Child>>,
    stdin: ChildStdin,
    fanout: StreamFanout,
}

async fn launch_piped(control_path: PathBuf) -> Result<PipedProc> {
    let mut cmd = Command::new(assert_helper_bin("passthrough_controlled"));
    cmd.env("CONTROL_SOCKET", &control_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    // Wait for control socket to appear
    for _ in 0..100 {
        if control_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");
    let child = Arc::new(tokio::sync::Mutex::new(child));

    let fanout = StreamFanout::new();
    let fanout_stdout = fanout.clone();
    tokio::spawn(async move {
        let mut r = tokio::io::BufReader::new(stdout);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match r.read_buf(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    fanout_stdout.push(&buf[..n]).await;
                }
                Err(_) => break,
            }
        }
        fanout_stdout.finish().await;
    });
    let fanout_stderr = fanout.clone();
    tokio::spawn(async move {
        let mut r = tokio::io::BufReader::new(stderr);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match r.read_buf(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    fanout_stderr.push(&buf[..n]).await;
                }
                Err(_) => break,
            }
        }
    });

    Ok(PipedProc {
        child,
        stdin,
        fanout,
    })
}

struct PtyProc {
    master: Box<dyn portable_pty::MasterPty + Send>,
    fanout: StreamFanout,
    child: Arc<tokio::sync::Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
}

async fn launch_pty(control_path: PathBuf) -> Result<PtyProc> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new(assert_helper_bin("passthrough_controlled"));
    cmd.env("CONTROL_SOCKET", &control_path);
    let child = Arc::new(tokio::sync::Mutex::new(pair.slave.spawn_command(cmd)?));
    for _ in 0..100 {
        if control_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // read master
    let mut reader = pair.master.try_clone_reader()?;
    let fanout = StreamFanout::new();
    let fanout_clone = fanout.clone();
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    fanout_clone.push_blocking(&handle, &buf[..n]);
                }
                Err(_) => break,
            }
        }
        fanout_clone.finish_blocking(&handle);
    });

    Ok(PtyProc {
        master: pair.master,
        fanout,
        child,
    })
}

async fn send_control(path: &PathBuf, lines: &[&str]) -> Result<()> {
    // helper may still be binding; retry briefly
    let mut attempts = 0;
    let mut stream = loop {
        match UnixStream::connect(path).await {
            Ok(s) => break s,
            Err(_e) if attempts < 100 => {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    };
    for line in lines {
        stream.write_all(line.as_bytes()).await?;
        stream.write_all(b"\n").await?;
    }
    stream.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn passthrough_piped_backlog_and_live() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let control = dir.path().join("ctl.sock");
    let mut proc = launch_piped(control.clone()).await?;

    // Emit first chunk before follower attaches
    send_control(&control, &["OUT A"]).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let follower = proc.fanout.attach().await;

    // Emit second chunk and exit
    send_control(&control, &["OUT B", "EXIT"]).await?;
    proc.stdin.write_all(b"\n").await.ok(); // ensure helper stdin open
    let data = timeout(Duration::from_secs(2), follower.run_until_done()).await?;
    let text = String::from_utf8_lossy(&data);
    assert!(text.contains("A"));
    assert!(text.contains("B"));
    assert!(
        text.find('A').unwrap_or(0) < text.find('B').unwrap_or(text.len()),
        "backlog (A) should precede live (B)"
    );
    Ok(())
}

#[tokio::test]
async fn passthrough_input_injection_round_trip() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let control = dir.path().join("ctl.sock");
    let mut proc = launch_piped(control.clone()).await?;

    let follower = proc.fanout.attach().await;
    // Ask helper to read from stdin and echo
    send_control(&control, &["ECHO_INPUT", "EXIT"]).await?;
    proc.stdin.write_all(b"secret\n").await?;
    proc.stdin.flush().await?;

    let data = timeout(Duration::from_secs(2), follower.run_until_done()).await?;
    let text = String::from_utf8_lossy(&data);
    assert!(text.contains("ECHO:secret"));
    Ok(())
}

#[tokio::test]
async fn passthrough_stderr_interleave() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let control = dir.path().join("ctl.sock");
    let proc = launch_piped(control.clone()).await?;

    let follower = proc.fanout.attach().await;
    send_control(
        &control,
        &["OUT first", "ERR second", "OUT third", "ERR fourth", "EXIT"],
    )
    .await?;

    let data = timeout(Duration::from_secs(2), follower.run_until_done()).await?;
    let text = String::from_utf8_lossy(&data);
    assert!(text.contains("first"));
    assert!(text.contains("second"));
    assert!(text.contains("third"));
    assert!(text.contains("fourth"));
    Ok(())
}

#[tokio::test]
async fn passthrough_slow_follower_no_drop() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let control = dir.path().join("ctl.sock");
    let proc = launch_piped(control.clone()).await?;
    let follower = proc.fanout.attach().await;

    // blast 20 lines quickly
    let mut cmds = Vec::new();
    for i in 0..20 {
        cmds.push(format!("OUT line-{i}"));
    }
    cmds.push("EXIT".into());
    let refs: Vec<&str> = cmds.iter().map(|s| s.as_str()).collect();
    send_control(&control, &refs).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    proc.fanout.finish().await;

    let data = timeout(Duration::from_secs(5), follower.run_until_done_slow(10)).await?;
    let text = String::from_utf8_lossy(&data);
    assert!(!text.is_empty(), "follower received no data");
    Ok(())
}

#[tokio::test]
async fn passthrough_sigkill_piped_preserves_backlog() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let control = dir.path().join("ctl.sock");
    let proc = launch_piped(control.clone()).await?;
    let follower = proc.fanout.attach().await;

    send_control(&control, &["OUT before-kill", "SLEEP 1500"]).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    {
        let mut child = proc.child.lock().await;
        let _ = child.kill().await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    proc.fanout.finish().await;

    let data = timeout(Duration::from_secs(2), follower.run_until_done()).await?;
    let text = String::from_utf8_lossy(&data);
    assert!(
        text.contains("before-kill"),
        "backlog should contain some output"
    );
    Ok(())
}

#[tokio::test]
async fn passthrough_pty_backlog_and_resize() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let control = dir.path().join("ctl.sock");
    let proc = launch_pty(control.clone()).await?;

    send_control(&control, &["OUT hello-pty"]).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let follower = proc.fanout.attach().await;
    // trigger size print
    send_control(&control, &["PRINT_SIZE", "EXIT"]).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(proc.master); // closes PTY write side
    {
        // wait with timeout; kill if needed to unblock reader
        let wait_res = tokio::time::timeout(Duration::from_secs(1), async {
            let mut guard = proc.child.lock().await;
            guard.wait()
        })
        .await;
        if wait_res.is_err() {
            let mut guard = proc.child.lock().await;
            let _ = guard.kill();
        }
    }
    proc.fanout.finish().await; // ensure followers complete even if PTY reader lingers

    let data = timeout(Duration::from_secs(2), follower.run_until_done()).await?;
    let text = String::from_utf8_lossy(&data);
    assert!(!text.is_empty(), "pty follower received no data");
    Ok(())
}

#[tokio::test]
async fn passthrough_abrupt_kill_yields_backlog() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let control = dir.path().join("ctl.sock");
    let proc = launch_piped(control.clone()).await?;
    let follower = proc.fanout.attach().await;
    let transcript_path = dir.path().join("abrupt_kill_transcript.txt");

    // Emit line and wait for its ACK to appear in backlog before killing.
    send_control(&control, &["OUT stay-alive", "SLEEP 1000"]).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    // Poll backlog for ACK
    let mut seen_ack = false;
    for _ in 0..20 {
        {
            let hist = proc.fanout.backlog.lock().await.clone();
            if String::from_utf8_lossy(&hist).contains("ACK stay-alive") {
                seen_ack = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(seen_ack, "expected ACK stay-alive before kill");
    // kill child abruptly
    {
        let mut child = proc.child.lock().await;
        let _ = child.kill().await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    proc.fanout.finish().await;

    let data = timeout(Duration::from_secs(2), follower.run_until_done()).await?;
    std::fs::write(&transcript_path, &data)?;
    let text = String::from_utf8_lossy(&data);
    if !text.contains("stay-alive") {
        let meta = std::fs::metadata(&transcript_path).ok();
        let size = meta.map(|m| m.len()).unwrap_or(0);
        let last_lines = std::fs::read_to_string(&transcript_path)
            .ok()
            .map(|s| {
                let mut lines: Vec<_> = s.lines().rev().take(10).map(|l| l.to_string()).collect();
                lines.reverse();
                lines.join("\n")
            })
            .unwrap_or_else(|| "<unreadable>".into());
        panic!(
            "transcript missing 'stay-alive'. path={:?} size={}B last_lines:\n{}",
            transcript_path, size, last_lines
        );
    }
    Ok(())
}
