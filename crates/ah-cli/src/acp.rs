// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Minimal `ah acp` bridge for Milestone 5.
//!
//! The command autodetects the preferred ACP endpoint (UDS by default),
//! forwards stdin→ACP and ACP→stdout with zero translation, and exposes
//! WebSocket/UDS selection flags aligned with the public CLI specification.

use agent_client_protocol::SessionNotification;
use ah_acp_bridge::{
    default_uds_path, ensure_uds_parent, notification_envelope, session_event_to_notification,
    session_snapshot_to_notification,
};
use ah_domain_types::AgentChoice;
use ah_rest_api_contract::{
    CreateTaskRequest, FilterQuery, RepoConfig, RepoMode, RuntimeConfig, RuntimeType, Session,
    SessionEvent, SessionListResponse, SessionPromptRequest, SessionPromptResponse,
};
use ah_rest_client::{
    RestClient,
    auth::{AuthConfig, AuthMethod},
};
use ah_rest_server::{
    ServerConfig,
    acp::AcpGateway,
    config::{AcpAuthPolicy, AcpTransportMode},
    dependencies::DefaultServerDependencies,
};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use clap::ValueEnum;
use futures_util::{SinkExt, StreamExt, stream::BoxStream};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(unix)]
use std::{
    os::unix::fs::PermissionsExt,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
};
use tokio::io::{
    self, AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader,
};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex as AsyncMutex;
#[cfg(unix)]
use tokio::time::{Duration, Instant};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::warn;
use url::Url;

#[derive(Debug, Clone, Copy, ValueEnum, Eq, PartialEq, Default)]
pub enum DaemonizeMode {
    #[default]
    Auto,
    Never,
    Disabled,
}

#[derive(Debug, clap::Args, Clone)]
pub struct AcpArgs {
    /// Access point WS URL or UDS path (defaults to platform ACP socket path)
    #[arg(long)]
    pub endpoint: Option<String>,

    /// Force WebSocket transport
    #[arg(long)]
    pub ws: bool,

    /// Force Unix socket transport
    #[arg(long)]
    pub uds: bool,

    /// Target Harbor REST server (resolves to `<url>/acp/v1/connect`)
    #[arg(long = "remote-server")]
    pub remote_server: Option<String>,

    /// Load and attach to an existing ACP session
    #[arg(long)]
    pub session: Option<String>,

    /// Behavior when no access point is running
    #[arg(long, default_value = "auto", value_enum)]
    pub daemonize: DaemonizeMode,

    /// Idle shutdown timer when daemonized (seconds)
    #[arg(long, default_value = "86400")]
    pub idle_timeout: u64,

    /// Internal: run as daemon child (spawned by `--daemonize=auto`)
    #[arg(long, hide = true)]
    pub daemon_child: bool,
}

enum ResolvedTransport {
    WebSocket(String),
    Unix(PathBuf),
    RestBridge(String),
}

#[cfg(unix)]
#[derive(Clone, Debug)]
enum DaemonUpstream {
    WebSocket(String),
    Rest(String),
}

impl AcpArgs {
    pub async fn run(self) -> Result<()> {
        #[cfg(unix)]
        if self.daemon_child {
            let uds = self
                .endpoint
                .clone()
                .or_else(|| self.endpoint_from_flags())
                .ok_or_else(|| anyhow::anyhow!("daemon child requires --uds path"))?;
            let upstream = infer_upstream(self.remote_server.as_deref()).ok_or_else(|| {
                anyhow::anyhow!("daemon child requires --remote-server or ws endpoint")
            })?;
            return run_access_point_daemon(PathBuf::from(uds), upstream, self.idle_timeout).await;
        }

        let transport = self.resolve_transport()?;
        match transport {
            ResolvedTransport::WebSocket(url) => forward_websocket(&url).await,
            ResolvedTransport::Unix(path) => {
                forward_uds(
                    &path,
                    self.daemonize,
                    self.idle_timeout,
                    self.remote_server.clone(),
                    self.session.as_deref(),
                )
                .await
            }
            ResolvedTransport::RestBridge(base) => {
                run_rest_bridge(
                    &base,
                    self.session.as_deref(),
                    self.daemonize,
                    self.idle_timeout,
                )
                .await
            }
        }
    }

