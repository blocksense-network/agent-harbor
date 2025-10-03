## Browser Automation

Each document in this folder describes an automation targeting a specific site that agents‑workflow interacts with. Automations share the Agent Browser Profiles convention in [../Agent Browsers/Agent-Browser-Profiles.md](../Agent Browsers/Agent-Browser-Profiles.md) for persistent, named profiles.

Browser automation serves as the foundation for cloud agent support, enabling seamless integration between local CLI workflows and cloud-based AI platforms. All cloud agents currently require browser automation for authentication, task submission, and progress monitoring.

### Structure

- `<site>.md` — High‑level behavior of the automation (e.g., `Codex.md`).
- `<site>-Testing.md` — Testing strategy and edge cases for the automation.

### Common Principles

- Use Playwright persistent contexts bound to a selected profile.
- Prefer headless execution when the profile's login expectations are met; otherwise, switch to headful and guide the user.
- Detect UI drift and fail fast with actionable diagnostics. When possible, surface the browser window to help the user investigate.
- Integrate with `ah agent record` for session recording and `ah agent follow-cloud-task` for real-time monitoring.
- Support completion notifications with custom `agent-harbor://` links for seamless WebUI integration.
- Enable dual monitoring: browser automation can run alongside TUI interfaces for comprehensive progress tracking.
