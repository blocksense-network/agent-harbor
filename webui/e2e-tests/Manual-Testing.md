# WebUI Manual Testing Guide

This guide covers manual testing approaches for WebUI development, including visual debugging and interactive test execution.

## Debug Mode (Visual Browser)

For debugging individual test failures, run in headed mode to see the browser:

```bash
cd webui/e2e-tests

# Option 1: Using yarn script
yarn workspace ah-webui-e2e-tests run test:headed -- --grep "SSE"

# Option 2: Direct Playwright command (requires manual server setup)
# Terminal 1:
yarn workspace ah-webui-mock-server run dev

# Terminal 2:
yarn workspace ah-webui-ssr-sidecar run dev

# Terminal 3:
cd ../e2e-tests
yarn test --headed --grep "SSE"
```

## Interactive UI Mode

Playwright's UI mode provides a visual test runner:

```bash
cd webui
yarn workspace ah-webui-e2e-tests run test:ui
```

This opens a GUI where you can:

- Click individual tests to run them
- See test code alongside browser
- Step through tests interactively
- Inspect DOM and network requests

**Note:** UI mode requires manually starting servers first (see Debug Mode above).
