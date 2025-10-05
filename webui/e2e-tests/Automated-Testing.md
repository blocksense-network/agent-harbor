# WebUI Testing Approach with Automated Logging

## Philosophy

We use **automated tests with comprehensive logging** instead of manual intervention to avoid:
1. **Port conflicts** from manually starting/stopping servers
2. **Hanging processes** that break AI agent development loops  
3. **Inconsistent state** between test runs
4. **Context loss** when manual commands hang or fail

## Testing Layers (Bottom-Up Strategy)

When debugging or verifying functionality, **always test from smallest to largest**:

### Layer 1: API Server Tests (Smallest)

Test the mock/real API server in isolation **without** any frontend code.

```bash
just webui-test-api
```

**What this validates:**
- Mock server starts correctly
- API endpoints return expected JSON
- SSE streams work
- Draft/session CRUD operations

**When to run:** Before any SSR or browser tests. If API tests fail, SSR and browser tests will also fail.

### Layer 2: SSR Rendering Tests (Medium)

Test server-side HTML generation using the SSR server + mock API.

```bash
cd webui
timeout 120 yarn workspace ah-webui-ssr-sidecar run test:run 2>&1 | tee /tmp/ssr-test.log
```

**What this validates:**
- SSR server starts and responds with HTML
- Proxy correctly forwards API requests during SSR
- Initial page HTML contains expected content
- SolidJS hydration markers are present
- Server-side data fetching works (sessions, drafts, etc.)

**When to run:** After API tests pass. If SSR tests fail, the issue is in server-side rendering or proxy configuration, not client-side hydration.

### Layer 3: Browser E2E Tests (Largest)

Test full client-side hydration and interaction with Playwright.

```bash
just webui-test --project=browser-tests
```

**What this validates:**
- Client-side JavaScript bundles load
- SolidJS hydration completes successfully
- User interactions work (clicks, keyboard, etc.)
- Client-side API calls through proxy work
- Dynamic updates and reactivity

**When to run:** After both API and SSR tests pass. If browser tests fail but SSR passes, the issue is in client-side code or hydration.

### Debugging Strategy

When facing failures:

1. **Start at Layer 1** - Always run API tests first
2. **Fix Layer 1 before moving on** - Don't proceed to SSR tests until API tests are green
3. **Test each layer in isolation** - Use layer-specific test commands above
4. **Run individual tests** - Use `--grep` to focus on specific failing tests
5. **Only run full suite when all layers pass individually**

Example debugging workflow:

```bash
# Step 1: Verify API layer
just webui-test-api
# ✅ All pass → continue
# ❌ Failures → fix API server before proceeding

# Step 2: Verify SSR layer
just webui-test-ssr
# ✅ All pass → continue
# ❌ Failures → fix SSR rendering/proxy before proceeding

# Step 3: Verify browser layer (one test at a time if needed)
just webui-test --grep="specific test name"
# ✅ Pass → try next test
# ❌ Fail → debug client-side code

# Step 4: Full suite when all layers work
just webui-test --project=browser-tests
```

## Running Individual Tests (Fast Iteration)

Running the full test suite is slow (~2-3 minutes). For fast iteration during development, run individual tests or test files:

### Run a Single Test File

```bash
just webui-test tests/sse-live-updates.spec.ts
```

### Run Tests by Pattern/Name

```bash
# Run all SSE-related tests
just webui-test --grep "SSE"

# Run all TOM Select tests
just webui-test --grep "TOM Select"

# Run specific test by exact name
just webui-test --grep "Active session cards subscribe to SSE"
```

### Run Specific Project

```bash
# Run only API tests (fast, ~30 seconds)
just webui-test-api

# Run only infrastructure tests (medium, ~1 minute)
just webui-test --project=infrastructure-tests

# Run only browser tests (slow, ~2 minutes)
just webui-test --project=browser-tests
```

### Run Multiple Specific Files

```bash
# Run SSE and TOM Select tests together
just webui-test tests/sse-live-updates.spec.ts tests/tom-select-customization.spec.ts
```


### Quick Reference

| Task | Command | Time |
|------|---------|------|
| Full suite | `just webui-test` | ~3 min |
| Browser tests only | `just webui-test --project=browser-tests` | ~2 min |
| API tests only | `just webui-test-api` | ~30 sec |
| Single test file | `just webui-test tests/FILE.spec.ts` | ~1 min |
| Tests matching pattern | `just webui-test --grep "PATTERN"` | ~1-2 min |
| New SSE + TOM tests | `just webui-test tests/sse-live-updates.spec.ts tests/tom-select-customization.spec.ts` | ~1.5 min |

### Example: Fixing a Failing Test

```bash
# 1. Full suite reports "SSE Live Updates" tests failing
just webui-test

# 2. Run only SSE tests to isolate the issue (logs automatically saved)
just webui-test --grep "SSE Live"

# 3. Fix the code

# 4. Re-run just SSE tests (fast verification)
just webui-test --grep "SSE Live"

# 5. When SSE tests pass, run full suite to ensure no regressions
just webui-test

# 6. Check logs if still failing
LATEST_RUN=$(ls -td test-results/logs/test-run-* | head -1)
grep "\[PAGE_ERROR\]" "$LATEST_RUN"/*.log
grep "\[BROWSER_CONSOLE_ERROR\]" "$LATEST_RUN"/*.log
```

## Test Execution

### Automated Server Management

The `start-servers.sh` script automatically:
- Builds and starts the mock API server (port 3001)
- Builds and starts the SSR server (port 3002)
- Waits for both ports to be ready
- Runs Playwright tests with any arguments you pass
- Cleans up both servers on exit

### Mandatory Timeouts

**The just commands automatically handle timeouts** to prevent hangs:

