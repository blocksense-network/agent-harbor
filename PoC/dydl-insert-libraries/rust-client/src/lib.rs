use agentfs_proto::{FsRequest, FsResponse};
use libc::{c_char, c_int, c_void, mode_t, size_t, ssize_t};
use ssz::{Decode, Encode};
use std::ffi::CStr;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use lazy_static::lazy_static;

// Environment variable names
const ENV_AGENTFS_SERVER: &str = "AGENTFS_SERVER";
const ENV_AGENTFS_ENABLED: &str = "AGENTFS_ENABLED";

// Global state
lazy_static! {
    static ref AGENTFS_ENABLED: bool = std::env::var(ENV_AGENTFS_ENABLED)
        .map(|v| v == "1")
        .unwrap_or(false);
    static ref AGENTFS_SERVER: Option<String> = std::env::var(ENV_AGENTFS_SERVER).ok();
}

// Simplified synchronous client for PoC
// In a real implementation, this would be async
#[derive(Clone)]
struct SyncAgentFsClient {
    socket_path: String,
}

impl SyncAgentFsClient {
    fn new(socket_path: &str) -> Self {
        Self {
            socket_path: socket_path.to_string(),
        }
    }

    fn send_request(&self, request: FsRequest) -> Result<FsResponse, Box<dyn std::error::Error>> {
        let mut stream = UnixStream::connect(&self.socket_path)?;

        // Encode request as SSZ bytes
        let request_bytes = request.as_ssz_bytes();

        // Send length prefix + SSZ data
        let len = request_bytes.len() as u32;
        stream.write_all(&len.to_be_bytes())?;
        stream.write_all(&request_bytes)?;

        // Read response length
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf)?;
        let resp_len = u32::from_be_bytes(len_buf) as usize;

        // Read response SSZ bytes
        let mut resp_buf = vec![0u8; resp_len];
        stream.read_exact(&mut resp_buf)?;

        // Decode response from SSZ
        let response = FsResponse::from_ssz_bytes(&resp_buf)
            .map_err(|e| format!("SSZ decode error: {:?}", e))?;

        Ok(response)
    }

    fn open(&self, path: &str, read: bool, write: bool, _create: bool) -> Result<i32, Box<dyn std::error::Error>> {
        let request = FsRequest::open(path.to_string(), read, write);
        match self.send_request(request)? {
            FsResponse::Handle(resp) => Ok(resp.handle as i32),
            FsResponse::Error(resp) => Err(String::from_utf8_lossy(&resp.error).to_string().into()),
            _ => Err("Unexpected response type".into()),
        }
    }

    fn close(&self, handle: i32) -> Result<(), Box<dyn std::error::Error>> {
        let request = FsRequest::close(handle as u64);
        match self.send_request(request)? {
            FsResponse::Ok(_) => Ok(()),
            FsResponse::Error(resp) => Err(String::from_utf8_lossy(&resp.error).to_string().into()),
            _ => Err("Unexpected response type".into()),
        }
    }

    fn read(&self, handle: i32, count: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let request = FsRequest::read(handle as u64, 0, count); // offset 0 for simplicity
        match self.send_request(request)? {
            FsResponse::Data(resp) => Ok(resp.data),
            FsResponse::Error(resp) => Err(String::from_utf8_lossy(&resp.error).to_string().into()),
            _ => Err("Unexpected response type".into()),
        }
    }

    fn write(&self, handle: i32, offset: u64, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let request = FsRequest::write(handle as u64, offset, data.to_vec());
        match self.send_request(request)? {
            FsResponse::Ok(_) => Ok(()),
            FsResponse::Error(resp) => Err(String::from_utf8_lossy(&resp.error).to_string().into()),
            _ => Err("Unexpected response type".into()),
        }
    }

    fn getattr(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let request = FsRequest::getattr(path.to_string());
        match self.send_request(request)? {
            FsResponse::Attrs(_) => Ok(()),
            FsResponse::Error(resp) => Err(String::from_utf8_lossy(&resp.error).to_string().into()),
            _ => Err("Unexpected response type".into()),
        }
    }

    fn mkdir(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let request = FsRequest::mkdir(path.to_string());
        match self.send_request(request)? {
            FsResponse::Ok(_) => Ok(()),
            FsResponse::Error(resp) => Err(String::from_utf8_lossy(&resp.error).to_string().into()),
            _ => Err("Unexpected response type".into()),
        }
    }

    fn unlink(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let request = FsRequest::unlink(path.to_string());
        match self.send_request(request)? {
            FsResponse::Ok(_) => Ok(()),
            FsResponse::Error(resp) => Err(String::from_utf8_lossy(&resp.error).to_string().into()),
            _ => Err("Unexpected response type".into()),
        }
    }

    fn readdir(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let request = FsRequest::readdir(path.to_string());
        match self.send_request(request)? {
            FsResponse::Entries(_) => Ok(()),
            FsResponse::Error(resp) => Err(String::from_utf8_lossy(&resp.error).to_string().into()),
            _ => Err("Unexpected response type".into()),
        }
    }
}

// Thread-local client and handle mapping
thread_local! {
    static CLIENT: std::cell::RefCell<Option<SyncAgentFsClient>> = std::cell::RefCell::new(None);
    static HANDLE_MAP: std::cell::RefCell<HashMap<i32, i32>> = std::cell::RefCell::new(HashMap::new()); // Local fd -> AgentFS handle
    static NEXT_LOCAL_HANDLE: std::cell::RefCell<i32> = std::cell::RefCell::new(1000);
}

