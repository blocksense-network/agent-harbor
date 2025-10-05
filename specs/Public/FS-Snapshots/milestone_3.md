**Milestone 3: Credential Management System**
_Implementation:_ Based on research of actual AI agent tools, implement a comprehensive credential management system:

- **Credential Pattern Detection:** Support for different agent types with their specific credential requirements:
  - **Codex:** `OPENAI_API_KEY` environment variable, `~/.codex/auth.json` and config files
  - **GitHub Copilot:** `GITHUB_TOKEN` environment variable, `~/.config/gh/hosts.yml` for GitHub CLI auth
  - **Goose:** `OPENAI_API_KEY`, `ANTHROPIC_API_KEY` environment variables
  - **Gemini:** `GEMINI_API_KEY` environment variable
  - **Claude:** `ANTHROPIC_API_KEY` environment variable
- **Secure Mounting:** For containerized execution, mount credential files read-only and propagate environment variables securely. For VM execution, sync credential files safely while preserving file permissions (0600 for auth files).
- **Colima/VM Support:** Research confirms that Colima and similar VM solutions support both environment variable propagation and bind mounts for credential files. The system will use Docker's `--env-file` and `--mount` options for secure credential injection.

_Testing:_ Test credential mounting with dummy credentials in controlled environments. Verify that credentials are accessible to agents. Test both file-based and environment variable credentials.
