use agentfs_proto::*;
use anyhow::{Result, anyhow};
use ssz::{Decode, Encode};
use std::os::fd::AsRawFd;
use std::path::PathBuf;

/// Convert String to Vec<u8> for SSZ encoding
fn string_to_bytes(s: String) -> Vec<u8> {
    s.into_bytes()
}

/// Convert Vec<u8> to String for CLI usage
fn bytes_to_string(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes).map_err(|e| anyhow!("Invalid UTF-8 in response: {}", e))
}

/// Build a snapshot create request
pub fn build_snapshot_create_request(name: Option<String>) -> Request {
    Request::snapshot_create(name)
}

/// Build a snapshot list request
pub fn build_snapshot_list_request() -> Request {
    Request::snapshot_list()
}

/// Build a branch create request
pub fn build_branch_create_request(from: String, name: Option<String>) -> Request {
    Request::branch_create(from, name)
}

/// Build a branch bind request
pub fn build_branch_bind_request(branch: String, pid: Option<u32>) -> Request {
    Request::branch_bind(branch, pid)
}

/// Platform-specific transport for communicating with AgentFS adapters
pub enum ControlTransport {
    #[cfg(unix)]
    Fuse { mount_point: PathBuf },
    #[cfg(windows)]
    WinFsp { volume_path: String },
}

impl ControlTransport {
    pub fn new(mount_point: PathBuf) -> Result<Self> {
        #[cfg(unix)]
        {
            Ok(ControlTransport::Fuse { mount_point })
        }

        #[cfg(windows)]
        {
            // Convert path to Windows volume format (e.g., "X:")
            let volume_path = mount_point.to_string_lossy().to_string();
            if !volume_path.ends_with(':') && !volume_path.ends_with(":\\") {
                return Err(anyhow!(
                    "Windows mount point must be a drive letter (e.g., X:)"
                ));
            }
            Ok(ControlTransport::WinFsp {
                volume_path: volume_path.trim_end_matches('\\').to_string(),
            })
        }
    }
}

/// Send a control request to the AgentFS adapter
pub async fn send_control_request(
    transport: ControlTransport,
    request: Request,
) -> Result<Response> {
    match transport {
        #[cfg(unix)]
        ControlTransport::Fuse { mount_point } => {
            send_fuse_control_request(mount_point, request).await
        }
        #[cfg(windows)]
        ControlTransport::WinFsp { volume_path } => {
            send_winfsp_control_request(volume_path, request).await
        }
    }
}

#[cfg(unix)]
async fn send_fuse_control_request(mount_point: PathBuf, request: Request) -> Result<Response> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::fs;

    // Path to the control file
    let control_path = mount_point.join(".agentfs").join("control");

    // Check if control file exists
    if !control_path.exists() {
        return Err(anyhow!(
            "AgentFS control file not found at {:?}",
            control_path
        ));
    }

    // Check permissions (should be restricted)
    let metadata = fs::metadata(&control_path).await?;
    let permissions = metadata.permissions();
    let mode = permissions.mode();

    // For security, the control file should have restricted permissions
    // (typically root:root with 0600 or similar)
    if mode & 0o077 != 0 {
        eprintln!(
            "Warning: control file has overly permissive permissions: {:o}",
            mode
        );
    }

    // Encode request to SSZ
    let request_bytes = request.as_ssz_bytes();
    let mut buffer = request_bytes;

    // Ensure buffer has enough space for response (4KB should be enough)
    buffer.resize(4096, 0);

    // Define ioctl command (must match FUSE adapter)
    const AGENTFS_IOCTL_CMD: u32 = 0x8000_4146; // 'AF' in hex with 0x8000 bit set

    // Use nix crate for ioctl
    let fd = nix::fcntl::open(
        &control_path,
        nix::fcntl::OFlag::O_RDWR,
        nix::sys::stat::Mode::empty(),
    )
    .map_err(|e| anyhow!("Failed to open control file: {}", e))?;

    // Call ioctl with buffer containing request, ioctl will overwrite with response
    let result = unsafe {
        libc::ioctl(
            fd.as_raw_fd(),
            AGENTFS_IOCTL_CMD as libc::c_ulong,
            buffer.as_mut_ptr() as *mut libc::c_void,
        )
    };

    if result < 0 {
        let errno = nix::errno::Errno::last_raw();
        let _ = nix::unistd::close(fd);
        return Err(anyhow!("ioctl failed with errno {}", errno));
    }

    // Close file descriptor
    let _ = nix::unistd::close(fd);

    // Decode SSZ response (ioctl overwrites the buffer with response)
    let response = Response::from_ssz_bytes(&buffer)
        .map_err(|e| anyhow!("Failed to decode SSZ response: {:?}", e))?;

    Ok(response)
}

#[cfg(windows)]
async fn send_winfsp_control_request(volume_path: String, request: Request) -> Result<Response> {
    use std::ffi::CString;
    use winapi::shared::minwindef::{DWORD, FALSE};
    use winapi::shared::ntdef::NULL;
    use winapi::um::fileapi::{CreateFileA, OPEN_EXISTING};
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::winioctl::DeviceIoControl;
    use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ, GENERIC_WRITE};

    // Encode request to SSZ
    let request_bytes = request.as_ssz_bytes();

    // Create volume path with \\.\ prefix for DeviceIoControl
    let device_path = format!("\\\\.\\{}", volume_path.trim_end_matches(':'));

    // Open handle to volume
    let device_path_cstr = CString::new(device_path)?;
    let handle = unsafe {
        CreateFileA(
            device_path_cstr.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            NULL,
            OPEN_EXISTING,
            0,
            NULL,
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        return Err(anyhow!("Failed to open volume handle for {}", volume_path));
    }

    // For now, we'll simulate DeviceIoControl
    // In a real implementation, this would need:
    // 1. Define IOCTL codes matching the WinFsp adapter
    // 2. Call DeviceIoControl with proper buffers
    // 3. Handle the response

    eprintln!("WinFsp DeviceIoControl transport not fully implemented yet");
    eprintln!("Request encoded as SSZ: {} bytes", request_bytes.len());

    // Close handle
    unsafe { CloseHandle(handle) };

    // Return mock response for now
    let mock_response = Response::error(
        "WinFsp DeviceIoControl transport not implemented".to_string(),
        Some(-1),
    );

    Ok(mock_response)
}
