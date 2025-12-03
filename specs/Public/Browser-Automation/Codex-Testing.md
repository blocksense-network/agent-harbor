## Testing Strategy — Codex Browser Automation

Goal: validate Playwright-driven automation that navigates `https://chatgpt.com/codex`, selects a workspace/branch, enters "go", and starts coding — while honoring Agent Browser Profiles visibility and login expectations.

### Levels of Testing

1. Unit-like checks (fast):

- Validate profile path resolution across platforms given environment overrides.
- Validate parsing and semantics of `meta.json` (visibility policy, login expectations, TTL/grace).
- Validate selector maps/config fallbacks without launching a browser.

2. Playwright + Electron integration tests (hidden/visible automation window):

- Use persistent contexts tied to ephemeral copies of real profiles (or synthetic profiles) to avoid mutating a user’s primary profiles.
- Mock or guard network calls as needed, but prefer real navigation to detect UI drift.

3. OS‑level visibility assertions for Electron automation windows:

- Verify that the automation window stays hidden when login is known good.
- Verify that the automation window is shown only when login is unknown/expired/failing or when UI drift is detected.

### Hidden vs Visible Automation Window Verification

- Hidden automation:
  - Assert the Electron automation host process is running with a dedicated Codex session window created using `show: false`.
  - At the OS level, confirm that either no user-facing Electron windows exist for the automation session or only a minimized/non-visible window exists (platform-specific).
  - Use Electron IPC to assert the internal automation window is alive (e.g., `webContents.isDestroyed() === false`) and has navigated to the expected Codex URL.
- Visible required:
  - Under stale/failed login scenarios or simulated UI drift, assert that the automation window is promoted to a visible `BrowserWindow` or attached to a visible `<webview>` in the Electron UI and is enumerated as a top-level window where possible.
  - After the user completes login and automation resumes, assert the same window continues to be used (no session restart) and can be hidden again via IPC.

OS helpers (CGWindowListCopyWindowInfo, `wmctrl`, Win32 `EnumWindows`, etc.) should be used when available to corroborate Electron IPC state but skipped when the environment cannot reliably report window state.

### CI Considerations

- Use containerized jobs with a virtual display (Xvfb or Xwayland) and a minimal window manager to support headful tests.
- For macOS runners, prefer hidden-window automation for most tests; restrict window-visibility tests to self-hosted runners capable of GUI automation.
- For Windows, run in a session with desktop interaction enabled.

### Login Expectation Scenarios

Test cases should cover:

- Known good login: `lastValidated` fresh and check passes → keep the automation window hidden.
- Stale login: `lastValidated` older than `graceSeconds` → perform probe; if probe fails, show the automation window and wait for the user.
- No expectations configured: run hidden by default; do not block, but allow an override to show the window on demand.
- Cookie present but selector absent: treat as not logged in (conservative), show the automation window and pause automation.

### UI Drift and Resilience

Detection:

- Missing critical selectors (workspace picker, branch selector, "Code" button) must fail fast with a machine‑readable error.
- Automation should then show the browser (headful), optionally open DevTools, and present an inline banner/toast explaining what failed and how to proceed.

Tests:

- Simulate selector renames by injecting CSS/JS to remove/alter test ids via Playwright route interception or a local test proxy. Assert that:
  - The automation raises a drift error quickly.
  - The browser is brought to foreground (headful).
  - A diagnostic message is visible to the user and logs include selector keys that failed.

### Workspace/Branch Selection Edge Cases

- Multiple workspaces; selection requires scrolling or dynamic loading.
- Branch list too long; search/filter interaction required.
- Permissions errors (workspace not accessible) — assert graceful message and headful fallback.

### Rate Limits and Captcha Handling

- If navigation returns a rate‑limit or captcha page, show the automation window, surface instructions, and pause. Tests simulate this by stubbing responses to return challenge pages and assert the fallback behavior.

### Electron Host Smoke Tests

Goal: verify Electron automation wiring independent of provider logic.

- Test: "Can open a hidden Codex session"
  - Start `cloud-worker` with a disposable Agent Browser Profile.
  - Assert an Electron automation host process is spawned.
  - Assert via IPC that a hidden Codex window reached `https://chatgpt.com/codex`.
- Test: "Login promotion and return to hidden"
  - Seed the profile with a stale login expectation.
  - Assert the automation window is shown to the user.
  - After a test login flow, assert login expectations pass, the same window continues, and it can be hidden again.
- Test: "Profile isolation via userData"
  - Run two sessions in parallel with different profiles.
  - Assert cookies/storage from one profile are not visible in the other.

### Telemetry and Artifacts

- Save Playwright traces, console logs, and screenshots on failure.
- Update `lastValidated` on successful login checks; avoid writes in tests unless operating on disposable profile copies.

### Fully Automated Local and CI Execution

- Provide a test harness that:
  - Creates a temporary profile directory seeded with synthetic cookies/selectors to emulate login.
- Runs the hidden-window success path and asserts no visible windows.
  - Runs stale/failed login paths and asserts window visibility transitioned as expected.
  - Runs UI drift scenarios using selector overrides.
  - Cleans up all temporary artifacts.

### Developer Ergonomics

- `--update-selectors` test mode to record new stable selectors when UI drift is acknowledged by a developer.
- `--show-browser` override to force headful during local debugging.
