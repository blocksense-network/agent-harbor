# WebUI Test Strategy

## Overview

This document outlines the comprehensive testing strategy for the WebUI implementation, ensuring complete coverage of all requirements specified in [WebUI-PRD.md](WebUI-PRD.md).

## Testing Principles

1. **PRD-Driven Testing**: Every requirement in the PRD must have corresponding automated test coverage
2. **Progressive Enhancement Validation**: Tests verify both server-rendered HTML and client-side JavaScript functionality
3. **Real-World Simulation**: Mock server provides realistic data and SSE streams matching production behavior
4. **Keyboard-First Testing**: Verify keyboard navigation and shortcuts work as specified
5. **Integration Over Unit**: Focus on end-to-end user journeys rather than isolated component testing

## Test Categories

### 1. Infrastructure Tests (`build-tooling.spec.ts`)

Validates build tooling, compilation, and configuration:

- ✅ TypeScript compilation succeeds with strict mode
- ✅ ESLint configuration validates correctly
- ✅ Prettier formatting checks pass
- ✅ Production builds complete successfully
- ✅ Playwright configuration is valid

### 2. API Contract Tests (`api-contract.spec.ts`)

Validates mock server implements [REST-Service/API.md](REST-Service/API.md) spec correctly:

- ✅ GET /api/v1/sessions returns 5 sessions (3 completed, 2 active)
- ✅ GET /api/v1/sessions/:id returns correct session data
- ✅ POST /api/v1/tasks creates new sessions
- ✅ POST /api/v1/sessions/:id/stop transitions session to stopping
- ✅ DELETE /api/v1/sessions/:id removes session
- ✅ GET /api/v1/sessions/:id/events streams SSE events
- ✅ GET /api/v1/sessions/:id/logs returns session logs
- ✅ Validation errors return proper Problem+JSON format
- ✅ 404 errors for non-existent sessions

**Updated Requirements**:

- Mock server must return exactly 5 sessions (3 completed, 2 active)
- Active sessions must have continuous SSE event streams
- SSE streams must include thinking traces, tool executions, and file edits

### 3. SSR Rendering Tests (`example.spec.ts`)

Validates server-side rendering produces correct HTML:

- ✅ Server returns HTML with correct structure
- ✅ Agent Harbor branding renders in HTML
- ✅ Footer with keyboard shortcuts renders
- ✅ Task feed structure exists in SSR HTML
- ✅ Meta tags and accessibility attributes present
- ✅ Assets load with correct MIME types
- ✅ **NEW**: Server fetches data from API during SSR (fully-populated pages)
- ✅ **NEW**: SSR HTML contains session data from API

### 4. Layout and Navigation Tests (`layout-navigation.spec.ts`)

Validates simplified task-centric layout:

- ✅ Header with Agent Harbor logo and title
- ✅ Settings link in navigation
- ✅ Task feed displays chronologically
- ✅ Draft task cards always visible at bottom
- ✅ Footer with context-sensitive keyboard shortcuts
- ✅ Responsive design (desktop/mobile)
- ✅ **NEW**: Keyboard arrow navigation (↑↓) between task cards
- ✅ **NEW**: Visual selection state for selected task card
- ✅ **NEW**: Enter key navigates to task details page
- ✅ **NEW**: Task details page route works (placeholder implementation)

### 5. Keyboard Navigation Tests (`keyboard-navigation.spec.ts`) **NEW**

Validates keyboard-driven interface:

#### Arrow Key Navigation

- [ ] ↑ key moves selection up in task feed
- [ ] ↓ key moves selection down in task feed
- [ ] Visual indicator shows selected task (border/background highlight)
- [ ] Selection wraps at top/bottom of list
- [ ] Enter key on selected task navigates to task details page
- [ ] Esc key returns to task feed from details page

#### Context-Sensitive Shortcuts

- [ ] **Task feed focused**: Footer shows "↑↓ Navigate • Enter Select Task • Ctrl+N New Task"
- [ ] **New task text area focused**: Footer shows "Enter Launch Agent(s) • Shift+Enter New Line • Tab Next Field"
- [ ] **Modal open**: Footer shows "Esc Cancel • Tab Next • Shift+Tab Previous"
- [ ] Footer dynamically updates when focus changes
- [ ] "Agent(s)" adjusts to singular/plural based on selection

#### Draft Task Shortcuts

- [ ] Enter key in draft text area launches task (if valid)
- [ ] Shift+Enter creates new line in draft text area
- [ ] Tab key navigates between draft form fields
- [ ] Ctrl+N (Cmd+N on macOS) creates new draft task

#### Shortcut Footer Button

- [ ] "New Task" button visible in footer
- [ ] Button shows platform-specific shortcut (Ctrl+N or Cmd+N)
- [ ] Clicking button creates new draft task
- [ ] Keyboard shortcut executes same action