    fn resolve_transport(&self) -> Result<ResolvedTransport> {
        // Explicit remote server takes precedence and maps to websocket.
        if let Some(remote) = &self.remote_server {
            let base = remote.trim_end_matches('/').to_string();
            if base.starts_with("http://") || base.starts_with("https://") {
                return Ok(ResolvedTransport::RestBridge(base));
            }
            let endpoint = format!("{}/acp/v1/connect", base);
            return Ok(ResolvedTransport::WebSocket(endpoint));
        }

        // User-supplied endpoint
        if let Some(endpoint) = &self.endpoint {
            if self.ws || endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
                return Ok(ResolvedTransport::WebSocket(endpoint.clone()));
            }
            if self.uds || Path::new(endpoint).is_absolute() {
                return Ok(ResolvedTransport::Unix(PathBuf::from(endpoint)));
            }
            if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
                return Ok(ResolvedTransport::RestBridge(endpoint.clone()));
            }
        }

        // Default: platform-specific UDS path.
        Ok(ResolvedTransport::Unix(default_uds_path()))
    }

    fn endpoint_from_flags(&self) -> Option<String> {
        self.endpoint.clone().or_else(|| {
            if self.uds {
                Some(default_uds_path().to_string_lossy().to_string())
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio_tungstenite::tungstenite::Message;

    #[test]
    fn default_autodiscovers_uds() {
        let args = AcpArgs {
            endpoint: None,
            ws: false,
            uds: false,
            remote_server: None,
            session: None,
            daemonize: DaemonizeMode::Auto,
            idle_timeout: 10,
            daemon_child: false,
        };

        let transport = args.resolve_transport().expect("transport");
        assert!(
            matches!(transport, ResolvedTransport::Unix(ref p) if p.file_name().and_then(|s| s.to_str()) == Some("acp.sock"))
        );
    }

    #[test]
    fn ws_flag_wins() {
        let args = AcpArgs {
            endpoint: Some("wss://example.test/acp".into()),
            ws: true,
            uds: false,
            remote_server: None,
            session: None,
            daemonize: DaemonizeMode::Auto,
            idle_timeout: 10,
            daemon_child: false,
        };

        let transport = args.resolve_transport().expect("transport");
        assert!(
            matches!(transport, ResolvedTransport::WebSocket(url) if url.starts_with("wss://example.test"))
        );
    }

    #[test]
    fn uds_path_for_absolute_endpoint() {
        let args = AcpArgs {
            endpoint: Some("/tmp/custom-acp.sock".into()),
            ws: false,
            uds: true,
            remote_server: None,
            session: None,
            daemonize: DaemonizeMode::Auto,
            idle_timeout: 10,
            daemon_child: false,
        };

        let transport = args.resolve_transport().expect("transport");
        assert!(
            matches!(transport, ResolvedTransport::Unix(path) if path == PathBuf::from("/tmp/custom-acp.sock"))
        );
    }

    #[test]
    fn http_endpoint_routes_to_rest_bridge() {
        let args = AcpArgs {
            endpoint: Some("https://harbor.example".into()),
            ws: false,
            uds: false,
            remote_server: None,
            session: None,
            daemonize: DaemonizeMode::Auto,
            idle_timeout: 10,
            daemon_child: false,
        };

        let transport = args.resolve_transport().expect("transport");
        assert!(
            matches!(transport, ResolvedTransport::RestBridge(url) if url == "https://harbor.example")
        );
    }

    #[tokio::test]
    async fn websocket_forwarding_round_trip() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let ws = tokio_tungstenite::accept_async(stream).await.expect("ws accept");
            let (mut sink, mut stream) = ws.split();
            while let Some(msg) = stream.next().await {
                let msg = msg.expect("message");
                if let Ok(text) = msg.to_text() {
                    sink.send(Message::Text(text.to_string())).await.expect("send");
                }
            }
        });

        let (client_side, bridge_side) = tokio::io::duplex(1024);
        let (bridge_reader, bridge_writer) = tokio::io::split(bridge_side);

        let forward = tokio::spawn(async move {
            forward_websocket_with_io(
                &format!("ws://{}/", addr),
                BufReader::new(bridge_reader),
                bridge_writer,
            )
            .await
            .unwrap();
        });

        let (client_reader, mut client_writer) = tokio::io::split(client_side);
        client_writer.write_all(b"hello\n").await.expect("write to bridge");

        let mut buf_reader = BufReader::new(client_reader);
        let mut line = String::new();
        buf_reader.read_line(&mut line).await.expect("read echoed line");
        assert_eq!(line.trim(), "hello");

        drop(client_writer);
        drop(buf_reader);
        forward.abort();
        server.abort();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn uds_forwarding_round_trip() {
        use tokio::net::UnixListener;

        let dir = tempfile::tempdir().expect("tmpdir");
        let sock = dir.path().join("acp-echo.sock");
        let listener = UnixListener::bind(&sock).expect("bind uds");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept uds");
            let (mut r, mut w) = stream.into_split();
            io::copy(&mut r, &mut w).await.expect("copy uds");
        });

        let (client_side, bridge_side) = tokio::io::duplex(1024);
        let (bridge_reader, bridge_writer) = tokio::io::split(bridge_side);
        let forward = tokio::spawn(async move {
            forward_uds_with_io(
                &sock,
                DaemonizeMode::Never,
                1,
                bridge_reader,
                bridge_writer,
                None,
            )
            .await
            .unwrap();
        });

        let (client_reader, mut client_writer) = tokio::io::split(client_side);
        client_writer.write_all(b"ping\n").await.expect("write uds");
        let mut buf_reader = BufReader::new(client_reader);
        let mut echoed = String::new();
        buf_reader.read_line(&mut echoed).await.expect("read uds echo");
        assert_eq!(echoed, "ping\n");

        drop(client_writer);
        drop(buf_reader);
        forward.abort();
        server.abort();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn auto_daemonizes_when_socket_missing() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let ws = tokio_tungstenite::accept_async(stream).await.expect("ws accept");
            let (mut sink, mut stream) = ws.split();
            while let Some(msg) = stream.next().await {
                let msg = msg.expect("message");
                if let Ok(text) = msg.to_text() {
                    sink.send(Message::Text(text.to_string())).await.expect("send");
                }
            }
        });

        let dir = tempfile::tempdir().expect("tmpdir");
        let sock = dir.path().join("auto-acp.sock");

        std::env::set_var("AH_ACP_TEST_INPROC_DAEMON", "1");

        let (client_side, bridge_side) = tokio::io::duplex(1024);
        let (bridge_reader, bridge_writer) = tokio::io::split(bridge_side);
        let forward = tokio::spawn(async move {
            forward_uds_with_io(
                &sock,
                DaemonizeMode::Auto,
                3,
                bridge_reader,
                bridge_writer,
                Some(format!("ws://{}/", addr)),
            )
            .await
            .unwrap();
        });

        let (client_reader, mut client_writer) = tokio::io::split(client_side);
        client_writer.write_all(b"ping\n").await.expect("write uds");
        let mut buf_reader = BufReader::new(client_reader);
        let mut echoed = String::new();
        buf_reader.read_line(&mut echoed).await.expect("read uds echo");
        assert_eq!(echoed.trim(), "ping");

        std::env::remove_var("AH_ACP_TEST_INPROC_DAEMON");

        forward.abort();
        server.abort();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ssh_style_port_forward_simulation() {
        use tokio::net::{TcpListener, TcpStream};

        // Upstream websocket echo server.
        let ws_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind ws");
        let ws_addr = ws_listener.local_addr().unwrap();
        let ws_server = tokio::spawn(async move {
            let (stream, _) = ws_listener.accept().await.expect("accept ws");
            let ws = tokio_tungstenite::accept_async(stream).await.expect("ws accept");
            let (mut sink, mut stream) = ws.split();
            while let Some(msg) = stream.next().await {
                let msg = msg.expect("message");
                if let Ok(text) = msg.to_text() {
                    sink.send(Message::Text(text.to_string())).await.expect("send");
                }
            }
        });

        let dir = tempfile::tempdir().expect("tmpdir");
        let uds_path = dir.path().join("bridge.sock");

        // Run daemon that forwards UDS ↔ WebSocket.
        std::env::set_var("AH_ACP_TEST_INPROC_DAEMON", "1");
        let daemon = tokio::spawn(run_access_point_daemon(
            uds_path.clone(),
            DaemonUpstream::WebSocket(format!("ws://{}/", ws_addr)),
            10,
        ));

        // Simulate ssh -L style TCP forwarder to the UDS.
        let fwd_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind fwd");
        let fwd_addr = fwd_listener.local_addr().unwrap();
        let uds_clone = uds_path.clone();
        let forwarder = tokio::spawn(async move {
            loop {
                let (mut tcp_stream, _) = match fwd_listener.accept().await {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let uds_clone = uds_clone.clone();
                tokio::spawn(async move {
                    if let Ok(uds_stream) = UnixStream::connect(&uds_clone).await {
                        let (mut uds_r, mut uds_w) = uds_stream.into_split();
                        let (mut tcp_r, mut tcp_w) = tcp_stream.split();
                        let _ = tokio::join!(
                            io::copy(&mut tcp_r, &mut uds_w),
                            io::copy(&mut uds_r, &mut tcp_w)
                        );
                    }
                });
            }
        });

        // Wait for daemon to bind socket.
        tokio::time::sleep(Duration::from_millis(200)).await;

        let mut client = TcpStream::connect(fwd_addr).await.expect("connect forward");
        client.write_all(b"hello\n").await.expect("write");
        let mut reader = BufReader::new(client);
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("read");
        assert_eq!(line.trim(), "hello");

        std::env::remove_var("AH_ACP_TEST_INPROC_DAEMON");

        forwarder.abort();
        daemon.abort();
        ws_server.abort();
    }

    #[derive(Clone)]
    struct FakeBridgeClient {
        sessions: Arc<Mutex<HashMap<String, Session>>>,
        streams: StreamSenders,
    }

    type StreamSenders = Arc<
        Mutex<HashMap<String, tokio::sync::mpsc::UnboundedSender<Result<SessionEvent, String>>>>,
    >;

    impl FakeBridgeClient {
        fn new() -> Self {
            Self {
                sessions: Arc::new(Mutex::new(HashMap::new())),
                streams: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn seed_session(&self, id: &str) -> Session {
            let session = sample_session(id);
            self.sessions.lock().unwrap().insert(id.to_string(), session.clone());
            session
        }
    }

    fn sample_session(id: &str) -> Session {
        Session {
            id: id.to_string(),
            tenant_id: None,
            project_id: None,
            task: ah_rest_api_contract::TaskInfo {
                prompt: "hello".into(),
                attachments: HashMap::new(),
                labels: HashMap::new(),
            },
            agent: AgentChoice {
                agent: ah_domain_types::AgentSoftwareBuild {
                    software: ah_domain_types::AgentSoftware::Claude,
                    version: "latest".into(),
                },
                model: "sonnet".into(),
                count: 1,
                settings: HashMap::new(),
                display_name: None,
                acp_stdio_launch_command: None,
            },
            runtime: RuntimeConfig {
                runtime_type: RuntimeType::Local,
                devcontainer_path: None,
                resources: None,
            },
            workspace: ah_rest_api_contract::WorkspaceInfo {
                snapshot_provider: "mock".into(),
                mount_path: "/workspace".into(),
                host: None,
                devcontainer_details: None,
            },
            vcs: ah_rest_api_contract::VcsInfo {
                repo_url: None,
                branch: None,
                commit: None,
            },
            status: ah_rest_api_contract::SessionStatus::Running,
            started_at: None,
            ended_at: None,
            links: ah_rest_api_contract::SessionLinks {
                self_link: "self".into(),
                events: "events".into(),
                logs: "logs".into(),
            },
        }
    }

    #[async_trait]
    impl BridgeClient for FakeBridgeClient {
        async fn list_sessions(
            &self,
            _filters: Option<&FilterQuery>,
        ) -> Result<ah_rest_api_contract::SessionListResponse, String> {
            let items = self.sessions.lock().unwrap().values().cloned().collect();
            Ok(ah_rest_api_contract::SessionListResponse {
                items,
                next_page: None,
                total: Some(2),
            })
        }

        async fn get_session(&self, session_id: &str) -> Result<Session, String> {
            self.sessions
                .lock()
                .unwrap()
                .get(session_id)
                .cloned()
                .ok_or_else(|| "missing session".into())
        }

        async fn create_task(
            &self,
            _request: &CreateTaskRequest,
        ) -> Result<ah_rest_api_contract::CreateTaskResponse, String> {
            let id = format!("sess-{}", self.sessions.lock().unwrap().len() + 1);
            let session = sample_session(&id);
            self.sessions.lock().unwrap().insert(id.clone(), session);
            Ok(ah_rest_api_contract::CreateTaskResponse {
                session_ids: vec![id],
                status: ah_rest_api_contract::SessionStatus::Running,
                links: ah_rest_api_contract::TaskLinks {
                    self_link: "self".into(),
                    events: "events".into(),
                    logs: "logs".into(),
                },
            })
        }

        async fn cancel_session(&self, session_id: &str) -> Result<(), String> {
            self.sessions.lock().unwrap().remove(session_id);
            Ok(())
        }

        async fn pause_session(&self, _session_id: &str) -> Result<(), String> {
            Ok(())
        }

        async fn resume_session(&self, _session_id: &str) -> Result<(), String> {
            Ok(())
        }

        async fn prompt_session(
            &self,
            session_id: &str,
            request: &SessionPromptRequest,
        ) -> Result<SessionPromptResponse, String> {
            if request.prompt.contains("fail") {
                return Err("prompt rejected".into());
            }
            Ok(SessionPromptResponse {
                session_id: session_id.to_string(),
                accepted: true,
                stop_reason: None,
                limit_chars: None,
                used_chars: None,
                current_chars: Some(request.prompt.len()),
                over_limit_by: None,
                remaining_chars: None,
            })
        }

        async fn stream_session_events(
            &self,
            session_id: &str,
        ) -> Result<BoxStream<'static, Result<SessionEvent, String>>, String> {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            self.streams.lock().unwrap().insert(session_id.to_string(), tx);
            Ok(Box::pin(
                tokio_stream::wrappers::UnboundedReceiverStream::new(rx),
            ))
        }
    }

    #[tokio::test]
    async fn rest_bridge_handles_basic_flows() {
        use tokio::io::AsyncWriteExt;

        let client = FakeBridgeClient::new();
        client.seed_session("s-1");
        client.seed_session("s-2");

        let (front, back) = tokio::io::duplex(4096);
        let (reader, writer) = tokio::io::split(back);
        let bridge = tokio::spawn(async move {
            run_rest_bridge_with_client(
                client.clone(),
                Some("s-1"),
                BufReader::new(reader),
                writer,
                DaemonizeMode::Auto,
                10,
            )
            .await
            .unwrap();
        });

        let (front_reader, mut front_writer) = tokio::io::split(front);
        let mut reader_front = BufReader::new(front_reader);

        async fn read_until_contains<R: AsyncBufRead + Unpin>(
            reader: &mut R,
            needle: &str,
        ) -> String {
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                if line.contains(needle) {
                    return line;
                }
            }
        }

        // initialize
        front_writer
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n")
            .await
            .unwrap();
        let init_line = read_until_contains(&mut reader_front, "\"id\":1").await;
        assert!(init_line.contains("\"transports\":[\"stdio\"]"));

        // session/list
        front_writer
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"session/list\"}\n")
            .await
            .unwrap();
        let list_line = read_until_contains(&mut reader_front, "\"id\":2").await;
        assert!(list_line.contains("\"items\""));

        // pause/resume
        front_writer
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"session/pause\",\"params\":{\"sessionId\":\"s-1\"}}\n")
            .await
            .unwrap();
        let pause_line = read_until_contains(&mut reader_front, "\"id\":4").await;
        assert!(pause_line.contains("\"result\":{}"));

        front_writer
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"session/resume\",\"params\":{\"sessionId\":\"s-1\"}}\n")
            .await
            .unwrap();
        let resume_line = read_until_contains(&mut reader_front, "\"id\":5").await;
        assert!(resume_line.contains("\"result\":{}"));

        front_writer
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"session/cancel\",\"params\":{\"sessionId\":\"s-2\"}}\n")
            .await
            .unwrap();
        let cancel_line = read_until_contains(&mut reader_front, "\"id\":6").await;
        assert!(cancel_line.contains("\"result\":{}"));

        // prompt error
        front_writer
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"session/prompt\",\"params\":{\"sessionId\":\"s-1\",\"prompt\":\"fail please\"}}\n")
            .await
            .unwrap();
        let err_line = read_until_contains(&mut reader_front, "\"id\":3").await;
        assert!(err_line.contains("prompt rejected"));

        bridge.abort();
    }
}

