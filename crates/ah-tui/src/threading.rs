// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Reusable TUI threading harness (single UI thread + LocalSet).
//! Matches specs/Public/TUI-Threading.md.

use std::thread;
use tokio::runtime::Builder;
use tokio::task::LocalSet;

pub struct UiRuntime;

impl UiRuntime {
    /// Run the provided async block on a single UI thread with current-thread runtime + LocalSet.
    pub fn run<F>(fut: F) -> anyhow::Result<()>
    where
        F: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let handle = thread::Builder::new().name("tui-main".into()).spawn(|| {
            let rt = Builder::new_current_thread().enable_all().build().expect("ui runtime");
            let local = LocalSet::new();
            local.block_on(&rt, fut)
        })?;

        handle.join().map_err(|e| anyhow::anyhow!("ui thread panicked: {:?}", e))?
    }
}
