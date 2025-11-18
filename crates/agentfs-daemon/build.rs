// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

fn main() {
    // Link against CoreFoundation framework on macOS for CFMessagePort
    #[cfg(target_os = "macos")]
    {
        use std::io::{self, Write};
        // Emit cargo directive via stdout to avoid disallowed println!
        writeln!(
            io::stdout(),
            "cargo:rustc-link-lib=framework=CoreFoundation"
        )
        .unwrap();
    }
}
