use agentfs_proto::{FsRequest, FsResponse, FsDirEntry};
use ssz::{Encode, Decode};
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::{Result, anyhow};

// Re-export agentfs_core types for compatibility
pub use agentfs_core;

// Re-export types for convenience
pub use agentfs_proto::{FsRequest as Request, FsResponse as Response};
pub type HandleId = u64;

pub struct AgentFsClient {
    stream: UnixStream,
    next_handle: HandleId,
    handle_map: HashMap<i32, HandleId>, // Map local fd to AgentFS handle
}

impl AgentFsClient {
    pub async fn connect(path: &str) -> Result<Self> {
        let stream = UnixStream::connect(path).await?;
        Ok(Self {
            stream,
            next_handle: 1,
            handle_map: HashMap::new(),
        })
    }

    async fn send_request(&mut self, req: FsRequest) -> Result<FsResponse> {
        // Encode request as SSZ bytes
        let request_bytes = req.as_ssz_bytes();

        // Send length prefix + SSZ data
        let len = request_bytes.len() as u32;
        self.stream.write_all(&len.to_be_bytes()).await?;
        self.stream.write_all(&request_bytes).await?;

        // Read response length
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let resp_len = u32::from_be_bytes(len_buf) as usize;

        // Read response SSZ bytes
        let mut resp_buf = vec![0u8; resp_len];
        self.stream.read_exact(&mut resp_buf).await?;

        // Decode response from SSZ
        let response = FsResponse::from_ssz_bytes(&resp_buf)
            .map_err(|e| anyhow!("SSZ decode error"))?;

        Ok(response)
    }

    pub async fn open(&mut self, path: &Path, flags: i32) -> Result<i32> {
        let path_str = path.to_string_lossy().to_string();

        // Map POSIX flags to our simplified options
        let read = (flags & libc::O_RDONLY) != 0 || (flags & libc::O_RDWR) != 0;
        let write = (flags & libc::O_WRONLY) != 0 || (flags & libc::O_RDWR) != 0;
        let create = (flags & libc::O_CREAT) != 0;

        let req = if create {
            FsRequest::create(path_str, read, write)
        } else {
            FsRequest::open(path_str, read, write)
        };

        match self.send_request(req).await? {
            FsResponse::Handle(resp) => {
                let local_fd = self.next_handle as i32;
                self.handle_map.insert(local_fd, resp.handle);
                self.next_handle += 1;
                Ok(local_fd)
            }
            FsResponse::Error(resp) => {
                let error = String::from_utf8_lossy(&resp.error);
                Err(anyhow!("Open failed: {}", error))
            }
            _ => Err(anyhow!("Unexpected response type")),
        }
    }

    pub async fn close(&mut self, fd: i32) -> Result<()> {
        if let Some(handle) = self.handle_map.remove(&fd) {
            let req = FsRequest::close(handle);
            match self.send_request(req).await? {
                FsResponse::Ok(_) => Ok(()),
                FsResponse::Error(resp) => {
                    let error = String::from_utf8_lossy(&resp.error);
                    Err(anyhow!("Close failed: {}", error))
                }
                _ => Err(anyhow!("Unexpected response type")),
            }
        } else {
            Ok(()) // Not our file descriptor
        }
    }

    pub async fn read(&mut self, fd: i32, buf: &mut [u8], offset: i64) -> Result<usize> {
        if let Some(&handle) = self.handle_map.get(&fd) {
            let req = FsRequest::read(handle, offset as u64, buf.len());

            match self.send_request(req).await? {
                FsResponse::Data(resp) => {
                    let len = std::cmp::min(resp.data.len(), buf.len());
                    buf[..len].copy_from_slice(&resp.data[..len]);
                    Ok(len)
                }
                FsResponse::Error(resp) => {
                    let error = String::from_utf8_lossy(&resp.error);
                    Err(anyhow!("Read failed: {}", error))
                }
                _ => Err(anyhow!("Unexpected response type")),
            }
        } else {
            Err(anyhow!("File descriptor not managed by AgentFS"))
        }
    }

