// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Test shim that hooks posix_spawn to verify stackable-interpose works with it

#[cfg(target_os = "macos")]
use stackable_interpose::{enable_hooks, hook};

#[cfg(target_os = "macos")]
#[ctor::ctor]
fn init_hooks() {
    eprintln!("[test-posix-spawn-hook] Initializing hooks");
    enable_hooks();
}

#[cfg(target_os = "macos")]
hook! {
    unsafe fn posix_spawn(
        stackable_self,
        pid: *mut libc::pid_t,
        path: *const libc::c_char,
        file_actions: *const libc::posix_spawn_file_actions_t,
        attrp: *const libc::posix_spawnattr_t,
        argv: *const *mut libc::c_char,
        envp: *const *mut libc::c_char
    ) -> libc::c_int => my_posix_spawn {
        eprintln!("[test-posix-spawn-hook] posix_spawn hook called!");

        // Call the real function
        let result = stackable_interpose::call_next!(
            stackable_self,
            posix_spawn,
            pid,
            path,
            file_actions,
            attrp,
            argv,
            envp
        );

        if result == 0 && !pid.is_null() {
            let child_pid = unsafe { *pid };
            eprintln!("[test-posix-spawn-hook] posix_spawn succeeded, child PID: {}", child_pid);
        }

        result
    }
}

#[cfg(not(target_os = "macos"))]
pub fn dummy_function() {}
