// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_rest_server::acp::translator::{AcpCapabilities, JsonRpcTranslator};
use ah_rest_server::config::{AcpConfig, AcpTransportMode};
use proptest::prelude::*;
use serde_json::json;

#[test]
fn acp_initialize_caps_roundtrip() {
    let mut cfg = AcpConfig::default();
    cfg.transport = AcpTransportMode::WebSocket;
    let caps = JsonRpcTranslator::negotiate_caps(&cfg);
    assert_eq!(
        caps,
        AcpCapabilities {
            transports: vec!["websocket".into()],
            fs_read: false,
            fs_write: false,
            terminals: true
        }
    );

    let payload = JsonRpcTranslator::initialize_response(&caps);
    assert_eq!(payload["capabilities"]["transports"][0], "websocket");
    assert_eq!(payload["capabilities"]["filesystem"]["readTextFile"], false);

    // Unknown flags should be ignored
    let noisy = json!({
        "capabilities": {
            "filesystem": {
                "readTextFile": true,
                "unknownFlag": true
            },
            "terminal": true,
            "transports": ["websocket", "stdio"],
            "extra": {"foo":"bar"}
        }
    });
    let parsed = JsonRpcTranslator::ignore_unknown_caps(&noisy);
    assert_eq!(parsed.transports, vec!["websocket", "stdio"]);
    assert!(parsed.fs_read);
    assert!(parsed.terminals);
}

proptest! {
    #[test]
    fn unknown_capabilities_are_ignored_but_known_fields_respected(fs_read in proptest::bool::ANY, fs_write in proptest::bool::ANY) {
        let noisy = json!({
            "capabilities": {
                "filesystem": {
                    "readTextFile": fs_read,
                    "writeTextFile": fs_write,
                    "someFutureFlag": true
                },
                "terminal": true,
                "transports": ["websocket"],
                "experimental": {"foo": "bar"}
            }
        });
        let parsed = JsonRpcTranslator::ignore_unknown_caps(&noisy);
        prop_assert_eq!(parsed.fs_read, fs_read);
        prop_assert_eq!(parsed.fs_write, fs_write);
        prop_assert!(parsed.transports.contains(&"websocket".into()));
    }
}
