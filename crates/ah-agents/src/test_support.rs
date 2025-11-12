// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::env;
use std::ffi::{OsStr, OsString};

/// RAII guard for temporarily setting an environment variable.
/// On drop, restores the previous value (or removes the var if it didn't exist).
pub struct EnvVarGuard {
    key: String,
    old_value: Option<OsString>,
}

impl EnvVarGuard {
    /// Set `key` to `value` for the duration of the guard.
    /// When the guard is dropped, the env var is restored.
    pub fn set<K, V>(key: K, value: V) -> Self
    where
        K: Into<String>,
        V: AsRef<OsStr>,
    {
        let key_string = key.into();
        let old_value = env::var_os(&key_string);
        env::set_var(&key_string, value);
        EnvVarGuard {
            key: key_string,
            old_value,
        }
    }

    /// Remove `key` for the duration of the guard.
    /// When the guard is dropped, the env var is restored to its previous value.
    pub fn remove<K>(key: K) -> Self
    where
        K: Into<String>,
    {
        let key_string = key.into();
        let old_value = env::var_os(&key_string);
        env::remove_var(&key_string);
        EnvVarGuard {
            key: key_string,
            old_value,
        }
    }

    /// Set HOME to the given path for the duration of the guard.
    /// Convenience method for the most common use case.
    pub fn set_home(path: impl AsRef<OsStr>) -> Self {
        Self::set("HOME", path)
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.old_value {
            Some(v) => env::set_var(&self.key, v),
            None => env::remove_var(&self.key),
        }
    }
}
