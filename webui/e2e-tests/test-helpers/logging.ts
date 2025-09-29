import { Page, TestInfo } from '@playwright/test';

/**
 * Sets up logging for a test page to capture browser console messages,
 * page errors, and other debugging information to the test log file.
 *
 * This follows the AGENTS.md guidelines for comprehensive logging.
 */
export function setupLogging(page: Page, testInfo: TestInfo) {
  // Log browser console messages
  page.on('console', (msg) => {
    const type = msg.type();
    const text = msg.text();

    // Log to test output (will be captured by our custom reporter)
    console.log(`[BROWSER_CONSOLE_${type.toUpperCase()}] ${text}`);

    // Also log location if available
    if (msg.location) {
      console.log(`[BROWSER_LOCATION] ${msg.location.url}:${msg.location.lineNumber}:${msg.location.columnNumber}`);
    }
  });

  // Log page errors
  page.on('pageerror', (error) => {
    console.log(`[PAGE_ERROR] ${error.message}`);
    console.log(`[PAGE_ERROR_STACK] ${error.stack || 'No stack trace'}`);
  });

  // Log request failures
  page.on('requestfailed', (request) => {
    console.log(`[REQUEST_FAILED] ${request.method()} ${request.url()}`);
    console.log(`[REQUEST_FAILURE_REASON] ${request.failure()?.errorText || 'Unknown error'}`);
  });

  // Log navigation events
  page.on('domcontentloaded', () => {
    console.log(`[PAGE_DOM_CONTENT_LOADED] ${page.url()}`);
  });

  page.on('load', () => {
    console.log(`[PAGE_LOAD] ${page.url()}`);
  });

  // Log unhandled promise rejections
  page.on('pageerror', (error) => {
    if (error.name === 'UnhandledPromiseRejection') {
      console.log(`[UNHANDLED_PROMISE_REJECTION] ${error.message}`);
    }
  });

  console.log(`[TEST_SETUP] Logging configured for test: ${testInfo.title}`);
  console.log(`[TEST_FILE] ${testInfo.file}`);
  console.log(`[TEST_LINE] ${testInfo.line}`);
}
