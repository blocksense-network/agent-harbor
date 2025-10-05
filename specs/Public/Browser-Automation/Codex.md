## Codex Browser Automation (Playwright)

### Purpose

Automate the Codex WebUI to initiate coding sessions for both local and cloud agent workflows using shared agent browser profiles. This automation supports the `cloud-codex` agent type and serves as the foundation for cloud agent browser automation integration.

### Behavior (happy path)

1. Determine ChatGPT username: accept optional `--chatgpt-username` (see CLI.md).
2. Discover profiles: list agent browser profiles whose `loginExpectations.origins` include `https://chatgpt.com`.
3. Filter by username: if `--chatgpt-username` is provided, restrict to profiles whose `loginExpectations.username` matches.
4. Select or create profile:
   - If one or more profiles match, choose the best candidate (prompt if multiple).
   - If none match, create a new profile named `chatgpt-<username>` when a username is provided, otherwise `chatgpt`.
5. Override behavior: if `--browser-profile` is provided, skip discovery/creation and use that profile name directly (create fresh if missing).
6. Launch Playwright with a persistent context in headless mode.
7. If the expected login is not present, relaunch in visible mode to let the user authenticate, then continue.
8. Navigate to Codex, select workspace and branch, enter the task description, and press "Code":
   - Workspace comes from `--codex-workspace` or `config: codex-workspace` (see [Configuration.md](../Configuration.md)).
   - Branch comes from the `ah task --branch` value.
9. For cloud agents: integrate with `ah agent record` for session monitoring and `ah agent follow-cloud-task` for real-time progress tracking.
10. Record success and trigger completion notifications if enabled.

If the automation code fails to execute due to potential changes in the Codex WebUI. Report detailed diagnostic information for the user (e.g. which UI element you were trying to locate; Which selectors were used and what happened - the expected element was not found, more than one element was found, etc).

### Visibility and Login Flow

- Runs headless by default; when login is not present, restarts headful to allow the user to log in, then proceeds automatically.

### Configuration

Controlled via AH configuration (see `docs/cli-spec.md` and `docs/configuration.md`):

- Enable/disable automation for `ah task`.
- Select or override the agent browser profile name.
- Set default Codex workspace: `codex-workspace`.

### Notes

- Playwright selectors should prefer role/aria/test id attributes to resist UI text changes.
- Use stable navigation points inside Codex (workspace and branch selectors) and fail fast with helpful error messages when not found; optionally open DevTools in headful mode for investigation.
