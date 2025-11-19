// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::io::{self, Write};
use tracing::info;
use tracing_subscriber::{Layer, Registry, layer::SubscriberExt};

struct CargoLayer;

impl<S> Layer<S> for CargoLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        struct MsgVisitor {
            msg: Option<String>,
        }
        impl tracing::field::Visit for MsgVisitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    let raw = format!("{value:?}");
                    let cleaned = raw
                        .strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                        .unwrap_or(&raw)
                        .to_string();
                    self.msg = Some(cleaned);
                }
            }
        }
        let mut visitor = MsgVisitor { msg: None };
        event.record(&mut visitor);
        if let Some(m) = visitor.msg {
            let mut stdout = io::stdout();
            let _ = writeln!(stdout, "{}", m);
        }
    }
}

fn init_tracing() {
    let subscriber = Registry::default().with(CargoLayer);
    let _ = tracing::subscriber::set_global_default(subscriber);
}

fn main() {
    init_tracing();
    // Link against CoreFoundation framework on macOS for CFMessagePort
    #[cfg(target_os = "macos")]
    {
        info!("cargo:rustc-link-lib=framework=CoreFoundation");
    }
}
