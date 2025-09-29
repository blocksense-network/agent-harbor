## WebUI — Product Requirements and UI Specification

### Product Summary

The WebUI provides a browser-based experience for creating, monitoring, and managing agent coding sessions backed by the Agents-Workflow REST Service. It targets:

- Engineering teams running on-prem/private cloud clusters.
- Individual developers preferring a graphical dashboard over CLI.

### Goals

- Zero-friction task creation with sensible defaults and policy-aware templates.
- Real-time visibility into active sessions: status, logs, and artifacts.
- One-click launch into preferred IDEs (VS Code, Cursor, Windsurf) pointing at the per-task workspace.
- Governance: tenancy, RBAC, audit trail.

### Non-Goals

- Full web IDE. The WebUI integrates with external IDEs.
- Replacing VCS flows. It assists delivery (PR/branch/patch) but does not host repos.

### User Roles

- Admin: Manage tenants, executors, policies.
- Operator: Create/monitor sessions, manage queues, pause/resume.
- Viewer: Read-only access to sessions and logs.

### Key Use Cases

1. Create a new task with repo, runtime, and agent settings.
2. Watch live logs and events, inspect workspace details.
3. Stop/pause/resume a running session.
4. Launch IDE connected to the workspace.
5. Browse history, filter by status/agent/project, and inspect outcomes (PR/branch/patch).
6. Receive non-intrusive error notifications for failed operations without workflow interruption.
7. Get visual feedback on draft auto-save status for confidence in work persistence.

### Simplified Task-Centric Layout

The WebUI follows a simple, task-focused design with two main areas:

- **Header**: Agent Harbor logo, title "Agent Harbor" (without sub-title) and navigation with Settings link.
- **Tasks**: Chronological list of recent tasks (completed, active, merged) displayed as bordered cards, with draft tasks always visible at the top, sorted newest first.

#### Task States

Tasks display in five different states with optimized heights:

- **Merged**: Compact 2-line card (title + metadata)
- **Completed**: Compact 2-line card (title + metadata) - no SSE activity area needed
- **Active**: Full 5-line card (title + metadata + 3 fixed-height activity rows)
- **Draft**: Variable height card with text area and controls (keyboard navigable, Enter to submit)

#### Task Feed Cards

Each task displays as a bordered card with status-appropriate styling:

- **Fixed height - NEVER changes**: Cards maintain constant height regardless of content
- **Compact layout**: All metadata (repo, branch, agent, timestamp) fits on single lines
- **Status indicators**: Color-coded icons (✓ completed, ● active, 📝 draft, 🔀 merged)
- **Visual separators** between cards
- **Quick actions**: Stop, Pause/Resume, IDE Launch (contextual based on state)
- **Keyboard navigation**: Arrow keys (↑↓) navigate between ALL cards (draft tasks first, then sessions newest first) with visual selection state
  - **Keyboard-only selection**: Cards display blue border and blue background **ONLY when selected via keyboard navigation**
  - **Click behavior**: Clicking anywhere on a card does NOT select it - click the task title to navigate to details
  - **Title click navigation**: Task title is a clickable link that navigates to the task details page
  - **Draft card keyboard behavior**: When a draft card is keyboard-selected:
    - The task textarea automatically receives focus
    - User can immediately start typing
    - Pressing Enter submits the task (equivalent to clicking "Go" button)
    - Arrow keys still navigate between cards (when textarea is not focused)
- **Focus management**: Selecting a different card removes focus from any previously focused textarea
  - **Textarea blur on navigation**: When navigating away from a draft card via keyboard, the textarea loses focus immediately
  - **Viewport scrolling**: When keyboard navigation selects a card outside the current viewport, the UI automatically scrolls to make the card visible
- **Dynamic shortcuts**: Footer shortcuts change based on current focus:
  - **Draft textarea focused**: "Enter" displays as "Launch Agent(s)" (plural if multiple agents selected)
  - **Session card selected**: "Enter" displays as "Review Session Details"
  - **No selection**: Default shortcuts displayed
- **Enter to open**: Pressing Enter while a session card is selected navigates to the task details page

**Completed/Merged Card Layout (Compact - 2 lines):**

Line 1: Status icon • Task title (clickable) • Action buttons
Line 2: Repository • Branch • Agent • Timestamp (all on one compact line)

**Active Task Card Layout (Full - 5 lines):**

Line 1: Status icon • Task title (clickable) • Action buttons
Line 2: Repository • Branch • Agent • Timestamp (all on one compact line)
Lines 3-5: **ALWAYS 3 fixed-height activity rows** (never empty, never more, never less)

