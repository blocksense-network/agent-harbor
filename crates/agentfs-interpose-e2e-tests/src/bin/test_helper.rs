// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

#[cfg(target_os = "macos")]
#[path = "../mac/test_helper_macos.rs"]
mod test_helper_macos;

#[cfg(target_os = "macos")]
fn main() {
    test_helper_macos::main();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    tracing::warn!("agentfs-interpose-test-helper is only available on macOS");
    std::process::exit(1);
}
