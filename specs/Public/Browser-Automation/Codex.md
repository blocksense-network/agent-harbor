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
6. Ask the Electron automation host to start a Codex session for the selected profile:
   - Electron sets `userData` to `<profile>/browsers/chromium`.
   - A hidden automation window is created with `show: false`.
   - Playwright attaches to this window and drives navigation and interactions.
7. If the expected login is not present:
   - The same automation window is made visible (attached to a dedicated Browser Automation panel in the Electron UI).
   - The user completes the login flow.
   - Once login expectations pass, control returns to automation and the window can be hidden again if no further user interaction is needed.
8. Navigate to Codex, select workspace and branch, enter the task description, and press "Code":
   - Workspace comes from `--codex-workspace` or `config: codex-workspace` (see [Configuration.md](../Configuration.md)).
   - Branch comes from the `ah task --branch` value.
9. For cloud agents: integrate with `ah agent record` for session monitoring and `ah agent follow-cloud-task` for real-time progress tracking.
10. Record success and trigger completion notifications if enabled.

If the automation code fails to execute due to potential changes in the Codex WebUI. Report detailed diagnostic information for the user (e.g. which UI element you were trying to locate; Which selectors were used and what happened - the expected element was not found, more than one element was found, etc).

### Visibility and Login Flow

- Runs in a hidden Electron automation window by default:
  - When login is known good, the window remains hidden and automation proceeds without user intervention.
  - When login is unknown/expired or a login probe fails, the same window is shown in the Electron UI so the user can authenticate.
  - After successful login, the window may be hidden again while automation continues in the same session (no browser restart).

### Configuration

Controlled via AH configuration (see `docs/cli-spec.md` and `docs/configuration.md`):

- Enable/disable automation for `ah task`.
- Select or override the agent browser profile name.
- Set default Codex workspace: `codex-workspace`.

### Notes

- Playwright selectors should prefer role/aria/test id attributes to resist UI text changes.
- Use stable navigation points inside Codex (workspace and branch selectors) and fail fast with helpful error messages when not found; optionally open DevTools in headful mode for investigation.
