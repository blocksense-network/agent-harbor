# ah-agents

Unified agent abstraction for Agent Harbor. Each agent backend implements the `AgentExecutor`
trait and can be launched via `ah agent start`.

## ACP client (Milestone 1 scaffold)

- Agent type `acp` wraps any ACP-compliant binary over stdio.
- Launch command resolution: `--acp-agent-cmd` → `AH_ACP_AGENT_CMD` → PATH search for
  `acp-agent` or `mock-agent` → fallback to `acp-agent`. Provide subcommand-style
  binaries directly (e.g., `--acp-agent-cmd "opencode acp"` or `"mock-agent --scenario ..."`).
- Version detection runs `<binary> <args> --version` and accepts noisy output
  (`mock-agent version v0.2.3` is parsed as `0.2.3`). Clear errors are surfaced when the
  binary is missing or not executable.
- Launch env passed through: `ACP_LLM_API`, `ACP_LLM_API_KEY`, `ACP_INITIAL_PROMPT`,
  `ACP_OUTPUT=json` (when `--output json*`), and `ACP_SNAPSHOT_CMD` (pre-wired to
  `ah agent fs snapshot`).
- Output parsing is intentionally minimal for now (text/log/error lines only); full ACP
  event translation will land with the transport milestone.
- Focused tests: `just test-acp-client` (or `cargo nextest run -p ah-agents -E 'test(\"^acp_client_\")'`).
