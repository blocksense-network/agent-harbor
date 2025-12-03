# How to daemonize a process (cross-platform, user-scoped)

Goal: run the ACP access point as a user-level daemon that survives the launching `ah acp` process exit while retaining the caller's privileges.

## Linux / POSIX (preferred: systemd --user)

- Preferred: create/manage a **systemd --user** unit. Benefits: lifecycle mgmt, logging, restart, no root needed.
- Fallback: classic daemonization: double-fork → `setsid()` → `chdir("/")` → close/reopen stdio to `/dev/null` or log files; avoid acquiring a controlling TTY.
- Always drop inherited pipes/pty FDs so the child is not tied to the parent's lifetime.

## macOS

- Run as a per-user **LaunchAgent** in `~/Library/LaunchAgents`. Use `KeepAlive` for persistence and `AbandonProcessGroup` if helpers should outlive the launcher.
- If LaunchAgents are unavailable, fallback to the POSIX double-fork pattern.

## Windows

- Preferred: register a **Task Scheduler** task in the user's context with “run whether user is logged on or not.”
- Fallback: `CreateProcess` with `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`, redirect stdio to `NUL`, set working dir explicitly, and avoid parent console handles.

## Implementation notes for `ah acp --daemonize=auto`

- Detect existing access point (UDS/port/WS). If absent:
  - First prefer the installer-provisioned user/system service (e.g., systemd unit, LaunchAgent, Scheduled Task) if present but stopped—start it.
  - If no managed service exists or cannot be started, fall back to ad-hoc launch:
    - Linux: systemd --user unit if available; otherwise double-fork.
    - macOS: LaunchAgent if available; otherwise double-fork.
    - Windows: Scheduled Task if available; otherwise detached CreateProcess.
- Idle timeout (default 24h) enforced inside the daemon so it can self-terminate when unused.
- Keep execution under the invoking user; do not elevate.
- Ensure stdio is detached/redirected to avoid termination when the parent exits.

## Rationale

Using the OS-native user-level service managers (systemd --user, LaunchAgents, Task Scheduler) provides reliable restarts, logging, and isolation while keeping privileges aligned with the invoking user. POSIX double-fork or Windows detached process creation are acceptable fallbacks when service managers are unavailable.
