# agentfs-interpose-shim

Minimal DYLD interposer shim for macOS that bootstraps AgentFS interpose sessions.

## Features

* Loads via `DYLD_INSERT_LIBRARIES`.
* Performs a guarded handshake with the AgentFS control socket.
* Optional allow-list to restrict which executables activate the shim (development safety).

## Environment Variables

| Variable | Description | Default |
| --- | --- | --- |
| `AGENTFS_INTERPOSE_ENABLED` | Enable (`1`/`true`) or disable (`0`/`false`) the shim. | Enabled |
| `AGENTFS_INTERPOSE_SOCKET` | Path to the AgentFS control UNIX-domain socket. Required for handshake. | *(unset)* |
| `AGENTFS_INTERPOSE_ALLOWLIST` | Comma-separated allow-list of executable basenames or path fragments. `*` allows all. | *(allow all)* |
| `AGENTFS_INTERPOSE_LOG` | When set to `0`/`false`, suppress shim diagnostics. Any other value keeps logging enabled. | Logging enabled |

## Handshake

On initialization the shim connects to the configured socket and emits a JSON
handshake payload with process metadata and allow-list details. The connection is
kept alive for future coordination. A simple newline-delimited JSON ACK is
expected but optional—the shim logs failures gracefully.

## Developing

```
cargo test -p agentfs-interpose-shim
```

The integration tests spin up a temporary UNIX socket and launch a tiny helper
process with `DYLD_INSERT_LIBRARIES` to verify the handshake and allow-list guard.
