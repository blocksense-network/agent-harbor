## Testing Strategy — Codex Browser Automation

Goal: validate Playwright-driven automation that navigates `https://chatgpt.com/codex`, selects a workspace/branch, enters "go", and starts coding — while honoring Agent Browser Profiles visibility and login expectations.

### Levels of Testing

1. Unit-like checks (fast):

- Validate profile path resolution across platforms given environment overrides.
- Validate parsing and semantics of `meta.json` (visibility policy, login expectations, TTL/grace).
- Validate selector maps/config fallbacks without launching a browser.

2. Playwright integration tests (headless/headful):

- Use persistent contexts tied to ephemeral copies of real profiles (or synthetic profiles) to avoid mutating a user’s primary profiles.
- Mock or guard network calls as needed, but prefer real navigation to detect UI drift.

3. OS‑level visibility assertions:

- Verify that the browser starts hidden (headless) when login is known good.
- Verify that the browser is displayed (headful) only when login is unknown/expired/failing or when UI drift is detected.

### Headless vs Headful Verification

- Headless: Assert Playwright is launched with `headless: true` and no windows are created at the OS level. For Linux/macOS/Windows, implement a platform helper that samples top‑level windows via:
  - macOS: `CGWindowListCopyWindowInfo` via a small helper binary or `osascript -e 'tell app "System Events" to ...'` as fallback.
  - Linux: `xprop`/`wmctrl` on X11; `gdbus`/`gsettings`/`swaymsg` on Wayland (best‑effort; may skip when unavailable).
  - Windows: Win32 `EnumWindows` via a helper, or `powershell` COM query fallback.
- Headful required: Assert at least one top‑level browser window becomes visible within a timeout after a failed login probe or drift detection.

These helpers should be wrapped with feature detection and skipped when the environment cannot reliably report window state (e.g., headless CI without a virtual display).

### CI Considerations

- Use containerized jobs with a virtual display (Xvfb or Xwayland) and a minimal window manager to support headful tests.
- For macOS runners, prefer native headless for most tests; restrict window‑visibility tests to self‑hosted runners capable of GUI automation.
- For Windows, run in a session with desktop interaction enabled.

### Login Expectation Scenarios

Test cases should cover:

- Known good login: `lastValidated` fresh and check passes → remain headless.
- Stale login: `lastValidated` older than `graceSeconds` → perform probe; if probe fails, switch to headful and wait for user.
- No expectations configured: proceed headless by default; do not block.
- Cookie present but selector absent: treat as not logged in (conservative), switch to headful.

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

- If navigation returns a rate‑limit or captcha page, switch to headful, surface instructions, and pause. Tests simulate this by stubbing responses to return challenge pages and assert the fallback behavior.

### Telemetry and Artifacts

- Save Playwright traces, console logs, and screenshots on failure.
- Update `lastValidated` on successful login checks; avoid writes in tests unless operating on disposable profile copies.

### Fully Automated Local and CI Execution

- Provide a test harness that:
  - Creates a temporary profile directory seeded with synthetic cookies/selectors to emulate login.
  - Runs headless success path and asserts no windows.
  - Runs stale/failed login paths and asserts window visibility transitioned as expected.
  - Runs UI drift scenarios using selector overrides.
  - Cleans up all temporary artifacts.

### Developer Ergonomics

- `--update-selectors` test mode to record new stable selectors when UI drift is acknowledged by a developer.
- `--show-browser` override to force headful during local debugging.
