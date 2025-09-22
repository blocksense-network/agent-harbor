# WebUI Development Guide

## Running Tests

### Local Development Testing

**WebUI App:**

```bash
just webui-lint          # ESLint code quality checks
just webui-format        # Prettier code formatting
just webui-type-check    # TypeScript type checking
just webui-build         # Production build verification
just webui-dev           # Start development server (http://localhost:3000)
```

**Mock Server:**

```bash
just webui-build-mock    # Production build verification
just webui-mock-server   # Start mock API server (http://localhost:3001)
```

**E2E Tests:**

```bash
just webui-install-browsers   # Install Playwright browsers
just webui-test               # Run all E2E tests
just webui-test-headed        # Run tests in headed mode (visible browser)
just webui-test-debug         # Debug tests step-by-step
just webui-test-ui            # Interactive test runner UI
just webui-test-report        # View test reports after runs
```

### Full WebUI Test Suite

Run all WebUI components together for integration testing:

```bash
# Terminal 1: Start mock server
just webui-mock-server

# Terminal 2: Start WebUI app
just webui-dev

# Terminal 3: Run E2E tests
just webui-test
```

### Repository-wide Testing

Use the project's just targets for comprehensive testing:

```bash
just test              # Run all Rust tests
just lint-specs        # Lint markdown files
just webui-check       # Run all WebUI checks (lint, type-check, build, test)
```

## Development Workflow

1. **Install dependencies:**

   ```bash
   just webui-install
   ```

2. **Start development servers:**

   ```bash
   # Terminal 1: Mock API server
   just webui-mock-server

   # Terminal 2: WebUI app
   just webui-dev
   ```

3. **Run tests continuously:**

   ```bash
   # Terminal 3: E2E tests
   just webui-test
   ```

4. **Code quality checks:**
   ```bash
   just webui-lint
   just webui-type-check
   just webui-format
   ```

## Architecture

The WebUI consists of three main components:

- **`webui/app/`**: SolidJS + Tailwind CSS frontend application
- **`webui/mock-server/`**: Express.js mock REST API server
- **`webui/e2e-tests/`**: Playwright end-to-end test suite

### Data Flow

```
Browser → WebUI App → Mock Server → REST API Responses
                    ↓
            Playwright Tests
```

### Technology Stack

- **Frontend**: SolidJS, TypeScript, Tailwind CSS, Vite
- **Backend**: Node.js, Express, TypeScript
- **Testing**: Playwright, ESLint, Prettier
- **Build**: Vite (frontend), TypeScript compiler (backend)

## Contributing

1. Follow the established patterns in the codebase
2. Write tests for new features
3. Ensure all linting passes
4. Test across different browsers when making UI changes
5. Update this guide when adding new development workflows
