// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Hook-safe I/O implementation for command trace client
//!
//! This implementation uses direct libc calls for all I/O operations,
//! ensuring that we never trigger installed hooks when communicating with the trace server.
//! This prevents infinite recursion when the shim itself hooks the same syscalls.

use ah_command_trace_client::io_trait::UnixSocketIO;
use std::ffi::CString;
use std::io::{self, Error, ErrorKind};
use std::os::unix::io::RawFd;
use std::path::Path;

/// Hook-safe I/O implementation using direct libc calls
///
/// All operations use direct `libc::` calls to bypass any installed hooks.
pub struct HookSafeIO;

impl UnixSocketIO for HookSafeIO {
    fn connect(&self, path: &Path) -> io::Result<RawFd> {
        unsafe {
            // Create socket
            let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
            if fd < 0 {
                return Err(Error::last_os_error());
            }

            // Prepare sockaddr_un
            let path_cstr = CString::new(path.as_os_str().to_string_lossy().as_ref())
                .map_err(|_| Error::new(ErrorKind::InvalidInput, "path contains null byte"))?;

            let mut addr: libc::sockaddr_un = std::mem::zeroed();
            addr.sun_family = libc::AF_UNIX as libc::sa_family_t;

            let path_bytes = path_cstr.as_bytes_with_nul();
            if path_bytes.len() > addr.sun_path.len() {
                libc::close(fd);
                return Err(Error::new(ErrorKind::InvalidInput, "path too long"));
            }

            std::ptr::copy_nonoverlapping(
                path_bytes.as_ptr() as *const i8,
                addr.sun_path.as_mut_ptr(),
                path_bytes.len(),
            );

            // Connect
            let result = libc::connect(
                fd,
                &addr as *const libc::sockaddr_un as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_un>() as libc::socklen_t,
            );

            if result < 0 {
                let err = Error::last_os_error();
                libc::close(fd);
                return Err(err);
            }

            Ok(fd)
        }
    }

    fn write_all(&self, fd: RawFd, buf: &[u8]) -> io::Result<()> {
        let mut written = 0;
        while written < buf.len() {
            let result = unsafe {
                libc::write(
                    fd,
                    buf[written..].as_ptr() as *const libc::c_void,
                    buf.len() - written,
                )
            };
            if result < 0 {
                return Err(Error::last_os_error());
            }
            written += result as usize;
        }
        Ok(())
    }

    fn read_exact(&self, fd: RawFd, buf: &mut [u8]) -> io::Result<()> {
        let mut read_bytes = 0;
        while read_bytes < buf.len() {
            let result = unsafe {
                libc::read(
                    fd,
                    buf[read_bytes..].as_mut_ptr() as *mut libc::c_void,
                    buf.len() - read_bytes,
                )
            };
            if result < 0 {
                return Err(Error::last_os_error());
            }
            if result == 0 {
                return Err(Error::new(ErrorKind::UnexpectedEof, "unexpected EOF"));
            }
            read_bytes += result as usize;
        }
        Ok(())
    }

    fn close(&self, fd: RawFd) -> io::Result<()> {
        let result = unsafe { libc::close(fd) };
        if result < 0 {
            Err(Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn set_read_timeout(&self, fd: RawFd, timeout: std::time::Duration) -> io::Result<()> {
        let timeval = libc::timeval {
            tv_sec: timeout.as_secs() as libc::time_t,
            tv_usec: timeout.subsec_micros() as libc::suseconds_t,
        };
        let result = unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &timeval as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        };
        if result < 0 {
            Err(Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn set_write_timeout(&self, fd: RawFd, timeout: std::time::Duration) -> io::Result<()> {
        let timeval = libc::timeval {
            tv_sec: timeout.as_secs() as libc::time_t,
            tv_usec: timeout.subsec_micros() as libc::suseconds_t,
        };
        let result = unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_SNDTIMEO,
                &timeval as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        };
        if result < 0 {
            Err(Error::last_os_error())
        } else {
            Ok(())
        }
    }
}