async fn forward_websocket(url: &str) -> Result<()> {
    let stdin = BufReader::new(io::stdin());
    let stdout = io::stdout();
    forward_websocket_with_io(url, stdin, stdout).await
}

#[cfg(unix)]
fn infer_upstream(remote: Option<&str>) -> Option<DaemonUpstream> {
    remote.map(|url| {
        if url.starts_with("http://") || url.starts_with("https://") {
            DaemonUpstream::Rest(url.to_string())
        } else {
            DaemonUpstream::WebSocket(url.to_string())
        }
    })
}

async fn forward_websocket_with_io<R, W>(url: &str, reader: R, writer: W) -> Result<()>
where
    R: AsyncBufRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (ws_stream, _) = connect_async(url)
        .await
        .with_context(|| format!("failed to connect to {url}"))?;
    let (mut sink, mut stream) = ws_stream.split();

    // Socket → writer
    let mut writer = writer;
    let stdout_task = tokio::spawn(async move {
        while let Some(msg) = stream.next().await {
            let msg = msg?;
            match msg {
                Message::Text(text) => {
                    writer.write_all(text.as_bytes()).await?;
                    writer.write_all(b"\n").await?;
                    writer.flush().await?;
                }
                Message::Binary(bin) => {
                    writer.write_all(&bin).await?;
                    writer.flush().await?;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        Ok::<(), tokio_tungstenite::tungstenite::Error>(())
    });

    // reader → socket
    let stdin_task = tokio::spawn(async move {
        let mut lines = reader.lines();
        while let Some(line) = lines.next_line().await? {
            sink.send(Message::Text(line)).await?;
        }
        Ok::<(), tokio_tungstenite::tungstenite::Error>(())
    });

    let (_, _) = tokio::try_join!(stdout_task, stdin_task)?;
    Ok(())
}

#[cfg(unix)]
async fn forward_uds(
    path: &Path,
    mode: DaemonizeMode,
    wait_secs: u64,
    remote_server: Option<String>,
    preload_session: Option<&str>,
) -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    match forward_uds_with_io(path, mode, wait_secs, stdin, stdout, remote_server.clone()).await {
        Ok(()) => Ok(()),
        Err(_err) if matches!(mode, DaemonizeMode::Never) => {
            // Inline hybrid fallback: prefer REST bridge when remote is provided, otherwise
            // spin up the stdio ACP gateway in-process using the shared access-point code.
            if let Some(base) = remote_server {
                run_rest_bridge(&base, preload_session, DaemonizeMode::Never, wait_secs).await
            } else {
                run_inline_access_point(wait_secs).await
            }
        }
        Err(err) => Err(err),
    }
}

#[cfg(not(unix))]
async fn forward_uds(
    path: &Path,
    _mode: DaemonizeMode,
    _wait_secs: u64,
    _remote_server: Option<String>,
    _preload_session: Option<&str>,
) -> Result<()> {
    bail!(
        "Unix socket transport is not supported on this platform (path: {})",
        path.display()
    );
}

#[cfg(unix)]
async fn forward_uds_with_io<R, W>(
    path: &Path,
    mode: DaemonizeMode,
    wait_secs: u64,
    mut reader: R,
    mut writer: W,
    remote_server: Option<String>,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    #[cfg(not(unix))]
    let _ = remote_server;

    if !path.exists() {
        match mode {
            DaemonizeMode::Disabled => {
                bail!(
                    "no ACP access point found at {} and daemonize=disabled",
                    path.display()
                );
            }
            DaemonizeMode::Never => {
                bail!(
                    "no ACP access point found at {} and daemonize=never; start one or use --remote-server",
                    path.display()
                );
            }
            DaemonizeMode::Auto => {
                #[cfg(unix)]
                if let Some(upstream) = infer_upstream(remote_server.as_deref()) {
                    let uds = path.to_path_buf();
                    spawn_access_point_daemon(uds, upstream, wait_secs).await?;
                }
                let deadline =
                    tokio::time::Instant::now() + tokio::time::Duration::from_secs(wait_secs);
                loop {
                    if path.exists() {
                        break;
                    }
                    if tokio::time::Instant::now() >= deadline {
                        bail!("timed out waiting for access point at {}", path.display());
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    let stream = UnixStream::connect(path)
        .await
        .with_context(|| format!("failed to connect to {}", path.display()))?;
    let (mut uds_reader, mut uds_writer) = stream.into_split();

    let stdout_task = tokio::spawn(async move {
        io::copy(&mut uds_reader, &mut writer).await?;
        writer.flush().await?;
        Ok::<_, io::Error>(())
    });

    let stdin_task = tokio::spawn(async move {
        io::copy(&mut reader, &mut uds_writer).await?;
        Ok::<_, io::Error>(())
    });

    let (_, _) = tokio::try_join!(stdout_task, stdin_task)?;
    Ok(())
}

#[cfg(unix)]
async fn run_access_point_daemon(
    path: PathBuf,
    upstream: DaemonUpstream,
    idle_timeout_secs: u64,
) -> Result<()> {
    ensure_uds_parent(&path)?;
    if path.exists() {
        let _ = tokio::fs::remove_file(&path).await;
    }

    let listener = UnixListener::bind(&path)?;
    let _cleanup = SocketCleanup(path.clone());
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));

    let idle = tokio::time::sleep(Duration::from_secs(idle_timeout_secs));
    tokio::pin!(idle);

    loop {
        tokio::select! {
            res = listener.accept() => {
                let (stream, _) = res?;
                idle.as_mut().reset(Instant::now() + Duration::from_secs(idle_timeout_secs));
                let upstream_clone = upstream.clone();
                let auth = rest_auth_from_env();
                tokio::spawn(async move {
                    if let Err(err) = handle_daemon_connection(stream, upstream_clone, auth, idle_timeout_secs).await {
                        warn!(?err, "daemon connection closed");
                    }
                });
            }
            _ = &mut idle => {
                warn!("ACP daemon idle timeout reached; shutting down");
                break;
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
struct SocketCleanup(PathBuf);

#[cfg(unix)]
impl Drop for SocketCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(unix)]
async fn spawn_access_point_daemon(
    path: PathBuf,
    upstream: DaemonUpstream,
    idle_timeout_secs: u64,
) -> Result<()> {
    if std::env::var("AH_ACP_TEST_INPROC_DAEMON").is_ok() {
        tokio::spawn(run_access_point_daemon(path, upstream, idle_timeout_secs));
        return Ok(());
    }

    let exe = std::env::current_exe().context("locating current executable")?;
    let mut cmd = Command::new(exe);
    cmd.arg("acp")
        .arg("--daemon-child")
        .arg("--uds")
        .arg(path.to_string_lossy().to_string())
        .arg("--idle-timeout")
        .arg(idle_timeout_secs.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    match upstream {
        DaemonUpstream::WebSocket(url) => {
            cmd.arg("--remote-server").arg(url);
        }
        DaemonUpstream::Rest(base) => {
            cmd.arg("--remote-server").arg(base);
        }
    }

    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    let _child = cmd.spawn().context("spawning access-point daemon")?;
    Ok(())
}

#[cfg(unix)]
async fn handle_daemon_connection(
    stream: UnixStream,
    upstream: DaemonUpstream,
    auth: AuthConfig,
    idle_timeout: u64,
) -> Result<()> {
    let (reader_half, writer_half) = stream.into_split();
    match upstream {
        DaemonUpstream::WebSocket(url) => {
            forward_websocket_with_io(&url, BufReader::new(reader_half), writer_half).await
        }
        DaemonUpstream::Rest(base) => {
            let client = RestClient::from_url(&base, auth)
                .with_context(|| format!("invalid REST base URL {base}"))?;
            run_rest_bridge_with_client(
                client,
                None,
                BufReader::new(reader_half),
                writer_half,
                DaemonizeMode::Auto,
                idle_timeout,
            )
            .await
        }
    }
}

/// Inline access-point fallback used when `--daemonize=never` and no daemon is reachable.
async fn run_inline_access_point(idle_timeout: u64) -> Result<()> {
    let mut config = ServerConfig::default();
    config.acp.enabled = true;
    config.acp.transport = AcpTransportMode::Stdio;
    config.acp.auth_policy = AcpAuthPolicy::Anonymous;
    config.acp.idle_timeout_secs = idle_timeout;
    config.acp.uds_path = None;

    let deps = DefaultServerDependencies::new(config.clone())
        .await
        .context("building inline access-point state")?;
    let state = deps.into_state();
    let gateway = AcpGateway::bind(config.acp.clone(), state)
        .await
        .context("binding inline ACP gateway")?;

    gateway.run().await.context("running inline ACP gateway over stdio")
}

fn rest_auth_from_env() -> AuthConfig {
    let api_key = std::env::var("AH_API_KEY").ok();
    let bearer = std::env::var("AH_JWT").ok().or_else(|| std::env::var("AH_BEARER").ok());
    if let Some(key) = api_key {
        AuthConfig {
            method: AuthMethod::api_key(key),
            tenant_id: None,
        }
    } else if let Some(token) = bearer {
        AuthConfig {
            method: AuthMethod::bearer(token),
            tenant_id: None,
        }
    } else {
        AuthConfig::default()
    }
}

/// Minimal REST→ACP bridge: maps a subset of ACP methods to REST API calls and
/// forwards REST session events as `session/update` notifications over the
/// provided I/O channels. The public CLI uses stdio; tests can inject in-memory
/// streams via `run_rest_bridge_with_client`.
async fn run_rest_bridge(
    base_url: &str,
    preload_session: Option<&str>,
    _daemonize: DaemonizeMode,
    _idle_timeout: u64,
) -> Result<()> {
    let auth = rest_auth_from_env();

    let client = RestClient::from_url(base_url, auth.clone())
        .with_context(|| format!("invalid base URL {base_url}"))?;
    let stdin = BufReader::new(io::stdin());
    let stdout = io::stdout();
    run_rest_bridge_with_client(
        client,
        preload_session,
        stdin,
        stdout,
        _daemonize,
        _idle_timeout,
    )
    .await
}

#[async_trait]
trait BridgeClient: Clone + Send + Sync + 'static {
    async fn list_sessions(
        &self,
        filters: Option<&FilterQuery>,
    ) -> Result<SessionListResponse, String>;
    async fn get_session(&self, session_id: &str) -> Result<Session, String>;
    async fn create_task(
        &self,
        request: &CreateTaskRequest,
    ) -> Result<ah_rest_api_contract::CreateTaskResponse, String>;
    async fn cancel_session(&self, session_id: &str) -> Result<(), String>;
    async fn pause_session(&self, session_id: &str) -> Result<(), String>;
    async fn resume_session(&self, session_id: &str) -> Result<(), String>;
    async fn prompt_session(
        &self,
        session_id: &str,
        request: &SessionPromptRequest,
    ) -> Result<SessionPromptResponse, String>;
    async fn stream_session_events(
        &self,
        session_id: &str,
    ) -> Result<BoxStream<'static, Result<SessionEvent, String>>, String>;
}

#[async_trait]
impl BridgeClient for RestClient {
    async fn list_sessions(
        &self,
        filters: Option<&FilterQuery>,
    ) -> Result<SessionListResponse, String> {
        RestClient::list_sessions(self, filters).await.map_err(|e| e.to_string())
    }

    async fn get_session(&self, session_id: &str) -> Result<Session, String> {
        RestClient::get_session(self, session_id).await.map_err(|e| e.to_string())
    }

    async fn create_task(
        &self,
        request: &CreateTaskRequest,
    ) -> Result<ah_rest_api_contract::CreateTaskResponse, String> {
        RestClient::create_task(self, request).await.map_err(|e| e.to_string())
    }

    async fn cancel_session(&self, session_id: &str) -> Result<(), String> {
        RestClient::cancel_session(self, session_id).await.map_err(|e| e.to_string())
    }

    async fn pause_session(&self, session_id: &str) -> Result<(), String> {
        RestClient::pause_session(self, session_id).await.map_err(|e| e.to_string())
    }

    async fn resume_session(&self, session_id: &str) -> Result<(), String> {
        RestClient::resume_session(self, session_id).await.map_err(|e| e.to_string())
    }

    async fn prompt_session(
        &self,
        session_id: &str,
        request: &SessionPromptRequest,
    ) -> Result<SessionPromptResponse, String> {
        RestClient::prompt_session(self, session_id, request)
            .await
            .map_err(|e| e.to_string())
    }

    async fn stream_session_events(
        &self,
        session_id: &str,
    ) -> Result<BoxStream<'static, Result<SessionEvent, String>>, String> {
        let stream = RestClient::stream_session_events(self, session_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Box::pin(stream.map(|item| item.map_err(|e| e.to_string()))))
    }
}

async fn run_rest_bridge_with_client<C, R, W>(
    client: C,
    preload_session: Option<&str>,
    reader: R,
    writer: W,
    _daemonize: DaemonizeMode,
    _idle_timeout: u64,
) -> Result<()>
where
    C: BridgeClient,
    R: AsyncBufRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let writer = Arc::new(AsyncMutex::new(writer));
    let subscriptions: Arc<AsyncMutex<Vec<tokio::task::JoinHandle<()>>>> =
        Arc::new(AsyncMutex::new(vec![]));

    if let Some(sess) = preload_session {
        spawn_session_stream(
            sess.to_string(),
            client.clone(),
            writer.clone(),
            subscriptions.clone(),
        )
        .await?;
        if let Ok(session) = client.get_session(sess).await {
            send_session_notification(&writer, session_snapshot_to_notification(&session))
                .await
                .ok();
        }
    }

    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(err) => {
                send_json_lines(&writer, json_error(Value::Null, -32700, &err.to_string()))
                    .await
                    .ok();
                continue;
            }
        };
        let id = value.get("id").cloned().unwrap_or(Value::Null);
        let method = value.get("method").and_then(|m| m.as_str());
        let params = value.get("params").cloned().unwrap_or(Value::Null);

        let result = match method {
            Some("initialize") => Ok(json!({
                "protocolVersion": "1.0",
                "capabilities": {
                    "loadSession": true,
                    "promptCapabilities": { "image": false, "audio": false, "embeddedContext": false, "meta": null },
                    "mcp": { "http": true, "sse": true, "meta": null },
                    "transports": ["stdio"],
                    "_meta": { "agent.harbor": { "transports": ["stdio"] } }
                },
                "authMethods": []
            })),
            Some("session/list") => {
                let filters: Option<FilterQuery> = serde_json::from_value(params.clone()).ok();
                client
                    .list_sessions(filters.as_ref())
                    .await
                    .map(|resp| json!({ "items": resp.items, "total": resp.total.unwrap_or(resp.items.len() as u32) }))
            }
            Some("session/load") => {
                if let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) {
                    let session =
                        client.get_session(session_id).await.map_err(anyhow::Error::msg)?;
                    spawn_session_stream(
                        session_id.to_string(),
                        client.clone(),
                        writer.clone(),
                        subscriptions.clone(),
                    )
                    .await
                    .ok();
                    send_session_notification(&writer, session_snapshot_to_notification(&session))
                        .await
                        .ok();
                    Ok(session_to_response(&session))
                } else {
                    Err("missing sessionId".into())
                }
            }
            Some("session/new") => match build_create_task(params.clone()) {
                Ok(req) => client.create_task(&req).await.map(|resp| {
                    let sid = resp.session_ids.first().cloned().unwrap_or_default();
                    tokio::spawn(spawn_session_stream(
                        sid.clone(),
                        client.clone(),
                        writer.clone(),
                        subscriptions.clone(),
                    ));
                    json!({ "sessionId": sid, "status": resp.status })
                }),
                Err(err) => Err(err),
            },
            Some("session/cancel") => {
                if let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) {
                    client.cancel_session(session_id).await.map(|_| json!({}))
                } else {
                    Err("missing sessionId".into())
                }
            }
            Some("session/pause") => {
                if let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) {
                    client.pause_session(session_id).await.map(|_| json!({}))
                } else {
                    Err("missing sessionId".into())
                }
            }
            Some("session/resume") => {
                if let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) {
                    client.resume_session(session_id).await.map(|_| json!({}))
                } else {
                    Err("missing sessionId".into())
                }
            }
            Some("session/prompt") => {
                if let Some(session_id) = params.get("sessionId").and_then(|v| v.as_str()) {
                    match prompt_from_params(&params) {
                        Ok(req) => client
                            .prompt_session(session_id, &req)
                            .await
                            .map(|resp| serde_json::to_value(resp).unwrap_or_else(|_| json!({}))),
                        Err(err) => Err(err),
                    }
                } else {
                    Err("missing sessionId".into())
                }
            }
            _ => Err("method not found".into()),
        };

        match result {
            Ok(res) => {
                send_json_lines(&writer, json!({"jsonrpc":"2.0","id":id,"result":res}))
                    .await
                    .ok();
            }
            Err(err) => {
                send_json_lines(&writer, json_error(id, -32000, &err)).await.ok();
            }
        }
    }

    // Graceful shutdown: drop subscriptions
    let mut subs = subscriptions.lock().await;
    for handle in subs.drain(..) {
        handle.abort();
    }

    Ok(())
}

