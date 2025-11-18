// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared POSIX functionality for command trace interposition

use crate::core::{self, ShimState};
use ah_command_trace_client::{ClientConfig, CommandTraceClient};
use ah_command_trace_proto::{CommandChunk, CommandStart};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::thread;
use std::time::SystemTime;

// Temporarily disabled static CLIENT to debug segfault
// static CLIENT: Mutex<Option<CommandTraceClient>> = Mutex::new(None);

/// Thread-local FD tracking for stdout/stderr redirection
thread_local! {
    static FD_TABLE: RefCell<Option<FdTable>> = const { RefCell::new(None) };
}

/// Tracks file descriptor aliases for stdout (1) and stderr (2)
#[derive(Debug, Clone)]
struct FdTable {
    /// Maps FD to whether it's an alias of stdout (1) or stderr (2)
    fd_aliases: HashMap<i32, i32>,
}

impl FdTable {
    fn new() -> Self {
        let mut fd_aliases = HashMap::new();
        // Initially, FD 1 is stdout, FD 2 is stderr
        fd_aliases.insert(1, 1); // FD 1 -> stdout
        fd_aliases.insert(2, 2); // FD 2 -> stderr
        Self { fd_aliases }
    }

    /// Check if an FD is an alias of stdout or stderr
    fn get_stream_type(&self, fd: i32) -> Option<u8> {
        self.fd_aliases.get(&fd).map(|&original| {
            match original {
                1 => 0, // stdout
                2 => 1, // stderr
                _ => panic!("Invalid original FD in table"),
            }
        })
    }

    /// Track a dup operation: new_fd becomes an alias of old_fd
    fn dup(&mut self, old_fd: i32, new_fd: i32) {
        if let Some(&original) = self.fd_aliases.get(&old_fd) {
            self.fd_aliases.insert(new_fd, original);
        }
    }

    /// Track closing an FD
    fn close(&mut self, fd: i32) {
        self.fd_aliases.remove(&fd);
    }

    /// Track opening a new FD (if it's stdout/stderr related)
    fn open(&mut self, fd: i32, is_stdout: bool, is_stderr: bool) {
        if is_stdout {
            self.fd_aliases.insert(fd, 1);
        } else if is_stderr {
            self.fd_aliases.insert(fd, 2);
        }
    }
}

/// Check if we're in a safe state to run hooks (not during library initialization)
fn is_hook_safe() -> bool {
    matches!(
        *core::get_shim_state().lock().unwrap(),
        crate::core::ShimState::Ready { .. }
    )
}

/// Get the stream type (0=stdout, 1=stderr) for a given FD
fn get_stream_type_for_fd(fd: i32) -> Option<u8> {
    FD_TABLE.with(|table| {
        let mut table = table.borrow_mut();
        if table.is_none() {
            *table = Some(FdTable::new());
        }
        table.as_ref().unwrap().get_stream_type(fd)
    })
}

/// Track an FD operation
fn track_fd_dup(old_fd: i32, new_fd: i32) {
    FD_TABLE.with(|table| {
        let mut table = table.borrow_mut();
        if table.is_none() {
            *table = Some(FdTable::new());
        }
        table.as_mut().unwrap().dup(old_fd, new_fd);
    });
}

fn track_fd_close(fd: i32) {
    FD_TABLE.with(|table| {
        let mut table = table.borrow_mut();
        if table.is_none() {
            *table = Some(FdTable::new());
        }
        table.as_mut().unwrap().close(fd);
    });
}

