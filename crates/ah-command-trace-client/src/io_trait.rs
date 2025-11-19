// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! I/O abstraction trait for command trace client
//!
//! This module provides a trait-based abstraction for Unix socket I/O operations.
//! This allows the client to work correctly both in normal contexts and within
//! interpose hooks where we must use `call_real!` to avoid recursion.

use std::io::{self, Error, ErrorKind};
use std::os::unix::io::RawFd;
use std::path::Path;

/// Abstraction for Unix socket I/O operations
///
/// Implementations of this trait provide the low-level socket operations
/// needed by the command trace client. Different implementations can use
/// different syscall mechanisms (normal vs hook-safe).
pub trait UnixSocketIO {
    /// Connect to a Unix domain socket at the given path
    ///
    /// Returns a file descriptor for the connected socket.
    fn connect(&self, path: &Path) -> io::Result<RawFd>;

    /// Write all bytes to the file descriptor
    ///
    /// Returns an error if not all bytes could be written.
    fn write_all(&self, fd: RawFd, buf: &[u8]) -> io::Result<()>;

    /// Read exact number of bytes from the file descriptor
    ///
    /// Returns an error if the exact number of bytes couldn't be read.
    fn read_exact(&self, fd: RawFd, buf: &mut [u8]) -> io::Result<()>;

    /// Close the file descriptor
    fn close(&self, fd: RawFd) -> io::Result<()>;

    /// Set read timeout on the socket (optional, returns Ok if not supported)
    fn set_read_timeout(&self, fd: RawFd, timeout: std::time::Duration) -> io::Result<()>;

    /// Set write timeout on the socket (optional, returns Ok if not supported)
    fn set_write_timeout(&self, fd: RawFd, timeout: std::time::Duration) -> io::Result<()>;
}

/// Standard I/O implementation using normal syscalls
///
/// This implementation uses libc directly for syscalls, which will go through
/// any installed hooks in the normal way.
pub struct StandardIO;

impl UnixSocketIO for StandardIO {
    fn connect(&self, path: &Path) -> io::Result<RawFd> {
        use std::os::unix::net::UnixStream;
        let stream = UnixStream::connect(path)?;
        use std::os::unix::io::IntoRawFd;
        Ok(stream.into_raw_fd())
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
