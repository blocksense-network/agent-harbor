// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_recorder::AhrEvent;
use ah_recorder::reader::AhrReader;
use serde::{Deserialize, Serialize};
use std::env;
use tracing::{error, info};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum EventJson {
    #[serde(rename = "data")]
    Data {
        index: usize,
        timestamp_ns: u64,
        byte_offset: u64,
        data: String,
    },
    #[serde(rename = "snapshot")]
    Snapshot {
        index: usize,
        timestamp_ns: u64,
        label: Option<String>,
    },
    #[serde(rename = "resize")]
    Resize {
        index: usize,
        timestamp_ns: u64,
        cols: u16,
        rows: u16,
    },
}

fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        error!(usage = %format!("{} <ahr_file>", args[0]), "invalid arguments");
        std::process::exit(1);
    }

    let mut reader = AhrReader::new(&args[1])?;
    let events = reader.read_all_events()?;

    // Output each event as a separate JSON line (JSONL format)
    for (i, event) in events.iter().enumerate() {
        let json_event = match event {
            AhrEvent::Data {
                ts_ns,
                start_byte_off,
                data,
            } => {
                let text = String::from_utf8_lossy(data).to_string();
                EventJson::Data {
                    index: i,
                    timestamp_ns: *ts_ns,
                    byte_offset: *start_byte_off,
                    data: text,
                }
            }
            AhrEvent::Snapshot(snapshot) => EventJson::Snapshot {
                index: i,
                timestamp_ns: snapshot.ts_ns,
                label: snapshot.label.clone(),
            },
            AhrEvent::Resize { ts_ns, cols, rows } => EventJson::Resize {
                index: i,
                timestamp_ns: *ts_ns,
                cols: *cols,
                rows: *rows,
            },
        };

        // Output each event as a separate JSON line
        info!(
            event = serde_json::to_string(&json_event)?,
            "ahr event json"
        );
    }

    Ok(())
}
