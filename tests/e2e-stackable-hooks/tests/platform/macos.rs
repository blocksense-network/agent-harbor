// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! macOS-specific test utilities for shim injection

use std::path::PathBuf;
use std::process::Command;

/// Extension trait for Command to add shim library injection
pub trait CommandExt {
    fn with_shim_libraries(&mut self, libraries: &[PathBuf]) -> &mut Self;
}

impl CommandExt for Command {
    fn with_shim_libraries(&mut self, libraries: &[PathBuf]) -> &mut Self {
        if !libraries.is_empty() {
            let library_paths =
                libraries.iter().map(|lib| lib.to_string_lossy()).collect::<Vec<_>>().join(":");

            self.env("DYLD_INSERT_LIBRARIES", &library_paths);
        }
        self
    }
}