**Activity Row Requirements (Active Tasks Only):**
- **Pre-populated from SSR**: Server fetches last 3 events for each active session during page generation
- **Never shows "Waiting for agent activity"**: Always displays the 3 most recent events
- **Fixed height rows**: Each of the 3 rows has fixed height (prevents UI "dancing")
- **SSE updates scroll existing rows**: New events scroll up, oldest disappears, newest appears at bottom
- **Height never changes**: Card height remains constant as events scroll

Active tasks show live streaming of agent activity (3 fixed-height lines):
- Thoughts: Single line with description
- File edits: Single line with filename and diff stats  
- Tool usage: Single line showing tool name and status, or two lines (tool + indented last_line output)
- **Live updates via SSE**: Active session cards continuously update by scrolling events upward as new events arrive

**Draft Card Layout (Variable height):**

- Text area for task description (expandable)
- Single row of compact controls (repository, branch, model selectors + Go button)
- Keyboard navigable: Arrow keys select, automatic focus on textarea
- Enter key submits task when textarea is focused

#### New Task Card

An empty task card is always visible at the top of the task feed for immediate task creation:

- **Always-visible text area** for task description with markdown support
- **Single line of compact controls** below the text area:
  - Left side: Repository Selector, Branch Selector, Model Selector (all compact, horizontally laid out)
  - Right side: "Go" button (right-aligned)
  - All controls fit on one row for a clean, horizontal layout
