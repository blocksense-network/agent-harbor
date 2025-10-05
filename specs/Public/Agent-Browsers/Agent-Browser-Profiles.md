## Agent Browser Profiles — Minimal Convention (v0)

### Scope

Defines a shared, cross‑platform convention for storing named browser profiles used by automated agents that require authenticated access to particular websites. A profile represents a persistent browser user data directory plus lightweight metadata that describes login expectations and provenance. This spec’s primary purpose is to make such profiles discoverable by applications while allowing users to transparently know which profile and authentication will be used by the application. The same profile name can be referenced by multiple applications. A default profile is used when none is specified.

#### Motivation

Multiple agentic applications (e.g., a research assistant, an issue triager, and an expense reporter) need to act on behalf of the user across several websites (e.g., `chatgpt.com`, `jira.example.com`, `expense.example.com`). Instead of each app asking the user to log in separately, they discover existing agent browser profiles by matching the sites/username metadata that each profile provides. Typically these apps run headless using a browser automation framework such as Playwright. When an expected login is not actually accomplished, the app restarts the automation engine in a visible state so the user can complete the login, then resumes and finishes the task.

If the app discovers multiple candidate profiles for the same website (for example, different `username` values), our guidance is to ask the user which profile to use for the current task. Applications should communicate profile names clearly and expose options to create new profiles or rename existing ones. Users are expected to become familiar with these profile names, which are reused across applications.

### Base Directory (per‑user)

Profiles live under a single per‑user base directory. It can be overridden by an environment variable; otherwise, standard OS conventions are used.

- Linux: `$XDG_DATA_HOME/agent-browser-profiles` or `$HOME/.local/share/agent-browser-profiles` when `XDG_DATA_HOME` is unset
- macOS: `$HOME/Library/Application Support/agent-browser-profiles`
- Windows: `%APPDATA%\agent-browser-profiles`

Override (highest precedence): `AGENT_BROWSER_PROFILES_DIR`

### Profile Naming and Resolution

- Profile names must be lowercase slugs: `[a-z0-9][a-z0-9-_]{0,62}` (no spaces).
- Reserved name: `default` — used when the caller does not specify a profile.
- Applications should fail fast on invalid names and print the resolved absolute path.

### Directory Layout

```
<AGENT_BROWSER_PROFILES_DIR>/
  <profile-name>/
    metadata.json        # Required metadata (schema v1)
    browsers/            # Playwright persistent context userDataDir
      chromium/
      firefox/
      safari/
      webkit/
    notes.md             # Optional, user-maintained
```

Notes:

- Only `metadata.json` is required by this spec. Subdirectories under `browsers/` are optional and created on demand.
- Data in these directories may contain secrets (cookies, tokens). Store them in user scope; do not commit to VCS.

### Metadata File: `metadata.json` (Schema v1)

Format: JSON, UTF‑8. Unknown fields must be ignored for forward compatibility.

```json
{
  "schemaVersion": 1,
  "profileName": "default",
  "description": "Primary automation profile",
  "createdAt": "2025-01-01T12:00:00Z",
  "updatedAt": "2025-01-01T12:00:00Z",
  "createdBy": ["my-app", "v1.2.3"],
  "loginExpectations": [
    {
      "origins": ["https://chatgpt.com"],
      "username": "alice@example.com"
    }
  ]
}
```

Field definitions:

- `schemaVersion` (number): Always `1` for this spec.
- `profileName` (string): Redundant safety for human inspection. Not authoritative for path resolution.
- `description` (string, optional).
- `createdAt` / `updatedAt` (RFC3339 strings): For auditing.
- `createdBy` (array<string>): Application and version that created this profile, e.g., `["app-name", "v1.2.3"]`.
- `loginExpectations` (array): Zero or more per‑site discovery hints. Each entry:
  - `origins` (array<string>): Allowed origins for the site (schemes required).
  - `username` (string): Account identifier expected to be logged in (email, handle, or user ID).
    Applications MAY include additional, application‑specific keys inside `loginExpectations` entries to support their own check mechanisms; such keys are not standardized by this spec.

### Environment Variables

- `AGENT_BROWSER_PROFILES_DIR`: Absolute path override for the base directory.
- `AGENT_BROWSER_PROFILE`: Default profile name to use when the application does not specify one.

### Security and Portability Notes

- Profile contents may include cookies and tokens protected by OS keychains. Profiles generally do not port across different machines/OSes. Treat them as per‑user, per‑machine.
- Never commit profile directories to source control.