### 6. TOM Select Integration Tests (`tom-select-components.spec.ts`) **NEW**

Validates TOM Select widgets for repository, branch, and model selection:

#### Repository Selector

- [ ] TOM Select widget renders with placeholder "Repository"
- [ ] Dropdown opens on click/focus
- [ ] Fuzzy search filters repository list
- [ ] Selecting repository updates draft state
- [ ] Previously selected repository persists as default
- [ ] Widget integrates with TOM Select library correctly

#### Branch Selector

- [ ] TOM Select widget renders with placeholder "Branch"
- [ ] Dropdown shows available branches
- [ ] Fuzzy search filters branch list
- [ ] Selecting branch updates draft state
- [ ] Widget loads branches from API (or mock data)

#### Model Selector (Multi-Select)

- [ ] TOM Select multi-select renders with instance counters
- [ ] Clicking opens model selection popup
- [ ] +/- buttons adjust instance count for each model
- [ ] Instance count displays next to model name
- [ ] Multiple models can be selected simultaneously
- [ ] "Launch Agent(s)" text adjusts based on count (singular/plural)
- [ ] Zero instances removes model from selection

#### TOM Select Features

- [ ] Fuzzy search works across all selectors
- [ ] Keyboard navigation works in dropdowns (arrow keys, Enter, Esc)
- [ ] Popup positioning works correctly (doesn't overflow viewport)
- [ ] Backdrop/overlay displays when dropdown open
- [ ] Smooth animations on open/close

### 7. Task Creation and Form Validation Tests (`task-creation.spec.ts`)

Updated for TOM Select integration:

- ✅ Draft task card renders with text area
- ✅ Repository selector (TOM Select) displays
- ✅ Branch selector (TOM Select) displays
- ✅ Model selector (TOM Select multi-select) displays
- ✅ Go button disabled until all fields valid
- ✅ Form validation shows errors
- ✅ **UPDATED**: Successful submission creates session via API
- ✅ **UPDATED**: Draft removed after successful creation
- [ ] **NEW**: Enter key launches task from draft text area
- [ ] **NEW**: Shift+Enter creates new line in text area

### 8. Session Management Tests (`session-management.spec.ts`)

Validates session display and controls:

- ✅ Session list displays 5 sessions (3 completed, 2 active)
- ✅ Status filter works correctly
- ✅ Session cards show status badges
- ✅ Stop/pause/resume buttons appear contextually
- ✅ **UPDATED**: Clicking session card selects it (visual indicator)
- ✅ **UPDATED**: Enter key navigates to selected session details

### 9. Real-Time Features Tests (`real-time-features.spec.ts`)

Validates SSE streaming and live updates:

- ✅ SSE connection establishes correctly
- ✅ Connection status indicators display
- ✅ **UPDATED**: Active sessions (2 minimum) stream continuous events
- ✅ **UPDATED**: Events include thinking traces
- ✅ **UPDATED**: Events include tool executions
- ✅ **UPDATED**: Events include file edits with diff previews
- ✅ Task cards update live as events arrive
- ✅ Connection errors handled gracefully
- ✅ Automatic reconnection with exponential backoff
- [ ] **NEW**: Tool execution shows last line of output live
- [ ] **NEW**: Completed tools collapse to single line with status indicator

### 10. Accessibility Tests (`accessibility.spec.ts`)

Validates WCAG AA compliance:

- ✅ Semantic HTML structure
- ✅ ARIA landmarks present
- ✅ Keyboard navigation functional
- ✅ Screen reader compatibility
- ✅ Color contrast meets standards
- ✅ Focus indicators visible
- [ ] **NEW**: Arrow key navigation announced to screen readers
- [ ] **NEW**: Selected task state announced
- [ ] **NEW**: Context-sensitive shortcuts accessible

### 11. LocalStorage Persistence Tests (`localstorage-persistence.spec.ts`)

Validates client-side state persistence:

- ✅ Draft tasks persist across page reloads
- ✅ Last selected repository/branch persists
- ✅ UI preferences persist (theme, etc.)
- ✅ State cleared on logout

## Mock Server Requirements

The mock server must be updated to match these specifications:

### Session Data

```typescript
// Mock server must return exactly 5 sessions:
const mockSessions = [
  // 3 completed sessions
  {
    id: 'session-01',
    status: 'completed',
    prompt: 'Implement user authentication',
    // ... full session object
  },
  {
    id: 'session-02',
    status: 'completed',
    prompt: 'Add payment integration',
    // ...
  },
  {
    id: 'session-03',
    status: 'completed',
    prompt: 'Fix responsive layout bug',
    // ...
  },
  // 2 active sessions with SSE streams
  {
    id: 'session-04',
    status: 'running',
    prompt: 'Refactor database queries',
    // ... SSE stream active
  },
  {
    id: 'session-05',
    status: 'running',
    prompt: 'Write E2E tests',
    // ... SSE stream active
  },
];
```

### SSE Event Streams

For active sessions, generate continuous event streams:

```typescript
// Example event types for SSE streams
interface SSEEvent {
  type: 'thinking' | 'tool_execution' | 'file_edit' | 'status';
  sessionId: string;
  timestamp: string;
  data: {
    // Thinking event
    thought?: string;

    // Tool execution event
    tool_name?: string;
    tool_args?: Record<string, any>;
    tool_output?: string; // Last line shown live
    tool_status?: 'running' | 'success' | 'error';

    // File edit event
    file_path?: string;
    diff_preview?: string;
    lines_added?: number;
    lines_removed?: number;
  };
}
```

Example SSE stream sequence:

1. Thinking: "I need to analyze the authentication flow"
2. Tool execution: read_file("auth.ts") - show output live
3. Tool execution: search_codebase("password validation")
4. Thinking: "Found the issue in the validation logic"
5. File edit: auth.ts (+5, -3 lines)
6. Tool execution: run_tests("auth.test.ts")
7. Status: "Tests passing, creating PR"

## SSR Server Requirements

The SSR server must fetch data from the API server during page generation:

```typescript
// Example SSR data fetching in entry-server.tsx
export async function render(url: string, API_BASE_URL: string) {
  // Fetch sessions data during SSR
  const sessionsResponse = await fetch(`${API_BASE_URL}/api/v1/sessions`);
  const sessionsData = await sessionsResponse.json();

  // Render page with data
  return renderToString(() => (
    <App initialSessions={sessionsData.items} />
  ));
}
```

This ensures:

- Users without JavaScript see complete session data
- Initial page load is fully populated
- Progressive enhancement works correctly

## Test Execution Strategy

### Local Development

```bash
# Start mock server
just webui-mock-server

# Start SSR server (with API_BASE_URL=http://localhost:3001)
just webui-dev

# Run tests
just webui-test
```

### CI/CD Pipeline

```bash
# Run all WebUI checks
just webui-check

# Includes:
# - webui-lint
# - webui-type-check
# - webui-build
# - webui-test-api (API contract tests with process-compose)
# - webui-test (E2E tests)
```

## Test File Organization

```
webui/e2e-tests/tests/
├── api-contract.spec.ts          # API contract validation
├── build-tooling.spec.ts         # Build infrastructure
├── example.spec.ts               # SSR rendering
├── layout-navigation.spec.ts     # Layout and navigation
├── keyboard-navigation.spec.ts   # NEW: Keyboard shortcuts
├── tom-select-components.spec.ts # NEW: TOM Select integration
├── task-creation.spec.ts         # Task creation flow
├── session-management.spec.ts    # Session display and controls
├── real-time-features.spec.ts    # SSE streaming
├── accessibility.spec.ts         # WCAG compliance
└── localstorage-persistence.spec.ts # Client state persistence
```

## Coverage Goals

- **PRD Coverage**: 100% of PRD requirements tested
- **API Contract**: 100% of REST endpoints validated
- **Keyboard Navigation**: All shortcuts tested
- **TOM Select**: Full integration coverage
- **SSE Streaming**: All event types validated
- **Accessibility**: WCAG AA compliance verified

## Success Criteria

All tests must pass before any milestone is considered complete:

- ✅ Build tooling tests (compilation, linting, formatting)
- ✅ API contract tests (14/14 passing)
- ✅ SSR rendering tests (HTML structure, branding, assets)
- ✅ Layout tests (simplified task-centric design)
- [ ] Keyboard navigation tests (arrow keys, Enter, shortcuts)
- [ ] TOM Select integration tests (fuzzy search, multi-select)
- ✅ Task creation tests (form validation, submission)
- ✅ Session management tests (display, filtering, controls)
- ✅ Real-time features tests (SSE streams, live updates)
- ✅ Accessibility tests (WCAG AA compliance)
- ✅ LocalStorage persistence tests (draft recovery)

## Next Steps

1. ✅ Update PRD with new requirements
2. ✅ Create comprehensive test strategy document
3. [ ] Update mock server to return 5 sessions with SSE streams
4. [ ] Implement SSR server-side data fetching
5. [ ] Create keyboard-navigation.spec.ts test file
6. [ ] Create tom-select-components.spec.ts test file
7. [ ] Update existing test files for new requirements
8. [ ] Implement keyboard navigation in components
9. [ ] Integrate TOM Select library
10. [ ] Verify all tests passing

## References

- [WebUI-PRD.md](WebUI-PRD.md) - Product requirements
- [WebUI.status.md](WebUI.status.md) - Implementation status
- [REST-Service/API.md](REST-Service/API.md) - API specification
- [TOM Select Documentation](https://tom-select.js.org/) - Widget library
