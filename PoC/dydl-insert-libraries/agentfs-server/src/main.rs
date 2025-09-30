use agentfs_core::{
    BranchId, FsConfig, FsCore, FsLimits, MemoryPolicy, OpenOptions, PID,
};
use agentfs_proto::{FsRequest, FsResponse, FsDirEntry};
use serde::{Deserialize, Serialize};
use ssz::{Decode, Encode};
use ssz_derive::{Decode, Encode};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

// Legacy JSON types for backward compatibility
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum LegacyFsRequest {
    #[serde(rename = "fs.open")]
    Open { path: String, read: bool, write: bool, create: bool },
    #[serde(rename = "fs.create")]
    Create { path: String, read: bool, write: bool },
    #[serde(rename = "fs.close")]
    Close { handle: u64 },
    #[serde(rename = "fs.read")]
    Read { handle: u64, offset: u64, len: usize },
    #[serde(rename = "fs.write")]
    Write { handle: u64, offset: u64, data: Vec<u8> },
    #[serde(rename = "fs.getattr")]
    GetAttr { path: String },
    #[serde(rename = "fs.mkdir")]
    Mkdir { path: String },
    #[serde(rename = "fs.unlink")]
    Unlink { path: String },
    #[serde(rename = "fs.readdir")]
    ReadDir { path: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LegacyFsResponse {
    Handle { handle: u64 },
    Data { data: Vec<u8> },
    Written { len: usize },
    Attrs {
        len: u64,
        is_dir: bool,
        is_symlink: bool,
    },
    Entries(Vec<LegacyDirEntry>),
    Ok,
    Error { error: String, code: Option<i32> },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LegacyDirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub len: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LegacyMessage {
    pub version: String,
    #[serde(flatten)]
    pub body: LegacyFsRequest,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LegacyResponse {
    #[serde(flatten)]
    pub body: LegacyFsResponse,
}

struct AgentFsServer {
    core: Arc<FsCore>,
    process_id: PID,
    handle_map: Arc<Mutex<HashMap<u64, agentfs_core::HandleId>>>,
    next_local_handle: Arc<Mutex<u64>>,
}

impl AgentFsServer {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Create AgentFS configuration
        let config = FsConfig {
            case_sensitivity: agentfs_core::CaseSensitivity::Sensitive,
            limits: FsLimits {
                max_open_handles: 1000,
                max_branches: 10,
                max_snapshots: 10,
            },
            memory: MemoryPolicy {
                max_bytes_in_memory: Some(100 * 1024 * 1024), // 100MB
                spill_directory: None,
            },
            cache: Default::default(),
            security: Default::default(),
            enable_ads: false,
            enable_xattrs: false,
            track_events: false,
        };

        // Create FsCore instance
        let core = Arc::new(FsCore::new(config)?);

        // Register processes (PID 1 for the server, PID 1000 for clients)
        let server_process_id = core.register_process(1, 0, 0, 0);
        let client_process_id = core.register_process(1000, 0, 0, 0);

        // Create initial snapshot
        let snapshot_id = core.snapshot_create(Some("initial"))?;
        // Use the default branch which should already exist
        let branch_id = BranchId::DEFAULT;
        // Bind the server process to ensure it works
        core.bind_process_to_branch(branch_id)?;

        Ok(Self {
            core,
            process_id: client_process_id, // Use client PID for operations
            handle_map: Arc::new(Mutex::new(HashMap::new())),
            next_local_handle: Arc::new(Mutex::new(1)),
        })
    }

    fn map_error(err: agentfs_core::FsError) -> FsResponse {
        let (error_msg, code) = match err {
            agentfs_core::FsError::NotFound => ("Not found", libc::ENOENT),
            agentfs_core::FsError::AlreadyExists => ("Already exists", libc::EEXIST),
            agentfs_core::FsError::AccessDenied => ("Access denied", libc::EACCES),
            agentfs_core::FsError::InvalidArgument => ("Invalid argument", libc::EINVAL),
            agentfs_core::FsError::InvalidName => ("Invalid name", libc::EINVAL),
            agentfs_core::FsError::NotADirectory => ("Not a directory", libc::ENOTDIR),
            agentfs_core::FsError::IsADirectory => ("Is a directory", libc::EISDIR),
            agentfs_core::FsError::Busy => ("Resource busy", libc::EBUSY),
            agentfs_core::FsError::TooManyOpenFiles => ("Too many open files", libc::EMFILE),
            agentfs_core::FsError::NoSpace => ("No space left", libc::ENOSPC),
            agentfs_core::FsError::Unsupported => ("Unsupported operation", libc::ENOTSUP),
            agentfs_core::FsError::Io(_) => ("I/O error", libc::EIO),
        };

        FsResponse::error(error_msg.to_string(), Some(code as u32))
    }


    fn normalize_agentfs_path(&self, path: &[u8]) -> String {
        let mut path_str = String::from_utf8_lossy(path).to_string();
        // Strip the /agentfs/ prefix to get the path relative to AgentFS root
        if path_str.starts_with("/agentfs/") {
            path_str = path_str[8..].to_string(); // Remove "/agentfs" prefix
            if path_str.is_empty() {
                path_str = "/".to_string(); // Root directory
            }
        } else if path_str == "/agentfs" {
            path_str = "/".to_string(); // Root directory
        }
        // Ensure path starts with / for AgentFS
        if !path_str.starts_with('/') {
            path_str = "/".to_string() + &path_str;
        }
        path_str
    }

    async fn handle_ssz_request(&self, req: FsRequest) -> FsResponse {
        match req {
            FsRequest::Open(req) => {
                let path_str = self.normalize_agentfs_path(&req.path);
                let opts = OpenOptions {
                    read: req.read,
                    write: req.write,
                    create: false,
                    truncate: false,
                    append: false,
                    share: vec![],
                    stream: None,
                };

                match self.core.open(&self.process_id, Path::new(&path_str), &opts) {
                    Ok(handle_id) => {
                        let mut next_handle = self.next_local_handle.lock().unwrap();
                        let local_handle = *next_handle;
                        self.handle_map.lock().unwrap().insert(local_handle, handle_id);
                        *next_handle += 1;
                        FsResponse::handle(local_handle)
                    }
                    Err(err) => Self::map_ssz_error(err),
                }
            }

            FsRequest::Create(req) => {
                let path_str = self.normalize_agentfs_path(&req.path);
                let opts = OpenOptions {
                    read: req.read,
                    write: req.write,
                    create: true,
                    truncate: true,
                    append: false,
                    share: vec![],
                    stream: None,
                };

                match self.core.create(&self.process_id, Path::new(&path_str), &opts) {
                    Ok(handle_id) => {
                        let mut next_handle = self.next_local_handle.lock().unwrap();
                        let local_handle = *next_handle;
                        self.handle_map.lock().unwrap().insert(local_handle, handle_id);
                        *next_handle += 1;
                        FsResponse::handle(local_handle)
                    }
                    Err(err) => Self::map_ssz_error(err),
                }
            }

            FsRequest::Close(req) => {
                let mut handle_map = self.handle_map.lock().unwrap();
                if let Some(&agentfs_handle) = handle_map.get(&req.handle) {
                    match self.core.close(&self.process_id, agentfs_handle) {
                        Ok(()) => {
                            handle_map.remove(&req.handle);
                            FsResponse::ok()
                        }
                        Err(err) => Self::map_ssz_error(err),
                    }
                } else {
                    FsResponse::error("Invalid handle".to_string(), Some(libc::EBADF as u32))
                }
            }

            FsRequest::Read(req) => {
                let handle_map = self.handle_map.lock().unwrap();
                if let Some(&agentfs_handle) = handle_map.get(&req.handle) {
                    let mut buf = vec![0u8; req.len];
                    match self.core.read(&self.process_id, agentfs_handle, req.offset, &mut buf) {
                        Ok(bytes_read) => {
                            buf.truncate(bytes_read);
                            FsResponse::data(buf)
                        }
                        Err(err) => Self::map_ssz_error(err),
                    }
                } else {
                    FsResponse::error("Invalid handle".to_string(), Some(libc::EBADF as u32))
                }
            }

            FsRequest::Write(req) => {
                let handle_map = self.handle_map.lock().unwrap();
                if let Some(&agentfs_handle) = handle_map.get(&req.handle) {
                    match self.core.write(&self.process_id, agentfs_handle, req.offset, &req.data) {
                        Ok(bytes_written) => FsResponse::written(bytes_written),
                        Err(err) => Self::map_ssz_error(err),
                    }
                } else {
                    FsResponse::error("Invalid handle".to_string(), Some(libc::EBADF as u32))
                }
            }

            FsRequest::GetAttr(req) => {
                let path_str = self.normalize_agentfs_path(&req.path);
                match self.core.getattr(&self.process_id, Path::new(&path_str)) {
                    Ok(attrs) => FsResponse::attrs(attrs.len, attrs.is_dir, attrs.is_symlink),
                    Err(err) => Self::map_ssz_error(err),
                }
            }

            FsRequest::Mkdir(req) => {
                let path_str = self.normalize_agentfs_path(&req.path);
                match self.core.mkdir(&self.process_id, Path::new(&path_str), 0o755) {
                    Ok(()) => FsResponse::ok(),
                    Err(err) => Self::map_ssz_error(err),
                }
            }

            FsRequest::Unlink(req) => {
                let path_str = self.normalize_agentfs_path(&req.path);
                match self.core.unlink(&self.process_id, Path::new(&path_str)) {
                    Ok(()) => FsResponse::ok(),
                    Err(err) => Self::map_ssz_error(err),
                }
            }

            FsRequest::ReadDir(req) => {
                let path_str = self.normalize_agentfs_path(&req.path);
                match self.core.readdir_plus(&self.process_id, Path::new(&path_str)) {
                    Ok(entries_with_attrs) => {
                        let entries = entries_with_attrs
                            .into_iter()
                            .map(|(entry, _attrs)| FsDirEntry::new(
                                entry.name,
                                entry.is_dir,
                                entry.is_symlink,
                                entry.len,
                            ))
                            .collect();
                        FsResponse::entries(entries)
                    }
                    Err(err) => Self::map_ssz_error(err),
                }
            }
        }
    }

    fn map_ssz_error(err: agentfs_core::FsError) -> FsResponse {
        let (error_msg, code) = match err {
            agentfs_core::FsError::NotFound => ("Not found", libc::ENOENT),
            agentfs_core::FsError::AlreadyExists => ("Already exists", libc::EEXIST),
            agentfs_core::FsError::AccessDenied => ("Access denied", libc::EACCES),
            agentfs_core::FsError::InvalidArgument => ("Invalid argument", libc::EINVAL),
            agentfs_core::FsError::InvalidName => ("Invalid name", libc::EINVAL),
            agentfs_core::FsError::NotADirectory => ("Not a directory", libc::ENOTDIR),
            agentfs_core::FsError::IsADirectory => ("Is a directory", libc::EISDIR),
            agentfs_core::FsError::Busy => ("Resource busy", libc::EBUSY),
            agentfs_core::FsError::TooManyOpenFiles => ("Too many open files", libc::EMFILE),
            agentfs_core::FsError::NoSpace => ("No space left", libc::ENOSPC),
            agentfs_core::FsError::Unsupported => ("Unsupported operation", libc::ENOTSUP),
            agentfs_core::FsError::Io(_) => ("I/O error", libc::EIO),
        };

        FsResponse::error(error_msg.to_string(), Some(code as u32))
    }
}

fn convert_legacy_to_ssz(legacy: LegacyFsRequest) -> FsRequest {
    match legacy {
        LegacyFsRequest::Open { path, read, write, create: _ } => {
            FsRequest::open(path, read, write)
        }
        LegacyFsRequest::Create { path, read, write } => {
            FsRequest::create(path, read, write)
        }
        LegacyFsRequest::Close { handle } => {
            FsRequest::close(handle)
        }
        LegacyFsRequest::Read { handle, offset, len } => {
            FsRequest::read(handle, offset as u64, len)
        }
        LegacyFsRequest::Write { handle, offset, data } => {
            FsRequest::write(handle, offset as u64, data)
        }
        LegacyFsRequest::GetAttr { path } => {
            FsRequest::getattr(path)
        }
        LegacyFsRequest::Mkdir { path } => {
            FsRequest::mkdir(path)
        }
        LegacyFsRequest::Unlink { path } => {
            FsRequest::unlink(path)
        }
        LegacyFsRequest::ReadDir { path } => {
            FsRequest::readdir(path)
        }
    }
}

fn convert_ssz_to_legacy(ssz: FsResponse) -> LegacyFsResponse {
    match ssz {
        FsResponse::Handle(resp) => LegacyFsResponse::Handle { handle: resp.handle },
        FsResponse::Data(resp) => LegacyFsResponse::Data { data: resp.data },
        FsResponse::Written(resp) => LegacyFsResponse::Written { len: resp.len },
        FsResponse::Attrs(resp) => LegacyFsResponse::Attrs {
            len: resp.len,
            is_dir: resp.is_dir,
            is_symlink: resp.is_symlink,
        },
        FsResponse::Entries(resp) => LegacyFsResponse::Entries(
            resp.entries.into_iter()
                .map(|entry| LegacyDirEntry {
                    name: String::from_utf8_lossy(&entry.name).to_string(),
                    is_dir: entry.is_dir,
                    is_symlink: entry.is_symlink,
                    len: entry.len,
                })
                .collect()
        ),
        FsResponse::Ok(_) => LegacyFsResponse::Ok,
        FsResponse::Error(resp) => LegacyFsResponse::Error {
            error: String::from_utf8_lossy(&resp.error).to_string(),
            code: resp.code.map(|c| c as i32),
        },
    }
}

async fn handle_json_client(mut socket: tokio::net::UnixStream, server: Arc<AgentFsServer>) {
    let server_ref = &*server;
    loop {
        // Read message length (4 bytes, big-endian)
        let mut len_buf = [0u8; 4];
        if socket.read_exact(&mut len_buf).await.is_err() {
            return; // Connection closed or error
        }
        let msg_len = u32::from_be_bytes(len_buf) as usize;

        // Read message
        let mut msg_buf = vec![0u8; msg_len];
        if socket.read_exact(&mut msg_buf).await.is_err() {
            return; // Connection closed or error
        }

        // Parse request
        if let Ok(msg_str) = std::str::from_utf8(&msg_buf) {
            eprintln!("[JSON] Server received request: {}", msg_str);
            match serde_json::from_str::<LegacyMessage>(msg_str) {
                Ok(message) => {
                    // Convert legacy request to SSZ request
                    let ssz_request = convert_legacy_to_ssz(message.body);
                    let response = server_ref.handle_ssz_request(ssz_request).await;

                    // Convert SSZ response to legacy response
                    let legacy_response = convert_ssz_to_legacy(response);

                    // Serialize response
                    let response_msg = LegacyResponse { body: legacy_response };
                    match serde_json::to_string(&response_msg) {
                        Ok(json) => {
                            let data = json.as_bytes();

                            // Send response length + data
                            let len_bytes = (data.len() as u32).to_be_bytes();
                            if socket.write_all(&len_bytes).await.is_err() {
                                eprintln!("Failed to send response length");
                                return;
                            }
                            if socket.write_all(data).await.is_err() {
                                eprintln!("Failed to send response data");
                                return;
                            }
                            eprintln!("[JSON] Server sent response: {}", json);
                        }
                        Err(e) => {
                            eprintln!("Failed to serialize response: {}", e);
                            return;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to parse request JSON: {}", e);
                    return;
                }
            }
        } else {
            eprintln!("Failed to decode request as UTF-8");
            return;
        }
    }
}

async fn handle_ssz_client(mut socket: tokio::net::UnixStream, server: Arc<AgentFsServer>) {
    let server_ref = &*server;
    loop {
        // Read message length (4 bytes, big-endian)
        let mut len_buf = [0u8; 4];
        if socket.read_exact(&mut len_buf).await.is_err() {
            return; // Connection closed or error
        }
        let msg_len = u32::from_be_bytes(len_buf) as usize;

        // Read SSZ message
        let mut msg_buf = vec![0u8; msg_len];
        if socket.read_exact(&mut msg_buf).await.is_err() {
            return; // Connection closed or error
        }

        // Parse SSZ request
        eprintln!("[SSZ] Server received request ({} bytes)", msg_len);
        match FsRequest::from_ssz_bytes(&msg_buf) {
            Ok(request) => {
                let response = server_ref.handle_ssz_request(request).await;

                // Serialize SSZ response
                let response_bytes = response.as_ssz_bytes();

                // Send response length + SSZ data
                let len_bytes = (response_bytes.len() as u32).to_be_bytes();
                if socket.write_all(&len_bytes).await.is_err() {
                    eprintln!("Failed to send response length");
                    return;
                }
                if socket.write_all(&response_bytes).await.is_err() {
                    eprintln!("Failed to send response data");
                    return;
                }
                eprintln!("[SSZ] Server sent response ({} bytes)", response_bytes.len());
            }
            Err(e) => {
                eprintln!("Failed to parse SSZ request: {:?}", e);
                return;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = "/tmp/agentfs.sock";

    // Create the AgentFS server
    let server = Arc::new(AgentFsServer::new()?);
    println!("AgentFS server initialized with PID binding");

    // Bind to Unix sockets for both protocols
    let json_socket_path = format!("{}.json", socket_path);
    let ssz_socket_path = format!("{}.ssz", socket_path);

    // Clean up existing sockets
    if tokio::fs::metadata(&json_socket_path).await.is_ok() {
        tokio::fs::remove_file(&json_socket_path).await?;
    }
    if tokio::fs::metadata(&ssz_socket_path).await.is_ok() {
        tokio::fs::remove_file(&ssz_socket_path).await?;
    }

    let json_listener = UnixListener::bind(&json_socket_path)?;
    let ssz_listener = UnixListener::bind(&ssz_socket_path)?;

    println!("AgentFS server listening on:");
    println!("  JSON protocol: {}", json_socket_path);
    println!("  SSZ protocol:  {}", ssz_socket_path);

    loop {
        tokio::select! {
            result = json_listener.accept() => {
                match result {
                    Ok((socket, _addr)) => {
                        let server_clone = Arc::clone(&server);
                        tokio::spawn(async move {
                            handle_json_client(socket, server_clone).await;
                        });
                    }
                    Err(e) => {
                        eprintln!("JSON accept error: {}", e);
                    }
                }
            }
            result = ssz_listener.accept() => {
                match result {
                    Ok((socket, _addr)) => {
                        let server_clone = Arc::clone(&server);
                        tokio::spawn(async move {
                            handle_ssz_client(socket, server_clone).await;
                        });
                    }
                    Err(e) => {
                        eprintln!("SSZ accept error: {}", e);
                    }
                }
            }
        }
    }
}
