//! macOS Seatbelt (SBPL) profile utilities and libsandbox FFI bindings.
//! 
//! This crate provides:
//! - A minimal builder for Seatbelt SBPL profiles focusing on default deny and path-based allowances
//! - Safe wrappers around `sandbox_init` / `sandbox_free_error` (best-effort; API is deprecated but present)
//! - A helper to apply a sandbox profile to the current process after optional `chroot(2)`
//! 
//! Notes:
//! - These APIs are only meaningful on macOS; on other platforms they compile to stubs that error.
//! - `sandbox_init` is deprecated; when possible, prefer using Endpoint Security for dynamic policies.

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int};
    use std::ptr;

    #[link(name = "sandbox")]
    extern "C" {
        // int sandbox_init(const char *profile, uint64_t flags, char **errorbuf);
        fn sandbox_init(profile: *const c_char, flags: u64, errorbuf: *mut *mut c_char) -> c_int;
        // void sandbox_free_error(char *errorbuf);
        fn sandbox_free_error(errorbuf: *mut c_char);
    }

    #[derive(thiserror::Error, Debug)]
    pub enum Error {
        #[error("sandbox_init failed: {0}")]
        SandboxInit(String),
        #[error("invalid profile string")]
        InvalidProfile,
    }

    pub type Result<T> = std::result::Result<T, Error>;

    /// Apply a seatbelt profile to the current process.
    pub fn apply_profile(profile: &str) -> Result<()> {
        let c_profile = CString::new(profile).map_err(|_| Error::InvalidProfile)?;
        let mut err_buf: *mut c_char = ptr::null_mut();
        let rc = unsafe { sandbox_init(c_profile.as_ptr(), 0, &mut err_buf) };
        if rc != 0 {
            let msg = if !err_buf.is_null() {
                let c = unsafe { CStr::from_ptr(err_buf) };
                let s = c.to_string_lossy().into_owned();
                unsafe { sandbox_free_error(err_buf) };
                s
            } else {
                "unknown error".to_string()
            };
            return Err(Error::SandboxInit(msg));
        }
        Ok(())
    }

    /// Simple SBPL builder with deny default and path allowances.
    #[derive(Debug, Clone)]
    pub struct SbplBuilder {
        allow_read_subpaths: Vec<String>,
        allow_write_subpaths: Vec<String>,
        allow_exec_subpaths: Vec<String>,
        deny_network: bool,
        deny_process_info_global: bool,
        allow_signal_same_group: bool,
            deny_apple_events: bool,
            deny_mach_lookup: bool,
    }

    impl SbplBuilder {
        pub fn new() -> Self {
            Self {
                allow_read_subpaths: Vec::new(),
                allow_write_subpaths: Vec::new(),
                allow_exec_subpaths: Vec::new(),
                deny_network: true,
                deny_process_info_global: false,
                allow_signal_same_group: false,
                deny_apple_events: false,
                deny_mach_lookup: false,
            }
        }

        pub fn allow_read_subpath(mut self, path: impl Into<String>) -> Self {
            self.allow_read_subpaths.push(path.into());
            self
        }

        pub fn allow_write_subpath(mut self, path: impl Into<String>) -> Self {
            self.allow_write_subpaths.push(path.into());
            self
        }

        pub fn allow_exec_subpath(mut self, path: impl Into<String>) -> Self {
            self.allow_exec_subpaths.push(path.into());
            self
        }

        /// Allow network (by default network is denied to align with egress-off-by-default)
        pub fn allow_network(mut self) -> Self {
            self.deny_network = false;
            self
        }

        /// Deny process-info globally; selectively allow self.
        pub fn harden_process_info(mut self) -> Self {
            self.deny_process_info_global = true;
            self
        }

        /// Allow sending signals to same process group only.
        pub fn allow_signal_same_group(mut self) -> Self {
            self.allow_signal_same_group = true;
            self
        }

        /// Deny Apple Events sending.
        pub fn deny_apple_events(mut self) -> Self {
            self.deny_apple_events = true;
            self
        }

        /// Deny Mach service lookup by default.
        pub fn deny_mach_lookup(mut self) -> Self {
            self.deny_mach_lookup = true;
            self
        }

        pub fn build(self) -> String {
            let mut lines = Vec::new();
            lines.push("(version 1)".to_string());
            lines.push("(deny default)".to_string());

            // Filesystem base rules: explicit write operations denied by default
            lines.push("(deny file-write*)".to_string());
            lines.push("(deny process-exec)".to_string());

            // Allowances by subpath
            for p in self.allow_read_subpaths {
                lines.push(format!("(allow file-read* (subpath \"{}\"))", p));
            }
            for p in self.allow_write_subpaths {
                // Allow all write classes under the subpath (create, data, unlink, mode, owner, times)
                lines.push(format!("(allow file-write* (subpath \"{}\"))", p));
                lines.push(format!("(allow file-read* (subpath \"{}\"))", p));
            }
            for p in self.allow_exec_subpaths {
                lines.push(format!("(allow process-exec (subpath \"{}\"))", p));
                lines.push(format!("(allow file-read* (subpath \"{}\"))", p));
            }

            // Networking: deny by default
            if self.deny_network {
                lines.push("(deny network*)".to_string());
            }

            // Process info and signals
            if self.deny_process_info_global {
                lines.push("(deny process-info*)".to_string());
                lines.push("(allow process-info-pidinfo (target self))".to_string());
                lines.push("(allow process-info-setcontrol (target self))".to_string());
            }

            if self.allow_signal_same_group {
                lines.push("(allow signal (target self))".to_string());
                lines.push("(allow signal (target same-group))".to_string());
                lines.push("(deny signal (target others))".to_string());
            }

            if self.deny_apple_events {
                lines.push("(deny appleevent-send)".to_string());
            }

            if self.deny_mach_lookup {
                lines.push("(deny mach-lookup)".to_string());
            }

            lines.join("\n")
        }
    }

    /// Apply a profile built by `SbplBuilder`.
    pub fn apply_builder(builder: SbplBuilder) -> Result<()> {
        apply_profile(&builder.build())
    }
}

#[cfg(not(target_os = "macos"))]
mod macos {
    #[derive(thiserror::Error, Debug)]
    pub enum Error {
        #[error("macOS-only functionality is unavailable on this platform")] 
        Unavailable,
    }
    pub type Result<T> = std::result::Result<T, Error>;

    #[derive(Default, Debug, Clone)]
    pub struct SbplBuilder;
    impl SbplBuilder {
        pub fn new() -> Self { Self }
        pub fn allow_read_subpath(self, _p: impl Into<String>) -> Self { self }
        pub fn allow_write_subpath(self, _p: impl Into<String>) -> Self { self }
        pub fn allow_exec_subpath(self, _p: impl Into<String>) -> Self { self }
        pub fn loopback_only(self) -> Self { self }
        pub fn harden_process_info(self) -> Self { self }
        pub fn allow_signal_same_group(self) -> Self { self }
        pub fn build(self) -> String { String::new() }
    }
    pub fn apply_profile(_: &str) -> Result<()> { Err(Error::Unavailable) }
    pub fn apply_builder(_: SbplBuilder) -> Result<()> { Err(Error::Unavailable) }
}

pub use macos::*;


