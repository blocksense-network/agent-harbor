// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Low-priority hooks for subprocess spawning that automatically propagate library injection.
//!
//! These hooks intercept subprocess spawning functions (execve, posix_spawn, etc.) and
//! automatically modify the environment to include LD_PRELOAD or DYLD_INSERT_LIBRARIES
//! when auto-propagation is enabled.
//!
//! The hooks use a low priority (1000) to ensure they run after application-specific hooks.

// Suppress expected warnings from macro-generated code
#![allow(non_camel_case_types)]
#![allow(private_interfaces)]
#![allow(static_mut_refs)]

use crate::auto_propagation::{free_modified_envp, modify_envp_with_injection};
#[allow(unused_imports)]
use crate::{call_next, hook};

// Use a low priority (high number) so these hooks run after application hooks
const PROPAGATION_PRIORITY: i32 = 1000;

// execve: int execve(const char *pathname, char *const argv[], char *const envp[])
//
// Reference: https://man7.org/linux/man-pages/man2/execve.2.html
// This is the fundamental exec system call. All other exec variants eventually call this.
hook! {
    priority: PROPAGATION_PRIORITY,
    unsafe fn execve(
        pathname: *const libc::c_char,
        argv: *const *mut libc::c_char,
        envp: *const *mut libc::c_char
    ) -> libc::c_int => propagate_execve {
        // Modify the environment to include library injection
        let modified_envp = modify_envp_with_injection(envp);

        // Use modified environment if available, otherwise use original
        let final_envp = if !modified_envp.is_null() {
            modified_envp as *const *mut libc::c_char
        } else {
            envp
        };

        // Call the next hook in the chain
        let result = call_next!(pathname, argv, final_envp);

        // Clean up the modified environment
        if !modified_envp.is_null() {
            free_modified_envp(modified_envp);
        }

        result
    }
}

// execvp: int execvp(const char *file, char *const argv[])
//
// Reference: https://man7.org/linux/man-pages/man3/exec.3.html
// This variant searches for the file in PATH and uses the current environment.
// We need to hook this to ensure propagation even when the caller doesn't specify envp.
#[cfg(any(target_os = "macos", target_os = "linux"))]
hook! {
    priority: PROPAGATION_PRIORITY,
    unsafe fn execvp(
        file: *const libc::c_char,
        argv: *const *mut libc::c_char
    ) -> libc::c_int => propagate_execvp {
        // Note: execvp doesn't take an envp parameter, so we can't modify it here.
        // However, we've already modified the process environment via setenv if needed.
        // Just pass through to the next hook.
        call_next!(file, argv)
    }
}

// execv: int execv(const char *path, char *const argv[])
//
// Reference: https://man7.org/linux/man-pages/man3/exec.3.html
// Similar to execvp but doesn't search PATH.
#[cfg(any(target_os = "macos", target_os = "linux"))]
hook! {
    priority: PROPAGATION_PRIORITY,
    unsafe fn execv(
        path: *const libc::c_char,
        argv: *const *mut libc::c_char
    ) -> libc::c_int => propagate_execv {
        // Like execvp, this uses the current environment, so just pass through.
        call_next!(path, argv)
    }
}

// execvpe: int execvpe(const char *file, char *const argv[], char *const envp[])
//
// Reference: https://man7.org/linux/man-pages/man3/exec.3.html
// GNU extension that searches PATH and accepts envp.
#[cfg(target_os = "linux")]
hook! {
    priority: PROPAGATION_PRIORITY,
    unsafe fn execvpe(
        file: *const libc::c_char,
        argv: *const *mut libc::c_char,
        envp: *const *mut libc::c_char
    ) -> libc::c_int => propagate_execvpe {
        let modified_envp = modify_envp_with_injection(envp);

        let final_envp = if !modified_envp.is_null() {
            modified_envp as *const *mut libc::c_char
        } else {
            envp
        };

        let result = call_next!(file, argv, final_envp);

        if !modified_envp.is_null() {
            free_modified_envp(modified_envp);
        }

        result
    }
}

// execveat: int execveat(int dirfd, const char *pathname, char *const argv[],
//                        char *const envp[], int flags)
//
// Reference: https://man7.org/linux/man-pages/man2/execveat.2.html
// Linux-specific variant that can execute relative to a directory fd.
#[cfg(target_os = "linux")]
hook! {
    priority: PROPAGATION_PRIORITY,
    unsafe fn execveat(
        dirfd: libc::c_int,
        pathname: *const libc::c_char,
        argv: *const *mut libc::c_char,
        envp: *const *mut libc::c_char,
        flags: libc::c_int
    ) -> libc::c_int => propagate_execveat {
        let modified_envp = modify_envp_with_injection(envp);

        let final_envp = if !modified_envp.is_null() {
            modified_envp as *const *mut libc::c_char
        } else {
            envp
        };

        let result = call_next!(dirfd, pathname, argv, final_envp, flags);

        if !modified_envp.is_null() {
            free_modified_envp(modified_envp);
        }

        result
    }
}

// posix_spawn: int posix_spawn(pid_t *pid, const char *path,
//                               const posix_spawn_file_actions_t *file_actions,
//                               const posix_spawnattr_t *attrp,
//                               char *const argv[], char *const envp[])
//
// Reference: https://pubs.opengroup.org/onlinepubs/9699919799/functions/posix_spawn.html
// POSIX standard function for spawning processes with more control than fork+exec.
#[cfg(any(target_os = "macos", target_os = "linux"))]
hook! {
    priority: PROPAGATION_PRIORITY,
    unsafe fn posix_spawn(
        pid: *mut libc::pid_t,
        path: *const libc::c_char,
        file_actions: *const libc::posix_spawn_file_actions_t,
        attrp: *const libc::posix_spawnattr_t,
        argv: *const *mut libc::c_char,
        envp: *const *mut libc::c_char
    ) -> libc::c_int => propagate_posix_spawn {
        let modified_envp = modify_envp_with_injection(envp);

        let final_envp = if !modified_envp.is_null() {
            modified_envp as *const *mut libc::c_char
        } else {
            envp
        };

        let result = call_next!(pid, path, file_actions, attrp, argv, final_envp);

        if !modified_envp.is_null() {
            free_modified_envp(modified_envp);
        }

        result
    }
}

// posix_spawnp: int posix_spawnp(pid_t *pid, const char *file,
//                                 const posix_spawn_file_actions_t *file_actions,
//                                 const posix_spawnattr_t *attrp,
//                                 char *const argv[], char *const envp[])
//
// Reference: https://pubs.opengroup.org/onlinepubs/9699919799/functions/posix_spawn.html
// Like posix_spawn but searches PATH for the file.
#[cfg(any(target_os = "macos", target_os = "linux"))]
hook! {
    priority: PROPAGATION_PRIORITY,
    unsafe fn posix_spawnp(
        pid: *mut libc::pid_t,
        file: *const libc::c_char,
        file_actions: *const libc::posix_spawn_file_actions_t,
        attrp: *const libc::posix_spawnattr_t,
        argv: *const *mut libc::c_char,
        envp: *const *mut libc::c_char
    ) -> libc::c_int => propagate_posix_spawnp {
        let modified_envp = modify_envp_with_injection(envp);

        let final_envp = if !modified_envp.is_null() {
            modified_envp as *const *mut libc::c_char
        } else {
            envp
        };

        let result = call_next!(pid, file, file_actions, attrp, argv, final_envp);

        if !modified_envp.is_null() {
            free_modified_envp(modified_envp);
        }

        result
    }
}
