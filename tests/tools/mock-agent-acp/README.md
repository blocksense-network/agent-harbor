# Mock Agent (ACP) Test Utility

This directory contains a minimal Scenario-Format fixture and helper script for exercising the Rust `mock-agent` ACP mode.

## Quick start

```bash
# Run mock-agent with echo scenario (agent-only; pair with a client)
./tests/tools/mock-agent-acp/run.sh

# Drive it end-to-end with the bundled SDK example client
cargo run -p mock-agent --example acp_client -- \
  --scenario tests/tools/mock-agent-acp/scenarios/acp_echo.yaml

# Override the scenario and pass capability flags through
./tests/tools/mock-agent-acp/run.sh ./tests/tools/mock-agent-acp/scenarios/acp_echo.yaml --image-support

# Try richer flows
./tests/tools/mock-agent-acp/run.sh ./tests/tools/mock-agent-acp/scenarios/acp_permission_and_read.yaml
./tests/tools/mock-agent-acp/run.sh ./tests/tools/mock-agent-acp/scenarios/acp_terminal.yaml
./tests/tools/mock-agent-acp/run.sh ./tests/tools/mock-agent-acp/scenarios/acp_loadsession_meta.yaml --load-session
./tests/tools/mock-agent-acp/run.sh ./tests/tools/mock-agent-acp/scenarios/acp_meta_multimodal.yaml --image-support --audio-support
```

`run.sh` delegates to `cargo run -p mock-agent -- …`, so it rebuilds the crate if needed. All additional arguments after the scenario path are forwarded directly to the mock-agent CLI (e.g., `--match-prompt`, `--session-id`, `--load-session`).

## Demo scenario

- `scenarios/acp_echo.yaml` sends a single user input, then streams an assistant reply after `sessionStart`. Use it to sanity-check ACP stdio wiring with the new SDK example client (`cargo run -p mock-agent --example acp_client -- --scenario …`).