```bash
# Good - automatic timeout handling
just webui-test --project=browser-tests

# For manual start-servers.sh usage (not recommended):
timeout 240 bash start-servers.sh --project=browser-tests
```

### Sophisticated Logging System

**All test runs automatically save comprehensive logs** - no manual capture needed:

```bash
# Just run tests - logging happens automatically
just webui-test --project=browser-tests
```

The WebUI test suite uses a comprehensive logging system that captures everything:

#### Automatic Per-Test Logging
- **Individual log files**: Each test gets its own timestamped log file
- **Browser console capture**: All `console.log`, `console.error`, etc. from browser
- **Page errors**: JavaScript errors and unhandled promise rejections
- **Network requests**: Failed requests with detailed error information
- **Navigation events**: DOM content loaded and page load events
- **Server logs**: Both mock server and SSR server output captured separately

#### Log File Locations
- `test-results/logs/test-run-YYYY-MM-DDTHH-MM-SS/` - Timestamped directory per test run
  - `mock-server.log` - Mock API server output
  - `ssr-server.log` - SSR server output
- `test-results/logs/YYYY-MM-DDTHH-MM-SS-N.log` - Individual test log files
- **Minimal console output**: Only success/failure indicators shown during test run

### Analyzing Logs

After a test run, examine logs for specific information:

```bash
# Check for draft-related activity
grep -i "draft" test-results/logs/*.log | head -20

# Check browser console logs
grep "\[BROWSER_CONSOLE_" test-results/logs/*.log

# Check page errors and network failures
grep -E "\[PAGE_ERROR\]|\[REQUEST_FAILED\]" test-results/logs/*.log

# Check SSR server activity
grep "\[SSR\]" test-results/logs/*server.log

# Check test results and failures
grep -E "(❌|✅|⏭️)" test-results/logs/*.log

# View server startup logs
cat test-results/logs/test-run-*/mock-server.log | head -50
cat test-results/logs/test-run-*/ssr-server.log | head -50
```

## Debug Tests & Comprehensive Logging

The test suite includes automatic comprehensive logging for all tests:

### Automatic Logging Features
- **Browser Console Capture**: All `console.log`, `console.warn`, `console.error` messages
- **Page Error Detection**: JavaScript errors and unhandled promise rejections
- **Network Monitoring**: Failed HTTP requests with error details
- **Navigation Tracking**: DOM content loaded and page load events
- **Server Log Integration**: Mock server and SSR server output captured

### Debug Tests
Special debug tests like `draft-debug.spec.ts` provide additional diagnostic information:
- Network requests to API endpoints
- Browser console logs
- DOM element counts
- HTML content inspection

Debug tests are **designed to "fail"** but provide extensive diagnostic output for development.

## Benefits of This Approach

1. **No Port Conflicts**: Servers are started fresh for each run
2. **No Hangs**: Timeout ensures tests complete or abort cleanly
3. **Comprehensive Logging**: Captures browser console, network requests, page errors, and server logs
4. **Minimal Console Output**: Clean test runs with full details saved to files
5. **AI-Friendly**: Logs can be examined programmatically without manual intervention
6. **Reproducible**: Same command produces consistent results
7. **Per-Test Isolation**: Each test gets its own log file for easy debugging

## Adding New Debug Information

To add new diagnostic information:

1. **For browser-side logging**: Use the `setupLogging()` helper in your test:
   ```typescript
   import { setupLogging } from '../test-helpers/logging';
   // In your test:
   test('my test', async ({ page }) => {
     setupLogging(page, testInfo);
     // Your test code - all browser console/errors automatically logged
   });
   ```

2. **For server-side logging**: Add console.log statements with distinctive prefixes:
   ```typescript
   console.log(`[MY_COMPONENT] Debug info: ${someValue}`);
   ```

3. **Run tests** (logging is automatic):
   ```bash
   just webui-test --grep "my test"
   ```
   *For manual start-servers.sh usage (not recommended):*
   ```bash
   cd webui/e2e-tests && timeout 120 ./start-servers.sh --grep "my test" 2>&1 | tee /tmp/debug.log
   ```

4. **Examine logs**:
   ```bash
   # Browser console logs
   grep "\[BROWSER_CONSOLE_" test-results/logs/*.log

   # Your custom prefixes
   grep "\[MY_COMPONENT\]" test-results/logs/*.log

   # Server-side logs
   grep "\[SSR\]" test-results/logs/*server.log
   ```

## Example Workflow

```bash
# Run tests with full logging (automatic with just command)
just webui-test --project=browser-tests

# Check results (minimal output shown, full logs saved)
echo "Test summary:"
ls -la test-results/logs/ | tail -10

# Investigate specific test failures
# Find the latest test run directory
LATEST_RUN=$(ls -td test-results/logs/test-run-* | head -1)
echo "Latest run: $LATEST_RUN"

# Check server logs for startup issues
head -30 "$LATEST_RUN/mock-server.log"
head -30 "$LATEST_RUN/ssr-server.log"

# Check specific test logs for errors
grep "❌" test-results/logs/*.log -l | xargs grep -A 5 "\[PAGE_ERROR\]"
```

## Never Do

❌ Manually start `yarn workspace <package> run dev` servers
❌ Run tests directly with `yarn workspace ah-webui-e2e-tests run test` (use just commands)  
❌ Use `curl` commands that can hang  
❌ Leave servers running between test runs  
❌ Manually manage log capture (it's automatic)  

## Always Do

✅ Use `just webui-test` command (handles timeouts and server lifecycle)  
✅ Let the logging system capture everything automatically  
✅ Examine logs post-facto instead of watching live output  
✅ Use `setupLogging()` helper for comprehensive browser capture  
✅ Use debug tests for detailed diagnostics  
