// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ah_core::workspace_terms_enumerator::{
    DefaultWorkspaceTermsEnumerator, WorkspaceTermsEnumerator,
};
use ah_repo::VcsRepo;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn wait_until_ready(enumerator: &DefaultWorkspaceTermsEnumerator) {
    assert!(
        enumerator.wait_until_ready(Duration::from_secs(30)),
        "workspace terms index did not build within the timeout"
    );
}

#[test]
#[ignore = "benchmark test – runs against the full agent-harbor repository"]
fn benchmark_workspace_terms_indexing() {
    let repo = Arc::new(VcsRepo::new(repo_root()).expect("open repo"));
    let enumerable: Arc<dyn ah_core::WorkspaceFilesEnumerator> = repo.clone();

    let start = Instant::now();
    let enumerator = DefaultWorkspaceTermsEnumerator::new(enumerable);
    wait_until_ready(&enumerator);
    println!(
        "[workspace_terms] initial index build completed in {:.2?}",
        start.elapsed()
    );
}

#[test]
#[ignore = "benchmark test – runs against the full agent-harbor repository"]
fn benchmark_workspace_terms_lookup() {
    let repo = Arc::new(VcsRepo::new(repo_root()).expect("open repo"));
    let enumerable: Arc<dyn ah_core::WorkspaceFilesEnumerator> = repo.clone();
    let enumerator = DefaultWorkspaceTermsEnumerator::new(enumerable);
    wait_until_ready(&enumerator);

    for prefix in ["workspace", "src/", "README", "crates/ah-tui"] {
        let start = Instant::now();
        let outcome = enumerator.lookup(prefix, 64);
        println!(
            "[workspace_terms] lookup '{prefix}' -> {} matches (shared='{}', shortest='{}') in {:.2?}",
            outcome.entries.len(),
            outcome.shared_extension,
            outcome.shortest_completion,
            start.elapsed()
        );
    }
}
