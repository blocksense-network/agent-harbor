// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

fn main() {
    // FSEventStream* symbols live in CoreServices, so ensure we link against it.
    // This is only needed on macOS.
    if std::env::var("TARGET").unwrap().contains("apple") {
        println!("cargo:rustc-link-lib=framework=CoreServices");
    }
}