    pub async fn write(&mut self, fd: i32, buf: &[u8], offset: i64) -> Result<usize> {
        if let Some(&handle) = self.handle_map.get(&fd) {
            let req = FsRequest::write(handle, offset as u64, buf.to_vec());

            match self.send_request(req).await? {
                FsResponse::Written(resp) => Ok(resp.len),
                FsResponse::Error(resp) => {
                    let error = String::from_utf8_lossy(&resp.error);
                    Err(anyhow!("Write failed: {}", error))
                }
                _ => Err(anyhow!("Unexpected response type")),
            }
        } else {
            Err(anyhow!("File descriptor not managed by AgentFS"))
        }
    }

    pub async fn getattr(&mut self, path: &Path) -> Result<agentfs_core::Attributes> {
        let path_str = path.to_string_lossy().to_string();
        let req = FsRequest::getattr(path_str);

        match self.send_request(req).await? {
            FsResponse::Attrs(resp) => {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
                Ok(agentfs_core::Attributes {
                    len: resp.len,
                    times: agentfs_core::FileTimes {
                        atime: now,
                        mtime: now,
                        ctime: now,
                        birthtime: now,
                    },
                    uid: 0, // Not available in simplified SSZ response
                    gid: 0, // Not available in simplified SSZ response
                    is_dir: resp.is_dir,
                    is_symlink: resp.is_symlink,
                    mode_user: agentfs_core::FileMode {
                        read: true,
                        write: resp.is_dir,
                        exec: resp.is_dir,
                    },
                    mode_group: agentfs_core::FileMode {
                        read: true,
                        write: false,
                        exec: resp.is_dir,
                    },
                    mode_other: agentfs_core::FileMode {
                        read: true,
                        write: false,
                        exec: false,
                    },
                })
            }
            FsResponse::Error(resp) => {
                let error = String::from_utf8_lossy(&resp.error);
                Err(anyhow!("GetAttr failed: {}", error))
            }
            _ => Err(anyhow!("Unexpected response type")),
        }
    }

    pub async fn mkdir(&mut self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy().to_string();
        let req = FsRequest::mkdir(path_str);

        match self.send_request(req).await? {
            FsResponse::Ok(_) => Ok(()),
            FsResponse::Error(resp) => {
                let error = String::from_utf8_lossy(&resp.error);
                Err(anyhow!("Mkdir failed: {}", error))
            }
            _ => Err(anyhow!("Unexpected response type")),
        }
    }

    pub async fn unlink(&mut self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy().to_string();
        let req = FsRequest::unlink(path_str);

        match self.send_request(req).await? {
            FsResponse::Ok(_) => Ok(()),
            FsResponse::Error(resp) => {
                let error = String::from_utf8_lossy(&resp.error);
                Err(anyhow!("Unlink failed: {}", error))
            }
            _ => Err(anyhow!("Unexpected response type")),
        }
    }

    pub async fn readdir(&mut self, path: &Path) -> Result<Vec<agentfs_core::DirEntry>> {
        let path_str = path.to_string_lossy().to_string();
        let req = FsRequest::readdir(path_str);

        match self.send_request(req).await? {
            FsResponse::Entries(resp) => {
                let entries = resp.entries.into_iter()
                    .map(|entry| agentfs_core::DirEntry {
                        name: String::from_utf8_lossy(&entry.name).to_string(),
                        is_dir: entry.is_dir,
                        is_symlink: entry.is_symlink,
                        len: entry.len,
                    })
                    .collect();
                Ok(entries)
            }
            FsResponse::Error(resp) => {
                let error = String::from_utf8_lossy(&resp.error);
                Err(anyhow!("ReadDir failed: {}", error))
            }
            _ => Err(anyhow!("Unexpected response type")),
        }
    }
}
