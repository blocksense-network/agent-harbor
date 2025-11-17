# ah-command-trace-shim

Cross-platform interposition shim for capturing command execution and output streams.

## Features

- Loads via `DYLD_INSERT_LIBRARIES` (macOS) or `LD_PRELOAD` (Linux).
- Performs a handshake with the recorder over a Unix socket using SSZ-compatible messages.
- Captures process execution and output streams.

## Environment Variables

| Variable              | Description                                                                                | Default   |
| --------------------- | ------------------------------------------------------------------------------------------ | --------- |
| `AH_CMDTRACE_ENABLED` | Enable (`1`/`true`) or disable (`0`/`false`) the shim.                                     | `true`    |
| `AH_CMDTRACE_SOCKET`  | Path to the recorder's Unix-domain socket. Required for operation.                         | _(unset)_ |
| `AH_CMDTRACE_LOG`     | When set to `0`/`false`, suppress shim diagnostics. Any other value keeps logging enabled. | `true`    |

## Development

Build the shim library:

```bash
cargo build -p ah-command-trace-shim
```

Run the smoke tests:

```bash
cargo test -p ah-command-trace-shim --test shim_injection_smoke
```

## Architecture

The shim uses a layered architecture:

- `src/lib.rs`: Cross-platform core logic and shared types
- `src/platform/macos.rs`: macOS-specific DYLD interposition
- `src/platform/linux.rs`: Linux-specific LD_PRELOAD interposition
- `src/unsupported.rs`: Stub implementation for unsupported platforms

The end-to-end tests are in a separate crate (`ah-command-trace-e2e-tests`) to avoid recursive injection during testing.