/// Initialize the client connection to the command trace server
pub fn initialize_client() -> Result<(), Box<dyn std::error::Error>> {
    let state = core::get_shim_state().lock().unwrap();

    match *state {
        ShimState::Ready {
            ref socket_path, ..
        } => {
            let config = ClientConfig::builder("ah-command-trace-shim", env!("CARGO_PKG_VERSION"))
                .build()
                .map_err(|e| format!("Failed to build client config: {}", e))?;

            match CommandTraceClient::connect(socket_path.as_ref(), &config) {
                Ok(_client) => {
                    // Temporarily disabled CLIENT storage
                    // *CLIENT.lock().unwrap() = Some(client);
                    core::log_message(
                        &ShimState::Ready {
                            socket_path: socket_path.clone(),
                            log_enabled: true,
                        },
                        "Command trace client initialized",
                    );
                    Ok(())
                }
                Err(e) => {
                    core::log_message(
                        &ShimState::Ready {
                            socket_path: socket_path.clone(),
                            log_enabled: true,
                        },
                        &format!("Failed to connect to command trace server: {}", e),
                    );
                    Err(e.into())
                }
            }
        }
        _ => {
            // Shim not ready, do nothing
            Ok(())
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
    // Temporarily disabled CLIENT usage to debug segfault
    eprintln!("[ah-command-trace-shim] send_command_start called but CLIENT disabled");
    return;

    /*
    // Try to get or establish a client connection
    let mut client_guard = CLIENT.lock().unwrap();
    if client_guard.is_none() {
        // Try to establish connection now, with retries
        drop(client_guard); // Release the lock before calling initialize_client

        for _attempt in 0..5 {
            if let Ok(_) = initialize_client() {
                break;
            }
        }

        client_guard = CLIENT.lock().unwrap();
        if client_guard.is_none() {
            eprintln!(
                "[ah-command-trace-shim] Failed to establish client connection after retries"
            );
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

        if let Err(e) = client.send_command_start(command_start) {
            eprintln!("[ah-command-trace-shim] Failed to send CommandStart: {}", e);
        }
    } else {
        eprintln!("[ah-command-trace-shim] No client available for CommandStart");
    }
    */
}

/// Send a CommandChunk message, establishing connection if needed
pub fn send_command_chunk(
    command_id: u64,
    stream_type: u8, // 0=stdout, 1=stderr
    data: Vec<u8>,
    pty_offset: Option<u64>,
) {
    // Temporarily disabled CLIENT usage to debug segfault
    eprintln!("[ah-command-trace-shim] send_command_chunk called but CLIENT disabled");
    return;

    /*
    // Try to get or establish a client connection
    let mut client_guard = CLIENT.lock().unwrap();
    if client_guard.is_none() {
        // Try to establish connection now, with retries
        drop(client_guard); // Release the lock before calling initialize_client

        for _attempt in 0..5 {
            if let Ok(_) = initialize_client() {
                break;
            }
        }

        client_guard = CLIENT.lock().unwrap();
        if client_guard.is_none() {
            eprintln!(
                "[ah-command-trace-shim] Failed to establish client connection after retries"
            );
            return;
        }
    }

    if let Some(ref mut client) = *client_guard {
        let timestamp_ns = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let chunk = CommandChunk {
            command_id,
            stream_type,
            sequence_no: 0, // Will be assigned by recorder
            data,
            pty_offset,
            timestamp_ns,
        };

        if let Err(e) = client.send_command_chunk(chunk) {
            eprintln!("[ah-command-trace-shim] Failed to send CommandChunk: {}", e);
        }
    } else {
        eprintln!("[ah-command-trace-shim] No client available for CommandChunk");
    }
    */
}

/// Get current working directory as bytes
pub fn get_current_dir() -> Vec<u8> {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.as_bytes().to_vec()))
        .unwrap_or_else(|| b"<unknown>".to_vec())
}

/// Get environment variables as key=value pairs
pub fn get_environment() -> Vec<Vec<u8>> {
    std::env::vars().map(|(k, v)| format!("{}={}", k, v).into_bytes()).collect()
}

/// Extract command information from exec arguments
pub fn extract_command_info(
    path: *const libc::c_char,
    argv: *const *mut libc::c_char,
    envp: *const *mut libc::c_char,
) -> (Vec<u8>, Vec<Vec<u8>>, Vec<Vec<u8>>, Vec<u8>) {
    let executable = if !path.is_null() {
        unsafe { CStr::from_ptr(path) }.to_bytes().to_vec()
    } else {
        b"<null>".to_vec()
    };

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

    (executable, args, env, cwd)
}

// Common POSIX hooks using redhook

redhook::hook! {
    unsafe fn execvp(file: *const libc::c_char, argv: *const *mut libc::c_char) -> libc::c_int => my_execvp {
        eprintln!("[ah-command-trace-shim] execvp hook called!");
        // Get process info BEFORE calling execvp (since execvp replaces the process)
        if is_hook_safe() {
            let pid = std::process::id() as u32;
            let ppid = unsafe { libc::getppid() } as u32;

            let executable = if !file.is_null() {
                unsafe { CStr::from_ptr(file) }.to_bytes().to_vec()
            } else {
                b"<unknown>".to_vec()
            };

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

            let env = get_environment();
            let cwd = get_current_dir();

            send_command_start(pid, ppid, &executable, args, env, cwd);
        }

        redhook::real!(execvp)(file, argv)
    }
}

