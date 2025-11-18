// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::env;
use std::io::{self, Write};
use std::path::PathBuf;

fn main() {
    // Find the shim library path
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let target_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(&profile);

    #[cfg(target_os = "macos")]
    let shim_name = "libah_command_trace_shim.dylib";

    #[cfg(target_os = "linux")]
    let shim_name = "libah_command_trace_shim.so";

    let shim_src = target_dir.join("deps").join(shim_name);
    let shim_dst = target_dir.join(shim_name);

    // Copy the shim library to a predictable location
    if shim_src.exists() {
        if let Err(e) = std::fs::copy(&shim_src, &shim_dst) {
            let _ = writeln!(io::stderr(), "Warning: Failed to copy shim library: {}", e);
        } else {
            let _ = writeln!(
                io::stdout(),
                "Copied shim library to: {}",
                shim_dst.display()
            );
        }
    } else {
        let _ = writeln!(
            io::stdout(),
            "Warning: Shim library not found at: {}",
            shim_src.display()
        );
        let _ = writeln!(
            io::stdout(),
            "Make sure to build the ah-command-trace-shim crate first"
        );
    }

    // Re-run build script if the shim library changes
    let _ = writeln!(
        io::stdout(),
        "cargo:rerun-if-changed=../ah-command-trace-shim/src"
    );
    let _ = writeln!(
        io::stdout(),
        "cargo:rerun-if-changed=../ah-command-trace-shim/Cargo.toml"
    );
}
