// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Linux-specific test utilities for shim injection

use std::path::PathBuf;
use std::process::Command;

/// Extension trait for Command to add shim library injection
#[allow(dead_code)]
pub trait CommandExt {
    fn with_shim_libraries(self, libraries: &[PathBuf]) -> Self;
}

impl CommandExt for Command {
    fn with_shim_libraries(mut self, libraries: &[PathBuf]) -> Self {
        if !libraries.is_empty() {
            let library_paths =
                libraries.iter().map(|lib| lib.to_string_lossy()).collect::<Vec<_>>().join(":");

            self.env("LD_PRELOAD", &library_paths);
        }
        self
    }
}