redhook::hook! {
    unsafe fn execvpe(file: *const libc::c_char, argv: *const *mut libc::c_char, envp: *const *mut libc::c_char) -> libc::c_int => my_execvpe {
        eprintln!("[ah-command-trace-shim] execvpe hook called!");
        // Get process info BEFORE calling execvpe (since execvpe replaces the process)
        if is_hook_safe() {
            let pid = std::process::id() as u32;
            let ppid = unsafe { libc::getppid() } as u32;

            let executable = if !file.is_null() {
                unsafe { CStr::from_ptr(file) }.to_bytes().to_vec()
            } else {
                b"<unknown>".to_vec()
            };

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

            send_command_start(pid, ppid, &executable, args, env, cwd);
        }

        redhook::real!(execvpe)(file, argv, envp)
    }
}

redhook::hook! {
    unsafe fn posix_spawn(pid: *mut libc::pid_t, path: *const libc::c_char, file_actions: *const libc::posix_spawn_file_actions_t, attrp: *const libc::posix_spawnattr_t, argv: *const *mut libc::c_char, envp: *const *mut libc::c_char) -> libc::c_int => my_posix_spawn {
        eprintln!("[ah-command-trace-shim] posix_spawn hook called!");
        // Call the real posix_spawn first
        let result = redhook::real!(posix_spawn)(pid, path, file_actions, attrp, argv, envp);

        // If spawn was successful and we have a PID, send CommandStart
        if result == 0 && !pid.is_null() && is_hook_safe() {
                let child_pid = unsafe { *pid } as u32;
                let parent_pid = std::process::id() as u32;

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

        result
    }
}

// FD tracking hooks

redhook::hook! {
    unsafe fn dup(old_fd: libc::c_int) -> libc::c_int => my_dup {
        eprintln!("[ah-command-trace-shim] dup hook called: {} -> ?", old_fd);
        let result = redhook::real!(dup)(old_fd);

        if result != -1 {
            track_fd_dup(old_fd, result);
            eprintln!("[ah-command-trace-shim] dup: {} -> {}", old_fd, result);
        }

        result
    }
}

redhook::hook! {
    unsafe fn dup2(old_fd: libc::c_int, new_fd: libc::c_int) -> libc::c_int => my_dup2 {
        eprintln!("[ah-command-trace-shim] dup2 hook called: {} -> {}", old_fd, new_fd);
        let result = redhook::real!(dup2)(old_fd, new_fd);

        if result != -1 {
            track_fd_dup(old_fd, new_fd);
            eprintln!("[ah-command-trace-shim] dup2: {} -> {}", old_fd, new_fd);
        }

        result
    }
}

redhook::hook! {
    unsafe fn dup3(old_fd: libc::c_int, new_fd: libc::c_int, flags: libc::c_int) -> libc::c_int => my_dup3 {
        eprintln!("[ah-command-trace-shim] dup3 hook called: {} -> {} (flags: {})", old_fd, new_fd, flags);
        let result = redhook::real!(dup3)(old_fd, new_fd, flags);

        if result != -1 {
            track_fd_dup(old_fd, new_fd);
            eprintln!("[ah-command-trace-shim] dup3: {} -> {}", old_fd, new_fd);
        }

        result
    }
}

redhook::hook! {
    unsafe fn close(fd: libc::c_int) -> libc::c_int => my_close {
        eprintln!("[ah-command-trace-shim] close hook called: {}", fd);
        let result = redhook::real!(close)(fd);

        if result == 0 {
            track_fd_close(fd);
            eprintln!("[ah-command-trace-shim] closed FD: {}", fd);
        }

        result
    }
}

redhook::hook! {
    unsafe fn fcntl(fd: libc::c_int, cmd: libc::c_int, arg: libc::c_int) -> libc::c_int => my_fcntl {
        eprintln!("[ah-command-trace-shim] fcntl hook called: fd={}, cmd={}, arg={}", fd, cmd, arg);

        // Handle F_DUPFD and F_DUPFD_CLOEXEC commands
        let result = if cmd == libc::F_DUPFD || cmd == libc::F_DUPFD_CLOEXEC {
            eprintln!("[ah-command-trace-shim] fcntl dup: {} -> {}", fd, arg);
            let real_result = redhook::real!(fcntl)(fd, cmd, arg);

            if real_result != -1 {
                track_fd_dup(fd, real_result);
                eprintln!("[ah-command-trace-shim] fcntl dup result: {}", real_result);
            }

            real_result
        } else {
            redhook::real!(fcntl)(fd, cmd, arg)
        };

        result
    }
}

redhook::hook! {
    unsafe fn pipe(pipefd: *mut libc::c_int) -> libc::c_int => my_pipe {
        eprintln!("[ah-command-trace-shim] pipe hook called");
        let result = redhook::real!(pipe)(pipefd);

        if result == 0 && !pipefd.is_null() {
            let read_fd = *pipefd;
            let write_fd = *pipefd.offset(1);
            eprintln!("[ah-command-trace-shim] pipe created: read={}, write={}", read_fd, write_fd);
            // Pipes are not stdout/stderr aliases, so we don't track them in our table
        }

        result
    }
}

redhook::hook! {
    unsafe fn pipe2(pipefd: *mut libc::c_int, flags: libc::c_int) -> libc::c_int => my_pipe2 {
        eprintln!("[ah-command-trace-shim] pipe2 hook called (flags: {})", flags);
        let result = redhook::real!(pipe2)(pipefd, flags);

        if result == 0 && !pipefd.is_null() {
            let read_fd = *pipefd;
            let write_fd = *pipefd.offset(1);
            eprintln!("[ah-command-trace-shim] pipe2 created: read={}, write={}", read_fd, write_fd);
            // Pipes are not stdout/stderr aliases, so we don't track them in our table
        }

        result
    }
}

redhook::hook! {
    unsafe fn isatty(fd: libc::c_int) -> libc::c_int => my_isatty {
        eprintln!("[ah-command-trace-shim] isatty hook called: {}", fd);
        redhook::real!(isatty)(fd)
    }
}

// Write hooks for capturing stdout/stderr output

redhook::hook! {
    unsafe fn write(fd: libc::c_int, buf: *const libc::c_void, count: libc::size_t) -> libc::ssize_t => my_write {
        eprintln!("[ah-command-trace-shim] write hook called: fd={}, count={}", fd, count);

        // Check if this FD is stdout or stderr (or an alias)
        if let Some(stream_type) = get_stream_type_for_fd(fd) {
            eprintln!("[ah-command-trace-shim] WRITE HOOK: fd={}, count={}, stream_type={}", fd, count, stream_type);

            // Extract the data being written
            if !buf.is_null() && count > 0 {
                let data = std::slice::from_raw_parts(buf as *const u8, count);
                let data_vec = data.to_vec();

                eprintln!("[ah-command-trace-shim] CAPTURED WRITE: {} bytes to stream {}", count, stream_type);

                // Use current process ID as command ID
                let command_id = std::process::id() as u64;
                send_command_chunk(command_id, stream_type, data_vec, None);
                eprintln!("[ah-command-trace-shim] SENT COMMAND CHUNK");
            }
        } else {
            eprintln!("[ah-command-trace-shim] write hook: fd {} not stdout/stderr", fd);
        }

        redhook::real!(write)(fd, buf, count)
    }
}

redhook::hook! {
    unsafe fn writev(fd: libc::c_int, iov: *const libc::iovec, iovcnt: libc::c_int) -> libc::ssize_t => my_writev {
        // Check if this FD is stdout or stderr (or an alias) and we're in a safe state
        if is_hook_safe() {
            if let Some(stream_type) = get_stream_type_for_fd(fd) {
                eprintln!("[ah-command-trace-shim] writev hook called: fd={}, iovcnt={}, stream_type={}", fd, iovcnt, stream_type);

                if !iov.is_null() && iovcnt > 0 {
                    let mut total_size = 0usize;
                    let mut data_vec = Vec::new();

                    // Collect data from all iovecs
                    for i in 0..iovcnt as isize {
                        let iovec = *iov.offset(i);
                        if !iovec.iov_base.is_null() && iovec.iov_len > 0 {
                            let slice = std::slice::from_raw_parts(iovec.iov_base as *const u8, iovec.iov_len);
                            data_vec.extend_from_slice(slice);
                            total_size += iovec.iov_len;
                        }
                    }

                    if total_size > 0 {
                        eprintln!("[ah-command-trace-shim] captured writev: {} bytes to stream {}", total_size, stream_type);
                        // Use current process ID as command ID
                        let command_id = std::process::id() as u64;
                        send_command_chunk(command_id, stream_type, data_vec, None);
                    }
                }
            }
        }

        redhook::real!(writev)(fd, iov, iovcnt)
    }
}
