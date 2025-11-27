// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// Use of println is fine in Cargo build scripts.
#[allow(clippy::disallowed_methods)]
fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env == "gnu" {
        // Allow multiple definitions so that low-priority propagation hooks and
        // application hooks can both provide the same interposed symbols. The
        // dispatcher resolves the call chain at runtime, so duplicate symbol
        // definitions are intentional.
        println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
    }

    // Let the library itself know that it should export dispatcher symbols.
    println!("cargo:rustc-cfg=stackable_hooks_internal_export");
    println!("cargo:rustc-check-cfg=cfg(stackable_hooks_internal_export)");
}
