// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared POSIX functionality for command trace interposition

#![allow(clippy::too_many_arguments)] // libc interpose signatures exceed clippy thresholds

use crate::core::{self, ShimState};
#[cfg(target_os = "macos")]
use crate::hook_safe_io::HookSafeIO;
use ah_command_trace_client::{ClientConfig, CommandTraceClient};
use ah_command_trace_proto::{CommandChunk, CommandStart};
use std::ffi::{CStr, OsString, c_void};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

// Macro to convert i32 to usize only on macOS to avoid unnecessary conversion warnings on Linux
#[cfg(target_os = "macos")]
macro_rules! macos_i32_to_usize {
    ($expr:expr) => {
        $expr as usize
    };
}

#[cfg(not(target_os = "macos"))]
macro_rules! macos_i32_to_usize {
    ($expr:expr) => {
        $expr
    };
}

thread_local! {
    static IN_TRACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

struct TraceGuard;

impl TraceGuard {
    fn try_enter() -> Option<Self> {
        if IN_TRACE.with(|c| c.get()) {
            None
        } else {
            IN_TRACE.with(|c| c.set(true));
            Some(Self)
        }
    }
}

impl Drop for TraceGuard {
    fn drop(&mut self) {
        IN_TRACE.with(|c| c.set(false));
    }
}

/// Global client connection to the command trace server
#[cfg(target_os = "macos")]
static CLIENT: Mutex<Option<CommandTraceClient<HookSafeIO>>> = Mutex::new(None);

#[cfg(not(target_os = "macos"))]
static CLIENT: Mutex<Option<CommandTraceClient<ah_command_trace_client::io_trait::StandardIO>>> =
    Mutex::new(None);

/// Initialize the client connection to the command trace server
pub fn initialize_client() -> Result<(), Box<dyn std::error::Error>> {
    // Prevent recursion during initialization
    let _guard = match TraceGuard::try_enter() {
        Some(g) => g,
        None => return Ok(()),
    };

    let state_guard = match crate::core::get_or_initialize_shim_state().and_then(|s| s.lock().ok())
    {
        Some(guard) => guard,
        None => return Ok(()),
    };

    let (socket_path, state_clone) = match &*state_guard {
        ShimState::Ready { socket_path, .. } => (socket_path.clone(), state_guard.clone()),
        _ => return Ok(()),
    };

    drop(state_guard);

    let config = ClientConfig::builder("ah-command-trace-shim", env!("CARGO_PKG_VERSION"))
        .build()
        .map_err(|e| format!("Failed to build client config: {}", e))?;

    #[cfg(target_os = "macos")]
    let io = HookSafeIO;
    #[cfg(not(target_os = "macos"))]
    let io = ah_command_trace_client::io_trait::StandardIO;

    match CommandTraceClient::connect(Path::new(&socket_path), &config, io) {
        Ok(mut client) => {
            // Self-report this process so exec-based launches are captured.
            let self_start = build_self_command_start();
            if let Err(e) = client.send_command_start(self_start) {
                core::log_message(
                    &state_clone,
                    &format!("Failed to self-report CommandStart: {}", e),
                );
            }

            *CLIENT.lock().unwrap() = Some(client);
            core::log_message(
                &state_clone,
                "Command trace client initialized (self-reported)",
            );
            Ok(())
        }
        Err(e) => {
            core::log_message(
                &state_clone,
                &format!("Connection attempt failed (will retry): {}", e),
            );
            // Don't set state to Error - just return error and let caller retry
            Err(e.into())
        }
    }
}

/// Send a CommandStart message, establishing connection if needed
pub fn send_command_start(
    pid: u32,
    ppid: u32,
    executable: &[u8],
    args: Vec<Vec<u8>>,
    env: Vec<Vec<u8>>,
    cwd: Vec<u8>,
) {
    // Prevent recursion/deadlock (e.g. if we hold CLIENT lock and write triggers hook)
    let _guard = match TraceGuard::try_enter() {
        Some(g) => g,
        None => {
            return;
        }
    };

    // Check if shim is still ready before attempting connection
    if let Some(ref state_guard) =
        crate::core::get_or_initialize_shim_state().and_then(|s| s.lock().ok())
    {
        if !matches!(**state_guard, crate::core::ShimState::Ready { .. }) {
            // Shim is disabled or in error state, don't attempt connection
            return;
        }
    } else {
        // Can't access shim state, don't attempt connection
        return;
    }

    // Try to get or establish a client connection
    let mut client_guard = CLIENT.lock().unwrap();
    if client_guard.is_none() {
        drop(client_guard); // Release the lock before calling initialize_client

        // Drop guard temporarily to allow initialize_client to re-enter (it has its own guard check)
        drop(_guard);

        if initialize_client().is_err() {
            return;
        }

        // Re-acquire guard for the rest of this function
        let _guard = match TraceGuard::try_enter() {
            Some(g) => g,
            None => return,
        };

        client_guard = CLIENT.lock().unwrap();
        if client_guard.is_none() {
            return;
        }
    }

    if let Some(ref mut client) = *client_guard {
        let start_time_ns = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let command_start = CommandStart {
            command_id: 0, // Will be assigned by server
            pid,
            ppid,
            cwd,
            executable: executable.to_vec(),
            args,
            env,
            start_time_ns,
        };

        let _ = client.send_command_start(command_start);
    }
}

/// Send a CommandChunk message
pub fn send_command_chunk(fd: i32, data: &[u8]) {
    // Prevent recursion (e.g. if client logs to stderr or writes to traced FD)
    let _guard = match TraceGuard::try_enter() {
        Some(g) => g,
        None => return,
    };

    let stream_type = if let Some(ref state_guard) =
        crate::core::get_or_initialize_shim_state().and_then(|s| s.lock().ok())
    {
        if let ShimState::Ready { fd_table, .. } = &**state_guard {
            fd_table.get(&fd).copied()
        } else {
            None
        }
    } else {
        None
    };

    if let Some(stream_type) = stream_type {
        let mut client_guard = CLIENT.lock().unwrap();
        // Ensure connected
        if client_guard.is_none() {
            drop(client_guard);

            // Drop guard to allow initialize_client to run
            drop(_guard);

            if initialize_client().is_err() {
                let msg = b"[ah-command-trace-shim] Unable to establish client connection\n";
                unsafe {
                    libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
                }
                return;
            }

            // Re-acquire guard
            let _guard = match TraceGuard::try_enter() {
                Some(g) => g,
                None => return,
            };

            client_guard = CLIENT.lock().unwrap();
            if client_guard.is_none() {
                let msg = b"[ah-command-trace-shim] Client connection unavailable\n";
                unsafe {
                    libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
                }
                return;
            }
        }

        if let Some(ref mut client) = *client_guard {
            let timestamp_ns = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;

            let chunk = CommandChunk {
                command_id: 0, // Server resolves this
                stream_type: stream_type as u8,
                sequence_no: 0,
                data: data.to_vec(),
                pty_offset: None,
                timestamp_ns,
            };

            if let Err(e) = client.send_command_chunk(chunk) {
                let msg = format!(
                    "[ah-command-trace-shim] Failed to send CommandChunk: {}\n",
                    e
                );
                unsafe {
                    libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
                }
            }
        }
    }
}

/// Update FD mapping when a new FD is created from an old one
fn update_fd_mapping(oldfd: i32, newfd: i32) {
    if let Some(ref mut state_guard) =
        crate::core::get_or_initialize_shim_state().and_then(|s| s.lock().ok())
    {
        if let ShimState::Ready { fd_table, .. } = &mut **state_guard {
            if let Some(&stream_type) = fd_table.get(&oldfd) {
                fd_table.insert(newfd, stream_type);
            }
        }
    }
}

/// Remove FD mapping when FD is closed
fn remove_fd_mapping(fd: i32) {
    if let Some(ref mut state_guard) =
        crate::core::get_or_initialize_shim_state().and_then(|s| s.lock().ok())
    {
        if let ShimState::Ready { fd_table, .. } = &mut **state_guard {
            fd_table.remove(&fd);
        }
    }
}

/// Get current working directory as bytes
pub fn get_current_dir() -> Vec<u8> {
    std::env::current_dir()
        .ok()
        .map(|p| p.as_os_str().as_bytes().to_vec())
        .unwrap_or_else(|| b"<unknown>".to_vec())
}

fn build_self_command_start() -> CommandStart {
    let pid = std::process::id();
    let ppid = unsafe { libc::getppid() as u32 };
    let executable = get_current_executable();
    let args = collect_process_args();
    let env = collect_process_env();
    let cwd = get_current_dir();
    let start_time_ns = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    CommandStart {
        command_id: 0,
        pid,
        ppid,
        cwd,
        executable,
        args,
        env,
        start_time_ns,
    }
}

fn get_current_executable() -> Vec<u8> {
    std::env::current_exe()
        .ok()
        .map(|p| p.as_os_str().as_bytes().to_vec())
        .unwrap_or_else(|| b"<unknown>".to_vec())
}

fn collect_process_args() -> Vec<Vec<u8>> {
    std::env::args_os().map(|s| s.into_vec()).collect()
}

fn collect_process_env() -> Vec<Vec<u8>> {
    std::env::vars_os().map(|(key, value)| encode_env_var(key, value)).collect()
}

fn encode_env_var(key: OsString, value: OsString) -> Vec<u8> {
    let mut encoded = key.into_vec();
    encoded.push(b'=');
    encoded.extend(value.into_vec());
    encoded
}

/// Get environment variables as key=value pairs
pub fn get_environment() -> Vec<Vec<u8>> {
    std::env::vars().map(|(k, v)| format!("{}={}", k, v).into_bytes()).collect()
}

/// Extract command information from exec arguments
/// # Safety
///
/// The caller must ensure that:
/// - `path`, if non-null, points to a valid C string (NUL-terminated) for the executable path.
/// - `argv` is either NULL or points to a NULL-terminated array of pointers, each pointing to a
///   valid NUL-terminated C string. Iteration stops at the first NULL pointer.
/// - `envp` is either NULL or points to a NULL-terminated array of pointers, each pointing to a
///   valid NUL-terminated C string representing an environment variable in `KEY=VALUE` form.
/// - All pointers remain valid for the duration of this function call; they must not be freed or
///   modified concurrently.
/// - Memory referenced is readable; no mutable aliasing rules are violated.
///
/// Undefined behavior may occur if any pointer is invalid, not properly NUL-terminated, or if the
/// arrays are not terminated by a NULL pointer.
pub type ExtractedCommandInfoParts = (Vec<u8>, Vec<Vec<u8>>, Vec<Vec<u8>>, Vec<u8>);

/// Extract executable path, argv list, environment variables and current working directory.
///
/// # Safety
///
/// The caller must ensure that the raw C pointers provided obey the following:
/// - `path` if non-null points to a valid NUL-terminated C string.
/// - `argv` is NULL or a NULL-terminated array of pointers to valid NUL-terminated C strings.
/// - `envp` is NULL or a NULL-terminated array of pointers to valid NUL-terminated C strings of the form `KEY=VALUE`.
/// - All memory regions referenced remain readable for the duration of the call and are not concurrently mutated.
/// - Pointers must not be invalid or dangling; violating these rules results in undefined behavior.
pub unsafe fn extract_command_info(
    path: *const libc::c_char,
    argv: *const *mut libc::c_char,
    envp: *const *mut libc::c_char,
) -> ExtractedCommandInfoParts {
    let executable = if !path.is_null() {
        CStr::from_ptr(path).to_bytes().to_vec()
    } else {
        b"<null>".to_vec()
    };

    let mut args = Vec::new();
    if !argv.is_null() {
        let mut i = 0;
        loop {
            let arg_ptr = *argv.offset(i);
            if arg_ptr.is_null() {
                break;
            }
            let arg = CStr::from_ptr(arg_ptr).to_bytes().to_vec();
            args.push(arg);
            i += 1;
        }
    }

    let mut env = Vec::new();
    if !envp.is_null() {
        let mut i = 0;
        loop {
            let env_ptr = *envp.offset(i);
            if env_ptr.is_null() {
                break;
            }
            let env_var = CStr::from_ptr(env_ptr).to_bytes().to_vec();
            env.push(env_var);
            i += 1;
        }
    }

    let cwd = get_current_dir();

    (executable, args, env, cwd)
}

// Common POSIX hooks using stackable-hooks

stackable_hooks::hook! {
    unsafe fn fork() -> libc::pid_t => my_fork {
        stackable_hooks::call_next!()
    }
}

stackable_hooks::hook! {
    unsafe fn execve(path: *const libc::c_char, argv: *const *mut libc::c_char, envp: *const *mut libc::c_char) -> libc::c_int => my_execve {
        stackable_hooks::call_next!(path, argv, envp)
    }
}

stackable_hooks::hook! {
    unsafe fn execvp(file: *const libc::c_char, argv: *const *mut libc::c_char) -> libc::c_int => my_execvp {
        stackable_hooks::call_next!(file, argv)
    }
}

stackable_hooks::hook! {
    unsafe fn execv(file: *const libc::c_char, argv: *const *mut libc::c_char) -> libc::c_int => my_execv {
        stackable_hooks::call_next!(file, argv)
    }
}

#[cfg(target_os = "linux")]
stackable_hooks::hook! {
    unsafe fn execveat(dirfd: libc::c_int, pathname: *const libc::c_char, argv: *const *mut libc::c_char, envp: *const *mut libc::c_char, flags: libc::c_int) -> libc::c_int => my_execveat {
        stackable_hooks::call_next!(dirfd, pathname, argv, envp, flags)
    }
}

#[cfg(target_os = "linux")]
stackable_hooks::hook! {
    unsafe fn execvpe(file: *const libc::c_char, argv: *const *mut libc::c_char, envp: *const *mut libc::c_char) -> libc::c_int => my_execvpe {
        stackable_hooks::call_next!(file, argv, envp)
    }
}

stackable_hooks::hook! {
    unsafe fn posix_spawn(pid: *mut libc::pid_t, path: *const libc::c_char, file_actions: *const libc::posix_spawn_file_actions_t, attrp: *const libc::posix_spawnattr_t, argv: *const *mut libc::c_char, envp: *const *mut libc::c_char) -> libc::c_int => my_posix_spawn {
        // Call the real posix_spawn first
        let result = stackable_hooks::call_next!(pid, path, file_actions, attrp, argv, envp);

        // If spawn was successful and we have a PID, send CommandStart
        if result == 0 && !pid.is_null() {
// Check if shim is ready (acquire and release lock immediately to avoid deadlock)
            let is_ready = if let Some(mutex) = crate::core::get_or_initialize_shim_state() {
                if let Ok(guard) = mutex.lock() {
                    matches!(*guard, ShimState::Ready { .. })
                } else {
                    false
                }
            } else {
                false
            };

            if is_ready {
                let child_pid = unsafe { *pid } as u32;
                let parent_pid = std::process::id();

                // Get executable path
                let executable = if !path.is_null() {
                    unsafe { CStr::from_ptr(path) }.to_bytes().to_vec()
                } else {
                    b"<null>".to_vec()
                };

                // Get arguments
                let mut args = Vec::new();
                if !argv.is_null() {
                    let mut i = 0;
                    loop {
                        let arg_ptr = unsafe { *argv.offset(i) };
                        if arg_ptr.is_null() {
                            break;
                        }
                        let arg = unsafe { CStr::from_ptr(arg_ptr) }.to_bytes().to_vec();
                        args.push(arg);
                        i += 1;
                    }
                }

                // Get environment
                let mut env = Vec::new();
                if !envp.is_null() {
                    let mut i = 0;
                    loop {
                        let env_ptr = unsafe { *envp.offset(i) };
                        if env_ptr.is_null() {
                            break;
                        }
                        let env_var = unsafe { CStr::from_ptr(env_ptr) }.to_bytes().to_vec();
                        env.push(env_var);
                        i += 1;
                    }
                }

                let cwd = get_current_dir();

                send_command_start(child_pid, parent_pid, &executable, args, env, cwd);
            }
        }

        result
    }
}

stackable_hooks::hook! {
    unsafe fn posix_spawnp(pid: *mut libc::pid_t, file: *const libc::c_char, file_actions: *const libc::posix_spawn_file_actions_t, attrp: *const libc::posix_spawnattr_t, argv: *const *mut libc::c_char, envp: *const *mut libc::c_char) -> libc::c_int => my_posix_spawnp {
        // Call the real posix_spawnp first
        let result = stackable_hooks::call_next!(pid, file, file_actions, attrp, argv, envp);

        // If spawn was successful and we have a PID, send CommandStart
        if result == 0 && !pid.is_null() {
// Check if shim is ready (acquire and release lock immediately to avoid deadlock)
            let is_ready = if let Some(mutex) = crate::core::get_or_initialize_shim_state() {
                if let Ok(guard) = mutex.lock() {
                    matches!(*guard, ShimState::Ready { .. })
                } else {
                    false
                }
            } else {
                false
            };

            if is_ready {
                let child_pid = unsafe { *pid } as u32;
                let parent_pid = std::process::id();

                // Get executable path
                let executable = if !file.is_null() {
                    unsafe { CStr::from_ptr(file) }.to_bytes().to_vec()
                } else {
                    b"<null>".to_vec()
                };

                // Get arguments
                let mut args = Vec::new();
                if !argv.is_null() {
                    let mut i = 0;
                    loop {
                        let arg_ptr = unsafe { *argv.offset(i) };
                        if arg_ptr.is_null() {
                            break;
                        }
                        let arg = unsafe { CStr::from_ptr(arg_ptr) }.to_bytes().to_vec();
                        args.push(arg);
                        i += 1;
                    }
                }

                // Get environment
                let mut env = Vec::new();
                if !envp.is_null() {
                    let mut i = 0;
                    loop {
                        let env_ptr = unsafe { *envp.offset(i) };
                        if env_ptr.is_null() {
                            break;
                        }
                        let env_var = unsafe { CStr::from_ptr(env_ptr) }.to_bytes().to_vec();
                        env.push(env_var);
                        i += 1;
                    }
                }

                let cwd = get_current_dir();

                send_command_start(child_pid, parent_pid, &executable, args, env, cwd);
            }
        }

        result
    }
}

// FD Lifecycle and I/O Hooks

stackable_hooks::hook! {
    unsafe fn write(fd: libc::c_int, buf: *const c_void, count: libc::size_t) -> libc::ssize_t => my_write {
        let result = stackable_hooks::call_next!(fd, buf, count);

        if result > 0 {
             let slice = std::slice::from_raw_parts(buf as *const u8, result as usize);
             send_command_chunk(fd, slice);
        }

        result
    }
}
stackable_hooks::hook! {
    unsafe fn writev(fd: libc::c_int, iov: *const libc::iovec, iovcnt: libc::c_int) -> libc::ssize_t => my_writev {
        let result = stackable_hooks::call_next!(fd, iov, iovcnt);

        if result > 0 {
             // Reconstruct written data from iov?
             // Or just iterate iov and append until we reach result size?
             let mut remaining = result as usize;
             let mut collected_data = Vec::with_capacity(remaining);

             for i in 0..iovcnt {
                 if remaining == 0 { break; }
                 let iov_ptr = iov.offset(i as isize);
                 let iov_base = (*iov_ptr).iov_base;
                 let iov_len = (*iov_ptr).iov_len;

                 let len = std::cmp::min(iov_len, remaining);
                 let slice = std::slice::from_raw_parts(iov_base as *const u8, len);
                 collected_data.extend_from_slice(slice);
                 remaining -= len;
             }

             send_command_chunk(fd, &collected_data);
        }

        result
    }
}

stackable_hooks::hook! {
    unsafe fn dup(oldfd: libc::c_int) -> libc::c_int => my_dup {
        let newfd = stackable_hooks::call_next!(oldfd);
        if newfd >= 0 {
            update_fd_mapping(oldfd, newfd);
        }
        newfd
    }
}

stackable_hooks::hook! {
    unsafe fn dup2(oldfd: libc::c_int, newfd: libc::c_int) -> libc::c_int => my_dup2 {
        // If newfd was open, it is closed. remove_fd_mapping(newfd) first?
        // dup2 closes newfd silently if open.
        // But we just overwrite mapping, so it is fine.
        let result = stackable_hooks::call_next!(oldfd, newfd);
        if result >= 0 {
            update_fd_mapping(oldfd, newfd);
        }
        result
    }
}

#[cfg(target_os = "linux")]
stackable_hooks::hook! {
    unsafe fn dup3(oldfd: libc::c_int, newfd: libc::c_int, flags: libc::c_int) -> libc::c_int => my_dup3 {
        let result = stackable_hooks::call_next!(oldfd, newfd, flags);
        if result >= 0 {
            update_fd_mapping(oldfd, newfd);
        }
        result
    }
}

stackable_hooks::hook! {
    unsafe fn close(fd: libc::c_int) -> libc::c_int => my_close {
        let result = stackable_hooks::call_next!(fd);
        if result == 0 {
            remove_fd_mapping(fd);
        }
        result
    }
}

#[cfg(target_os = "linux")]
stackable_hooks::hook! {
    unsafe fn fcntl(fd: libc::c_int, cmd: libc::c_int, arg: libc::c_int) -> libc::c_int => my_fcntl {
        let result = stackable_hooks::call_next!(fd, cmd, arg);
        if result >= 0 && (cmd == libc::F_DUPFD || cmd == libc::F_DUPFD_CLOEXEC) {
            update_fd_mapping(fd, result);
        }
        result
    }
}

// sendmsg is often used for socket IO, but can be used for other things.
// For M2 we need to support it.
stackable_hooks::hook! {
    unsafe fn sendmsg(fd: libc::c_int, msg: *const libc::msghdr, flags: libc::c_int) -> libc::ssize_t => my_sendmsg {
        let result = stackable_hooks::call_next!(fd, msg, flags);

        if result > 0 {
             // Extract data from msg.msg_iov
             if !msg.is_null() {
                 let iov = (*msg).msg_iov;
                 let iovcnt = (*msg).msg_iovlen; // size_t or int depending on platform?
                 // libc::msghdr field types vary.
                 // On Linux/macOS msg_iovlen is size_t or int?
                 // In libc crate, it is usually size_t (usize) or int.
                 // I'll cast to isize for offset.

                 let mut remaining = result as usize;
                 let mut collected_data = Vec::with_capacity(remaining);

                 for i in 0..iovcnt {
                     if remaining == 0 { break; }
                     let iov_ptr = iov.add(macos_i32_to_usize!(i));
                     let iov_base = (*iov_ptr).iov_base;
                     let iov_len = (*iov_ptr).iov_len;

                     let len = std::cmp::min(iov_len, remaining);
                     let slice = std::slice::from_raw_parts(iov_base as *const u8, len);
                     collected_data.extend_from_slice(slice);
                     remaining -= len;
                 }
                 send_command_chunk(fd, &collected_data);
             }
        }

        result
    }
}