async fn spawn_session_stream<C, W>(
    session_id: String,
    client: C,
    writer: Arc<AsyncMutex<W>>,
    subscriptions: Arc<AsyncMutex<Vec<tokio::task::JoinHandle<()>>>>,
) -> Result<()>
where
    C: BridgeClient,
    W: AsyncWrite + Unpin + Send + 'static,
{
    match client.stream_session_events(&session_id).await {
        Ok(mut stream) => {
            let writer_clone = writer.clone();
            let handle = tokio::spawn(async move {
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(event) => {
                            let notif = session_event_to_notification(&session_id, &event);
                            let _ = send_session_notification(&writer_clone, notif).await;
                        }
                        Err(err) => {
                            let _ = send_json_lines(
                                &writer_clone,
                                json_error(Value::Null, -32000, &err.to_string()),
                            )
                            .await;
                            break;
                        }
                    }
                }
            });
            subscriptions.lock().await.push(handle);
        }
        Err(_) => {
            if let Ok(session) = client.get_session(&session_id).await {
                send_session_notification(&writer, session_snapshot_to_notification(&session))
                    .await
                    .ok();
            }
        }
    }
    Ok(())
}

fn build_create_task(params: Value) -> Result<CreateTaskRequest, String> {
    let prompt = params
        .get("prompt")
        .or_else(|| params.get("message"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "prompt/message is required".to_string())?;

    let repo_meta = params
        .get("_meta")
        .and_then(|m| m.get("repoUrl"))
        .and_then(|v| v.as_str())
        .and_then(|s| Url::parse(s).ok());

    Ok(CreateTaskRequest {
        tenant_id: params.get("tenantId").and_then(|v| v.as_str()).map(|s| s.to_string()),
        project_id: params.get("projectId").and_then(|v| v.as_str()).map(|s| s.to_string()),
        prompt,
        repo: RepoConfig {
            mode: repo_meta.as_ref().map(|_| RepoMode::Git).unwrap_or(RepoMode::None),
            url: repo_meta,
            branch: params
                .get("_meta")
                .and_then(|m| m.get("branch"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            commit: None,
        },
        runtime: RuntimeConfig {
            runtime_type: RuntimeType::Local,
            devcontainer_path: None,
            resources: None,
        },
        workspace: None,
        agents: vec![AgentChoice {
            agent: ah_domain_types::AgentSoftwareBuild {
                software: ah_domain_types::AgentSoftware::Claude,
                version: "sonnet".into(),
            },
            model: "claude-3.5-sonnet".into(),
            count: 1,
            settings: Default::default(),
            display_name: None,
            acp_stdio_launch_command: None,
        }],
        delivery: None,
        labels: Default::default(),
        webhooks: Default::default(),
    })
}

fn session_to_response(session: &Session) -> Value {
    json!({
        "sessionId": session.id,
        "status": session.status,
        "workspace": {
            "mountPath": session.workspace.mount_path,
            "snapshotProvider": session.workspace.snapshot_provider,
            "readOnly": matches!(session.status, ah_rest_api_contract::SessionStatus::Paused)
        },
        "task": {
            "prompt": session.task.prompt,
            "labels": session.task.labels
        },
        "links": session.links
    })
}

fn prompt_from_params(params: &Value) -> Result<SessionPromptRequest, String> {
    let prompt = params
        .get("prompt")
        .or_else(|| params.get("message"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "prompt/message is required".to_string())?
        .to_string();

    Ok(SessionPromptRequest { prompt })
}

async fn send_json_lines<W>(writer: &Arc<AsyncMutex<W>>, payload: Value) -> Result<(), ()>
where
    W: AsyncWrite + Unpin + Send,
{
    let mut guard = writer.lock().await;
    let mut buf = serde_json::to_vec(&payload).map_err(|_| ())?;
    buf.push(b'\n');
    guard.write_all(&buf).await.map_err(|_| ())?;
    guard.flush().await.map_err(|_| ())
}

async fn send_session_notification<W>(
    writer: &Arc<AsyncMutex<W>>,
    notification: SessionNotification,
) -> Result<(), ()>
where
    W: AsyncWriteExt + Unpin + Send,
{
    send_json_lines(writer, notification_envelope(&notification)).await
}

fn json_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}
