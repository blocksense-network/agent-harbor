# mock-agent (ACP mode)

Rust implementation of a deterministic ACP agent used for client-side protocol testing. It plays back Scenario-Format timelines over ACP stdio so real clients can validate session/update handling, file/permission callbacks, and terminal follower flows.

## CLI

```bash
cargo run -p mock-agent -- --scenario tests/tools/mock-agent-acp/scenarios/acp_echo.yaml
```

Key flags:

- `--scenario <PATH>` (repeatable): YAML file or directory; loader handles multiple sources and Levenshtein prompt matching.
- `--scenario-name`, `--session-id`, `--match-prompt`: scenario selection helpers.
- `--load-session`, `--image-support`, `--audio-support`, `--embedded-context`, `--mcp-http`, `--mcp-sse`: capability overrides that take precedence over scenario `acp.capabilities`.
- `--protocol-version`: advertised ACP protocol version (default `1`).

## SDK example client

Run the bundled client to sanity-check an agent binary (default: the local `mock-agent` build):

```bash
cargo run -p mock-agent --example acp_client -- \
  --scenario tests/tools/mock-agent-acp/scenarios/acp_echo.yaml
```

The client prints `session/update` streams and auto-approves permission/file/terminal requests to keep flows moving.

- Add image/audio to the first prompt: `--image-file path/to.png --audio-file path/to.wav`
- Send additional prompts interactively: type lines on stdin; EOF (Ctrl+D) exits.

## Test utility

`tests/tools/mock-agent-acp/run.sh` wraps the CLI for quick manual checks and accepts extra flags that are forwarded directly to the agent. Use it in integration tests that need deterministic ACP behavior without relying on the legacy Python mock-agent.
