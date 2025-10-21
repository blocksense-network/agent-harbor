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

## Code Quality & Linting

### Handling ESLint Tailwind Class Errors

When the `better-tailwindcss` ESLint plugin reports "unregistered class" errors, follow these guidelines:

#### ✅ **Only Allow These Exceptions**

**ESLint ignore list should ONLY contain CSS classes from third-party libraries** that are imported in `src/app.css`. No other exceptions are allowed.

```javascript
// eslint.config.js
rules: {
  'better-tailwindcss/no-unregistered-classes': ['error', {
    ignore: ['tom-select-input', 'model-multi-select'] // Only third-party classes
  }]
}
```

NEVER add **custom semantic classes** that are used only for component identification. You can replace these with custom data attributes:

```tsx
// ❌ Bad: Semantic class (requires ESLint ignore)
<div class="my-custom-component bg-blue-500">
  <button>Click me</button>
</div>

// ✅ Good: Data attribute (no ESLint ignore needed)
<div data-component class="bg-blue-500">
  <button>Click me</button>
</div>
```

**Benefits:**

- ✅ No ESLint exceptions required
- ✅ Clean separation of styling vs. identification
- ✅ Better for E2E testing (`[data-component]` selectors)
- ✅ Follows modern web standards

#### 🧪 **E2E Testing with Data Attributes**

Use data attribute selectors in your Playwright tests:

```typescript
// Component with data attribute
<div data-toast class="bg-red-100 text-red-800">
  Error message
</div>

// E2E test selector
const toast = page.locator('[data-toast]');
```

**Remember:** The ignore list should be minimal and only contain truly unavoidable third-party classes. Everything else should use data attributes!

## Contributing

1. Follow the established patterns in the codebase
2. Write tests for new features
3. Ensure all linting passes
4. Test across different browsers when making UI changes
5. Update this guide when adding new development workflows
