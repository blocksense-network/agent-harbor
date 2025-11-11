// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

fn main() {
    // Link against CoreFoundation framework on macOS
    // The test_helper binary directly calls CoreFoundation functions
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
    }
}
