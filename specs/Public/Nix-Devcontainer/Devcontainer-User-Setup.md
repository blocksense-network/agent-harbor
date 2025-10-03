# DevContainer Environment Setup

## Required Environment Variables

Set these environment variables on your host system before starting the devcontainer:

### OpenAI Integration

```bash
export OPENAI_API_KEY="your-openai-api-key"
export OPENAI_ORG_ID="your-org-id"  # Optional
```

### Git Hosting Integration

Set tokens for any providers you intend to push to. The setup script will create
a `~/.netrc` file from these values.

```bash
export GITHUB_TOKEN="your-github-token"    # Optional
export GITLAB_TOKEN="your-gitlab-token"    # Optional
export BITBUCKET_TOKEN="your-bitbucket-token"  # Optional
```

## Setting Environment Variables

### Linux/macOS

Add to your `~/.bashrc`, `~/.zshrc`, or equivalent:

```bash
export OPENAI_API_KEY="sk-..."
export GITHUB_TOKEN="ghp_..."
export GITLAB_TOKEN="glpat-..."
export BITBUCKET_TOKEN="bbt_..."
```

### Windows

Using PowerShell:

```powershell
$env:OPENAI_API_KEY = "sk-..."
$env:GITHUB_TOKEN = "ghp_..."
$env:GITLAB_TOKEN = "glpat-..."
$env:BITBUCKET_TOKEN = "bbt_..."
```

Or set permanently via System Properties > Environment Variables.

## Persistent Cache Benefits

The devcontainer uses Docker volumes for caching:

- **Nix Store**: Packages persist between container rebuilds
- **Cargo Cache**: Rust dependencies cached across sessions
- **First run**: May take longer to populate caches
- **Subsequent runs**: Significantly faster due to cached packages

## Security Notes

- API keys are never stored in the container image
- Environment variables are only available during runtime
- Restart the container if you update environment variables

## Health Check Entrypoint (Contract)

- Purpose: Provide a standard, scriptable way for `ah repo check` (host side) to verify that a project's devcontainer is healthy and ready.

- Entrypoint locations (first match wins):
  - `.devcontainer/ah-healthcheck` (preferred)
  - `.devcontainer/healthcheck.sh`

- Invocation (host side):
  - If Dev Containers CLI is available, run inside the container workspace:
    - `devcontainer exec --workspace-folder <repo-root> -- .devcontainer/ah-healthcheck --json`
  - Otherwise, projects may provide `just health` or `make health` as fallbacks (host‑side). `ah repo check` can try these when the script is absent.

- Invocation (inside container):
  - Executable must accept `--json` and default to human‑readable output when omitted.

- Exit codes:
  - 0: All checks passed
  - 1: One or more checks failed (still print per‑check details)
  - 2: Misconfiguration or prerequisite missing (e.g., devshell/toolchain not found)

- JSON output (when `--json`):

```json
{
  "passed": true,
  "checks": [
    { "name": "nix", "ok": true, "details": "nix 2.18.1" },
    { "name": "devshell", "ok": true, "details": "default shell available" },
    { "name": "task-runner", "ok": true, "details": "just 1.26.0" },
    { "name": "git", "ok": true, "details": "git 2.45.1" }
  ]
}
```

- Recommended checks (project may add more):
  - nix: `nix --version` and flakes enabled
  - devshell: evaluate/select default shell (e.g., `nix develop -c true`)
  - task‑runner: `just --version` or `make --version` depending on project config
  - git: `git --version`
  - disk: ensure workspace free space above a threshold (project‑configurable)
  - network (optional): probe known internal mirrors or document offline mode

- Notes:
  - The script must produce stable, parseable JSON when `--json` is passed and no other output on stdout. Human‑readable output should go to stdout when `--json` is not used; diagnostics may go to stderr in both modes.