// Check if path should be intercepted
fn should_intercept(path: &str) -> bool {
    path.starts_with("/agentfs/")
}

// Helper to get or create client
fn get_client() -> Option<SyncAgentFsClient> {
    CLIENT.with(|client_cell| {
        let mut client_opt = client_cell.borrow_mut();
        if client_opt.is_none() {
            if let Some(socket_path) = &*AGENTFS_SERVER {
                // Use SSZ socket for Rust client
                let ssz_socket_path = format!("{}.ssz", socket_path);
                *client_opt = Some(SyncAgentFsClient::new(&ssz_socket_path));
            }
        }
        client_opt.clone()
    })
}

// Helper to get next local handle
fn get_next_local_handle() -> i32 {
    NEXT_LOCAL_HANDLE.with(|handle_cell| {
        let mut handle = handle_cell.borrow_mut();
        let current = *handle;
        *handle += 1;
        current
    })
}

// Helper to map handles
fn add_handle_mapping(local_fd: i32, agentfs_handle: i32) {
    HANDLE_MAP.with(|map_cell| {
        map_cell.borrow_mut().insert(local_fd, agentfs_handle);
    });
}

fn get_agentfs_handle(local_fd: i32) -> Option<i32> {
    HANDLE_MAP.with(|map_cell| {
        map_cell.borrow().get(&local_fd).cloned()
    })
}

fn remove_handle_mapping(local_fd: i32) {
    HANDLE_MAP.with(|map_cell| {
        map_cell.borrow_mut().remove(&local_fd);
    });
}

// Interposed functions using redhook
redhook::hook! {
    unsafe fn open(path: *const c_char, flags: c_int, mode: mode_t) -> c_int => my_open {
        if let Some(path_str) = path.as_ref().and_then(|p| CStr::from_ptr(p).to_str().ok()) {
            if should_intercept(path_str) {
                if let Some(client) = get_client() {
                    eprintln!("[RUST-FS-INTERPOSE] Intercepting open: {}", path_str);

                    // Determine read/write mode from flags
                    let read = (flags & libc::O_RDONLY != 0) || (flags & libc::O_RDWR != 0);
                    let write = (flags & libc::O_WRONLY != 0) || (flags & libc::O_RDWR != 0);
                    let create = flags & libc::O_CREAT != 0;

                    match client.open(path_str, read, write, create) {
                        Ok(agentfs_handle) => {
                            let local_fd = get_next_local_handle();
                            add_handle_mapping(local_fd, agentfs_handle);
                            return local_fd;
                        }
                        Err(e) => {
                            eprintln!("[RUST-FS-INTERPOSE] AgentFS open failed: {}", e);
                            // Fall back to original
                        }
                    }
                }
            }
        }

        // Call original function
        redhook::real!(open)(path, flags, mode)
    }
}

redhook::hook! {
    unsafe fn close(fd: c_int) -> c_int => my_close {
        if let Some(agentfs_handle) = get_agentfs_handle(fd) {
            if let Some(client) = get_client() {
                eprintln!("[RUST-FS-INTERPOSE] Intercepting close: {}", fd);

                match client.close(agentfs_handle) {
                    Ok(_) => {
                        remove_handle_mapping(fd);
                        return 0;
                    }
                    Err(e) => {
                        eprintln!("[RUST-FS-INTERPOSE] AgentFS close failed: {}", e);
                        // Fall back to original
                    }
                }
            }
        }

        // Call original function
        redhook::real!(close)(fd)
    }
}

redhook::hook! {
    unsafe fn read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t => my_read {
        if let Some(agentfs_handle) = get_agentfs_handle(fd) {
            if let Some(client) = get_client() {
                eprintln!("[RUST-FS-INTERPOSE] Intercepting read: {}", fd);

                match client.read(agentfs_handle, count) {
                    Ok(data) => {
                        let len = data.len();
                        if len > 0 && !buf.is_null() {
                            std::ptr::copy_nonoverlapping(data.as_ptr(), buf as *mut u8, len);
                        }
                        return len as ssize_t;
                    }
                    Err(e) => {
                        eprintln!("[RUST-FS-INTERPOSE] AgentFS read failed: {}", e);
                        // Fall back to original
                    }
                }
            }
        }

        // Call original function
        redhook::real!(read)(fd, buf, count)
    }
}

redhook::hook! {
    unsafe fn write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t => my_write {
        if let Some(agentfs_handle) = get_agentfs_handle(fd) {
            if let Some(client) = get_client() {
                eprintln!("[RUST-FS-INTERPOSE] Intercepting write: {}", fd);

                let data = std::slice::from_raw_parts(buf as *const u8, count);
                match client.write(agentfs_handle, 0, data) {
                    Ok(_) => return count as ssize_t,
                    Err(e) => {
                        eprintln!("[RUST-FS-INTERPOSE] AgentFS write failed: {}", e);
                        // Fall back to original
                    }
                }
            }
        }

        // Call original function
        redhook::real!(write)(fd, buf, count)
    }
}

// Library initialization
#[ctor::ctor]
fn init() {
    eprintln!("[RUST-FS-INTERPOSE] Library loaded");
    if *AGENTFS_ENABLED {
        eprintln!("[RUST-FS-INTERPOSE] AgentFS interception enabled");
        if let Some(server) = &*AGENTFS_SERVER {
            eprintln!("[RUST-FS-INTERPOSE] Server: {}", server);
        }
    } else {
        eprintln!("[RUST-FS-INTERPOSE] AgentFS interception disabled");
    }
}

#[ctor::dtor]
fn fini() {
    eprintln!("[RUST-FS-INTERPOSE] Library unloaded");
}
