// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared POSIX functionality for command trace interposition

use crate::core::{self, SHIM_STATE, ShimState};
use ah_command_trace_client::{ClientConfig, CommandTraceClient};
use ah_command_trace_proto::CommandStart;
use std::ffi::CStr;
use std::sync::Mutex;
use std::time::SystemTime;

/// Global client connection to the command trace server
static CLIENT: Mutex<Option<CommandTraceClient>> = Mutex::new(None);

/// Initialize the client connection to the command trace server
pub fn initialize_client() -> Result<(), Box<dyn std::error::Error>> {
    let state = SHIM_STATE.get().and_then(|s| s.lock().ok());

    match state.as_ref().map(|s| &**s) {
        Some(ShimState::Ready { socket_path, .. }) => {
            let config = ClientConfig::builder("ah-command-trace-shim", env!("CARGO_PKG_VERSION"))
                .build()
                .map_err(|e| format!("Failed to build client config: {}", e))?;

            match CommandTraceClient::connect(socket_path.as_ref(), &config) {
                Ok(client) => {
                    *CLIENT.lock().unwrap() = Some(client);
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
    unsafe fn fork() -> libc::pid_t => my_fork {
        eprintln!("[ah-command-trace-shim] fork hook called!");
        let result = redhook::real!(fork)();

        if result == 0 {
            // Child process - send CommandStart if we're tracking
            if let Some(ref state) = SHIM_STATE.get().and_then(|s| s.lock().ok()) {
                if matches!(**state, ShimState::Ready { .. }) {
                    // Get process info
                    let pid = std::process::id() as u32;
                    let ppid = unsafe { libc::getppid() } as u32;
                    let executable = std::env::current_exe()
                        .ok()
                        .and_then(|p| p.to_str().map(|s| s.as_bytes().to_vec()))
                        .unwrap_or_else(|| b"<unknown>".to_vec());

                    let args: Vec<Vec<u8>> = std::env::args()
                        .map(|s| s.into_bytes())
                        .collect();

                    let env = get_environment();
                    let cwd = get_current_dir();

                    send_command_start(pid, ppid, &executable, args, env, cwd);
                }
            }
        }

        result
    }
}

redhook::hook! {
    unsafe fn execve(path: *const libc::c_char, argv: *const *mut libc::c_char, envp: *const *mut libc::c_char) -> libc::c_int => my_execve {
        eprintln!("[ah-command-trace-shim] execve hook called!");
        // Get process info BEFORE calling execve (since execve replaces the process)
        if let Some(ref state) = SHIM_STATE.get().and_then(|s| s.lock().ok()) {
            if matches!(**state, ShimState::Ready { .. }) {
                let pid = std::process::id() as u32;
                let ppid = unsafe { libc::getppid() } as u32;

                let (executable, args, env, cwd) = extract_command_info(path, argv, envp);

                send_command_start(pid, ppid, &executable, args, env, cwd);
            }
        }

        redhook::real!(execve)(path, argv, envp)
    }
}

redhook::hook! {
    unsafe fn execvp(file: *const libc::c_char, argv: *const *mut libc::c_char) -> libc::c_int => my_execvp {
        eprintln!("[ah-command-trace-shim] execvp hook called!");
        // Get process info BEFORE calling execvp (since execvp replaces the process)
        if let Some(ref state) = SHIM_STATE.get().and_then(|s| s.lock().ok()) {
            if matches!(**state, ShimState::Ready { .. }) {
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
        }

        redhook::real!(execvp)(file, argv)
    }
}

redhook::hook! {
    unsafe fn execv(file: *const libc::c_char, argv: *const *mut libc::c_char) -> libc::c_int => my_execv {
        eprintln!("[ah-command-trace-shim] execv hook called!");
        // Get process info BEFORE calling execv (since execv replaces the process)
        if let Some(ref state) = SHIM_STATE.get().and_then(|s| s.lock().ok()) {
            if matches!(**state, ShimState::Ready { .. }) {
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
        }

        redhook::real!(execv)(file, argv)
    }
}

redhook::hook! {
    unsafe fn posix_spawn(pid: *mut libc::pid_t, path: *const libc::c_char, file_actions: *const libc::posix_spawn_file_actions_t, attrp: *const libc::posix_spawnattr_t, argv: *const *mut libc::c_char, envp: *const *mut libc::c_char) -> libc::c_int => my_posix_spawn {
        eprintln!("[ah-command-trace-shim] posix_spawn hook called!");
        // Call the real posix_spawn first
        let result = redhook::real!(posix_spawn)(pid, path, file_actions, attrp, argv, envp);

        // If spawn was successful and we have a PID, send CommandStart
        if result == 0 && !pid.is_null() {
            if let Some(ref state) = SHIM_STATE.get().and_then(|s| s.lock().ok()) {
                if matches!(**state, ShimState::Ready { .. }) {
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
            }
        }

        result
    }
}

redhook::hook! {
    unsafe fn posix_spawnp(pid: *mut libc::pid_t, file: *const libc::c_char, file_actions: *const libc::posix_spawn_file_actions_t, attrp: *const libc::posix_spawnattr_t, argv: *const *mut libc::c_char, envp: *const *mut libc::c_char) -> libc::c_int => my_posix_spawnp {
        // Call the real posix_spawnp first
        let result = redhook::real!(posix_spawnp)(pid, file, file_actions, attrp, argv, envp);

        // If spawn was successful and we have a PID, send CommandStart
        if result == 0 && !pid.is_null() {
            if let Some(ref state) = SHIM_STATE.get().and_then(|s| s.lock().ok()) {
                if matches!(**state, ShimState::Ready { .. }) {
                    let child_pid = unsafe { *pid } as u32;
                    let parent_pid = std::process::id() as u32;

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
        }

        result
    }
}
