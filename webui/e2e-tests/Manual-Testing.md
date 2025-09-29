# WebUI Manual Testing Guide

This guide covers manual testing approaches for WebUI development, including visual debugging and interactive test execution.

## Debug Mode (Visual Browser)

For debugging individual test failures, run in headed mode to see the browser:

```bash
cd webui/e2e-tests

# Option 1: Using npm script
npm run test:headed -- --grep "SSE"

# Option 2: Direct Playwright command (requires manual server setup)
# Terminal 1:
cd ../mock-server && npm run dev

# Terminal 2:
cd ../app && npm run dev

# Terminal 3:
cd ../e2e-tests
npx playwright test --headed --grep "SSE"
```

## Interactive UI Mode

Playwright's UI mode provides a visual test runner:

```bash
cd webui/e2e-tests
npm run test:ui
```

This opens a GUI where you can:
- Click individual tests to run them
- See test code alongside browser
- Step through tests interactively
- Inspect DOM and network requests

**Note:** UI mode requires manually starting servers first (see Debug Mode above).
