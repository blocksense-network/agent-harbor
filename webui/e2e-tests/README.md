# Agents-Workflow WebUI E2E Tests

This directory contains comprehensive end-to-end tests for the Agents-Workflow WebUI, implementing the test coverage requirements outlined in `WebUI.status.md`.

## Test Coverage Implemented

### Phase 1: Foundation (W1.5-W2) ✅ COMPLETED

#### Infrastructure Tests
- SSR sidecar serves HTML correctly
- Health endpoint functionality
- API proxy forwards requests
- CORS headers configuration
- Security headers validation
- 404 error handling

#### API Contract Tests
- GET /api/v1/agents returns correct schema
- GET /api/v1/runtimes returns correct schema
- GET /api/v1/executors returns correct schema
- GET /api/v1/sessions with filtering and pagination
- POST /api/v1/tasks creates sessions correctly
- GET /api/v1/sessions/:id returns session details
- POST /api/v1/sessions/:id/stop works correctly
- DELETE /api/v1/sessions/:id cancels sessions
- GET /api/v1/sessions/:id/logs returns logs
- GET /api/v1/sessions/:id/events establishes SSE streams
- Error handling and validation

#### Build and Tooling Tests
- All projects compile successfully with TypeScript strict mode
- ESLint configuration works across all projects
- Prettier configuration works across all projects
- Package.json files are valid JSON
- tsconfig.json files are valid JSON
- Playwright configuration validation

#### Layout and Navigation Tests
- Three-pane layout renders correctly on desktop
- Navigation links work and highlight active routes
- Collapsible panes work correctly
- Layout adapts to different screen sizes
- Global search interface renders
- URL routing works correctly
- Browser back/forward navigation

#### Accessibility Tests
- WCAG AA compliance checks with axe-core
- Keyboard navigation works on main pages
- ARIA landmarks are present
- Form elements have proper labels
- Color contrast meets standards
- Focus indicators are visible

#### localStorage Persistence Tests
- UI preferences persist across browser sessions
- Global search preferences are saved
- Theme preferences persist
- Form drafts are saved locally
- Pane sizes are remembered
- localStorage doesn't contain sensitive data
- Reasonable size limits

## Test Structure

```
tests/
├── api-contract.spec.ts      # REST API contract validation
├── build-tooling.spec.ts     # Build and development tooling
├── example.spec.ts          # Basic infrastructure tests
├── layout-navigation.spec.ts # UI layout and navigation
├── accessibility.spec.ts     # Accessibility compliance
└── localstorage-persistence.spec.ts # Client-side persistence
```

## Running the Tests

### Prerequisites

1. **Enter Nix Development Environment** (recommended):
```bash
# From the webui directory
cd webui && nix develop
```

2. Install dependencies for all WebUI projects:
```bash
# From webui/ directory (still in nix develop shell)
cd webui/mock-server && npm install
cd ../app && npm install
cd ../e2e-tests && npm install
```

**Note**: With Nix, Playwright browsers are automatically provided and don't need manual installation. The environment variables for Playwright are set automatically in the Nix shell.

### Running Specific Test Suites

```bash
# API contract tests (require mock server)
npm run test -- --project=api-tests

# Build and tooling tests
npm run test -- --project=build-tooling-tests

# Infrastructure tests
npm run test -- --project=infrastructure-tests

# Browser-based UI tests (require both servers)
npm run test -- --project=browser-tests
```

### Running All Tests

To run all tests with automatic server management (recommended):

```bash
# This starts both servers, waits for them to be ready, runs tests, and cleans up
npm run test:e2e
```

For manual server management (for debugging):

```bash
# Terminal 1: Start mock server
cd ../mock-server && npm run dev

# Terminal 2: Start SSR sidecar
cd ../app && npm start

# Terminal 3: Run all tests
cd ../e2e-tests && npm test
```

### Test Configuration

The tests are organized into different Playwright projects:

- **api-tests**: Test REST API contracts (no browser required)
- **build-tooling-tests**: Test build processes and tooling (no browser required)
- **infrastructure-tests**: Test basic server functionality (no browser required)
- **browser-tests**: Test UI functionality (requires browser and servers)

## Test Results

After running tests, view the HTML report:
```bash
npm run report
```

## CI/CD Integration

These tests are designed to run in CI/CD pipelines with the following workflow:

1. Build all projects with TypeScript strict mode
2. Run API and tooling tests (fast, no browser needed)
3. Start mock server and SSR sidecar
4. Run browser-based UI tests
5. Generate test reports and coverage

## Future Enhancements

### Phase 2: Core Functionality (W3-W4)
- Form validation tests
- Session CRUD operations
- Real-time SSE event testing

### Phase 3: Advanced Features (W5-W6)
- IDE integration tests
- Governance and RBAC tests

### Phase 4: Production Readiness (W7-W8)
- Complete user journey E2E tests
- Performance regression tests
- Visual regression testing
- Cross-browser compatibility

## Troubleshooting

### Common Issues

1. **Nix shell not active**: Ensure you're running `nix develop` from the `linux-sandbox` directory
2. **Browser dependencies missing**: Playwright should use Nix-provided browsers automatically; verify `PLAYWRIGHT_BROWSERS_PATH` is set
3. **Servers not starting**: Ensure mock-server and app dependencies are installed
4. **Port conflicts**: Tests expect mock server on port 3001, SSR on port 3000
5. **TypeScript compilation errors**: Ensure all projects have been built with `npm run build`

### Debug Mode

Run tests in debug mode:
```bash
npm run test:debug -- --project=api-tests
```

### Verbose Output

For detailed test execution:
```bash
DEBUG=pw:api npm test -- --project=api-tests
```

### Nix-Specific Troubleshooting

- **Playwright can't find browsers**: Check that `PLAYWRIGHT_BROWSERS_PATH` is set correctly in your Nix shell
- **Library dependencies**: If you see lib errors despite Nix, the playwright-driver.browsers package should include all required libs
- **Server startup failures**: The `start-servers.sh` script uses `nc` (netcat) and `kill` from Nix packages