- **TOM Select Integration** ([TOM Select](https://tom-select.js.org/) library):
  - Repository Selector: Popup combo-box with text input for fuzzy matching
  - Branch Selector: Popup combo-box with text input for fuzzy matching  
  - Model Selector: Multi-select combo with per-model instance counters:
    - **Dropdown behavior**: Plus/minus buttons visible on ALL rows (including hovered rows) for adjusting instance counts
    - **Selected badge behavior**: Selected models appear as badges in the input field with overlay plus/minus buttons for in-place editing
    - Users can click +/- on badges to adjust instance counts or remove models (when count reaches zero)
  - All TOM Select controls are compact (smaller text, reduced padding) to fit horizontally
- **TAB navigation** between controls
- **Multiple draft tasks** supported - users can have several draft tasks in progress, new ones inserted at the top
- **Auto-save drafts** to server and restore across sessions and devices (debounced, 500ms delay)
- **Default values** from last used selections
- **Context-sensitive keyboard shortcuts**:
  - While focus is inside the new task text area, footer shows: "Enter Launch Agent(s) • Shift+Enter New Line"
  - "Agent(s)" is plural if multiple agents are selected
  - Enter key launches the task (calls Go button action)
  - Shift+Enter creates a new line in the text area

#### Task Details Page

Clicking on any task card navigates to a task-specific details page (route-linked), preserving browser history for back/forward navigation:

- **Header**: Task ID, status, repository, agent, timestamps, duration
- **Tabs**:
  - Overview: Prompt, delivery mode, repository info, runtime summary
  - Live Log: Real-time stream with controls (tail, filter, copy/download, auto-scroll)
  - Events: Timeline of task events (provisioned, tests passed, PR opened)
  - Report: Final summary, diff/patch with download links and PR integration
  - Workspace: Mount paths, snapshot info, IDE launch helpers
- **Actions**: Stop, Pause/Resume, Cancel, Open IDE

#### Global Controls

- **Search/Filter bar**: Global search across tasks with filters for status, agent, repository
- **Settings panel**: Theme selection (light/dark), IDE preferences, repository management
- **Admin panel**: For enterprise mode - manage tenants, agents, runtimes, hosts (role-gated)

#### Repository Management

Repository selection integrated into the New Task popup combo-box:
- **Enterprise mode**: Curated list from workspace configuration
- **Local mode**: Auto-populated from previously used repositories with add/remove controls
- **Branch suggestions**: Live autocomplete from git repository
- **Validation**: Repository reachability and branch existence checks

Filters and branch suggestion endpoints (restored technical details):
- **Filters**: status (queued/provisioning/running/paused/completed/failed), agent type, projectId/repo, label key/values, date range; bulk actions (Stop, Cancel; role-gated).
- **Branch suggestions**:
  - Local mode: Use `git for-each-ref` against the local repo, cached in-memory per repo with debounce refresh.
  - Server mode: Query `/api/v1/repos/{id}/branches?query=<prefix>&limit=<n>` backed by the server's in-memory branch cache populated via standard git protocol.

### Visual Design & Theming

#### Modern Web Aesthetics

The WebUI follows a clean, modern design with light/dark theming only:

- **Light Theme**: Subtle shadows and modern card design
- **Dark Theme**: Catppuccin Mocha-inspired colors
- **Rounded borders**: Modern card design with generous padding and breathing room
- **Subtle shadows**: Layered depth without overwhelming the interface
- **Responsive design**: Adapts gracefully from desktop to mobile

#### Component Styling

- **Cards**: Clean rounded borders, subtle shadows, proper padding
- **Buttons**: Modern button styles with hover/focus states
- **Popups**: Lightweight combo-box popups with backdrop and smooth animations
- **Status indicators**: Color-coded badges and icons matching task states
- **Form controls**: Clean inputs with validation states and helper text

#### Task Card Design

- **Compact layout**: Information-dense but not cramped
- **Progressive disclosure**: Expandable sections for additional details
- **Live indicators**: Animated elements for active tasks
- **Action buttons**: Contextual actions based on task state

#### Keyboard Shortcuts Footer

Single-line footer (like Lazygit) showing keyboard shortcuts as hints:
- Left side: Context-sensitive keyboard shortcut hints that change dynamically based on application state:
  - **Task feed focused**: "↑↓ Navigate • Enter Select Task"
  - **New task text area focused**: "Enter Launch Agent(s) • Shift+Enter New Line • Tab Next Field"
  - **Modal/dialog open**: "Esc Cancel • Tab Next • Shift+Tab Previous"
- Right side: Clickable "New Task" button styled prominently with integrated keyboard shortcut display (Ctrl+N on Windows/Linux, Cmd+N on macOS)
  - Button has hover cursor (pointer) to indicate it's clickable
  - Keyboard shortcut Ctrl+N / Cmd+N works globally to create new drafts
  - "New Task" appears ONLY once (in the button), not duplicated in the left-side shortcuts
- Footer shortcuts must be modeled as part of application state. The logic that changes this state must be testable in vitest unit tests
- Shortcuts can change even within a single page based on focus state, modal dialogs, or component interactions
- "Agent(s)" in "Launch Agent(s)" adjusts to singular/plural based on number of selected agents

### Real-Time Behavior

#### SSE Event Stream

- Use SSE to subscribe to `/api/v1/sessions/{id}/events` for status/log updates.
- Reconnect with exponential backoff; buffer events during network blips.
- Server sends **one event at a time** to allow smooth UI updates

#### Task Card Live Activity Display

Active task cards display the **most recent 3 activity rows** in a fixed-height section:

**Row Display Rules:**
- **Fixed height rows**: Each row has a fixed height to prevent UI "dancing" as content updates
- **Scrolling effect**: New events cause rows to scroll upward visually (newest at bottom)
- **Always 3 rows visible**: Shows the 3 most recent activity items at all times

**Event Types:**

1. **Thinking Event** (`thought` property):
   - Format: `"Thoughts: {thought text}"`
   - Behavior: Scrolls existing rows up, appears as new bottom row
   - Example: `{sessionId: "...", thought: "Analyzing the codebase structure", ts: "..."}`

2. **Tool Use Start** (`tool_name` property):
   - Format: `"Tool usage: {tool_name}"`
   - Behavior: Scrolls existing rows up, appears as new bottom row
   - Example: `{sessionId: "...", tool_name: "search_codebase", tool_args: {...}, ts: "..."}`

3. **Tool Last Line** (`tool_name` + `last_line` properties):
   - Format: `"  {last_line}"` (indented, showing command output)
   - **Special behavior**: Updates the existing tool row IN PLACE without scrolling
   - Does NOT create a new row - modifies the current tool execution row
   - Example: `{sessionId: "...", tool_name: "search_codebase", last_line: "Found 42 matches in 12 files", ts: "..."}`

4. **Tool Complete** (`tool_name` + `tool_output` + `tool_status` properties):
   - Format: `"Tool usage: {tool_name}: {tool_output}"` (single line with status indicator)
   - Behavior: Sent immediately after last `last_line` event
   - The last_line row is removed and replaced by this completion row
   - May scroll up if followed by a new event
   - Example: `{sessionId: "...", tool_name: "search_codebase", tool_output: "Found 3 matches", tool_status: "success", ts: "..."}`

5. **File Edit Event** (`file_path` property):
   - Format: `"File edits: {file_path} (+{lines_added} -{lines_removed})"`
   - Behavior: Scrolls existing rows up, appears as new bottom row
   - Example: `{sessionId: "...", file_path: "src/main.rs", lines_added: 5, lines_removed: 2, ts: "..."}`

6. **Status Update** (`status` property):
   - Updates card header status (not shown in activity rows)
   - Example: `{sessionId: "...", status: "running", ts: "..."}`

**Visual Behavior Example:**

```
Initial state (empty):
  [Waiting for agent activity...]

After "thought" event:
  Thoughts: Analyzing codebase structure

After "tool_name" event (scrolls up):
  Thoughts: Analyzing codebase structure
  Tool usage: search_codebase

After "last_line" event (updates in place - NO scroll):
  Thoughts: Analyzing codebase structure
  Tool usage: search_codebase
    Found 42 matches in 12 files

After "tool_output" event (replaces last_line row):
  Thoughts: Analyzing codebase structure
  Tool usage: search_codebase: Found 3 matches

After new "thought" event (scrolls up, oldest row disappears):
  Tool usage: search_codebase: Found 3 matches
  Thoughts: Now examining the authentication flow
```

**Implementation Requirements:**
- Maximum 3 rows displayed at all times
- Fixed row height (no dynamic height based on content)
- Smooth scroll-up animation when new events arrive (except last_line)
- Text truncation with ellipsis if content exceeds row width
- Visual distinction between different event types (icons, indentation)

#### Mock Server Simulation

- Development mock server generates realistic SSE streams for active sessions (minimum 2 active sessions with continuous event streams)
- Events sent one at a time with appropriate delays to simulate real agent activity
- Mix of thinking, tool execution (with last_line updates), and file edit events

### IDE Launch Integration

- Call `POST /api/v1/sessions/{id}/open/ide` and display returned commands.
- Provide copy-to-clipboard and "Try locally" hints.

TODO: This has to be re-thought. How does it work in [Local-Mode](Local-Mode.md)? How does it work with a [Remote Server](Remote-Mode.md)? VS Code and Cursor have remote mode, accessible over the web, but we need to create a secure tunnel for this.

### Empty States and Errors

- Helpful guidance for no sessions, no hosts, or missing permissions.
- Problem+JSON errors rendered with field-level highlights.

### Accessibility and i18n

- WCAG AA color contrast; keyboard navigation; ARIA landmarks.
- Strings externalized for localization; LTR/RTL aware layouts.

### Telemetry and Audit

- Client events (navigation/actions) batched and sent to server metrics endpoint.
- Audit trail ties UI actions to user identity and session ids.

### User Experience Enhancements

#### Toast Notifications for Errors

- **Global Error Handling**: Non-critical API errors (e.g., failed session operations) display as temporary toast notifications instead of console logs or modal dialogs
- **Non-Intrusive Design**: Toasts appear in top-right corner, auto-dismiss after 5 seconds, don't interrupt user workflow
- **Contextual Messages**: Error messages are user-friendly and actionable (e.g., "Failed to stop session, please try again")
- **Accessibility**: Toasts are announced to screen readers via ARIA live regions
- **Consistent Styling**: Error toasts use red background, success toasts use green, info toasts use blue

#### Optimistic UI for Draft Auto-Save

- **Visual Save Indicators**: Draft textareas show real-time save status in the lower right corner
- **Status States**: "Unsaved" (gray), "Saving..." (orange), "Saved" (green), "Error" (red)
- **Request Tracking**: Each save attempt is assigned a unique request ID to track validity
- **Request Invalidation**: When user types while a save request is pending, that request becomes "invalidated"
- **Status Algorithm**:
  - **Unsaved**: User has typed but no save request is in flight OR current in-flight request is invalidated
  - **Saving...**: There is a valid (non-invalidated) save request currently in flight
  - **Saved**: No pending changes AND most recent save request completed successfully
  - **Error**: Most recent save request failed and no new typing has occurred
- **Save Timing**: Save requests are sent only after 500ms of continuous inactivity
- **Concurrent Typing Protection**: Ongoing typing invalidates previous save requests, preventing text truncation
- **Server Response Handling**: Save confirmations for invalidated requests are ignored if newer changes exist
- **Integrated Positioning**: Indicators overlay the textarea corner without affecting layout

### Performance Targets

- TTI < 2s on 3G Fast; live log latency < 300ms; lists virtualized beyond 200 rows.

### Tech Notes (non-binding)

- SPA built with SolidJS + SolidStart + TypeScript + Tailwind CSS, SSE for events, OpenAPI client for REST.
- **TOM Select Integration**: Use [TOM Select](https://tom-select.js.org/) JavaScript library for repository, branch, and model selector widgets with fuzzy search
- **Proxy-Based Architecture**: The SSR server acts as the single entry point for all requests (HTML, CSS, JS, and API calls). The SSR server proxies all `/api/v1/*` requests to the API server (access point daemon). This architecture enables the SSR server to implement user access policies and security controls before forwarding requests to the underlying API.
- **API Server Integration**: The API server (access point daemon, same code path as `aw agent access-point`) runs either as a subprocess or sidecar process, communicating with the SSR server via HTTP or stdin/stdout for subprocess mode. The `aw webui` command starts both the SSR server and the access point daemon in-process.

#### Progressive Enhancement and Server-Side Rendering

**CRITICAL REQUIREMENT: The WebUI MUST function completely without JavaScript enabled.**

- **Server-Side Rendering (SSR)**: The SSR server fetches ALL data from the API server during initial page generation and renders a fully-populated HTML page. This includes:
  - Complete session list (all task cards with full details)
  - All draft tasks
  - Agent lists, runtime options, and all configuration data
  - **Zero reliance on client-side hydration for content** - the HTML must be complete and functional before any JavaScript executes

- **Progressive Enhancement Philosophy**:
  - **Base experience (no JavaScript)**: Users can view all sessions, drafts, and task details. All content is visible in the initial HTML.
  - **Enhanced experience (JavaScript enabled)**: SolidJS hydration adds interactivity:
    - SPA-like navigation with instant page transitions (fetch JSON, update DOM)
    - Live SSE streaming for active session updates
    - TOM Select fuzzy-search widgets for repository/branch/model selection
    - Keyboard shortcuts and arrow key navigation
    - Optimistic UI updates for pause/stop/resume actions
    - Auto-save for draft tasks
  - **JavaScript is for UX enhancement, NOT for content rendering**

- **Implementation Requirements**:
  - Components receive initial data as props from SSR route loaders (using `createAsync` in route definitions)
  - Components render synchronously from props during SSR - no async data fetching in component lifecycle
  - Client-side `createResource` is used ONLY for post-hydration interactions (filtering, refreshing, polling)
  - SSR server uses the same proxy mechanism for data fetching as the client uses for API calls
  - Test suite MUST verify that SSR HTML contains all expected content before any JavaScript executes

- State normalized by session id; optimistic UI for pause/stop/resume.
- **Development Data**: Mock server returns 5 sessions (3 completed, 2 active) with continuous SSE event streams for active sessions

### Implementation Plan

Planning and status tracking for this WebUI implementation live in [WebUI.status.md](WebUI.status.md). That document defines milestones, success criteria, and a precise, automated test plan per specs/AGENTS.md.

### Local Mode (--local)

- Purpose: Provide a zero-setup, single-developer experience. The WebUI binds to `127.0.0.1` only and automatically starts a local access point daemon.
- Invocation: `aw webui [--port <port>]`. In this mode:
  - Network binding: SSR server listens on localhost only (e.g., `http://127.0.0.1:3002`).
  - Access point daemon: Started in-process (same code path as `aw agent access-point`) and bound to a separate port (e.g., `http://127.0.0.1:3001`). The SSR server proxies all `/api/v1/*` requests to this daemon.
  - Auth and tenancy: No RBAC/tenants; implicit single user. Admin panels are hidden (Agents/Runtimes/Hosts/Settings for multi-tenant ops).
  - Config discovery: When `--remote-server` is provided, the SSR server proxies to that external access point instead of starting a local daemon.
  - Intention: By default, `aw webui` provides a fully self-contained experience. The `--remote-server` option enables connecting to a shared access point for team workflows.
  - IDE integration: Unchanged; IDE launch helpers assume local filesystem access to the workspace mount.
  - Persistence: Uses browser local storage for UI preferences. No external DB required.
  - Security: No TLS in local mode; not intended for remote access.
- Service reachability:
  - If the local REST service is unreachable, show a blocking banner with retry and guidance (e.g., “Start the service, then retry”).
  - Optionally offer a copyable command to start the local service.
- Feature differences vs full mode:
  - Hidden panels: Agents, Runtimes, Hosts, multi-tenant Settings.
  - Task feed, Create Task, and basic Settings remain.
  - Delivery flows (PR/branch/patch) are available; features gated by what the local service advertises via `/api/v1/*` capability endpoints.
