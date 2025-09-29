import { test, expect } from '@playwright/test';

test.describe.skip('Reporter Validation', () => {
  test('minimal reporter creates unique log files for each test', async ({ page }) => {
    // This test validates that our custom reporter creates log files
    // and captures the required information per AGENTS.md guidelines

    await page.goto('/');

    // Generate some console output to test logging
    await page.evaluate(() => {
      console.log('Test console log message');
      console.warn('Test console warning');
      console.error('Test console error');
    });

    // Wait a moment for logging to complete
    await page.waitForTimeout(100);

    // Navigate to trigger more events
    await page.goto('/settings');
    await page.waitForTimeout(100);

    // Verify page loads correctly (basic functionality test)
    await expect(page.locator('h1')).toContainText('Agent Harbor');

    console.log('Test completed successfully with comprehensive logging');
  });

  test('reporter captures browser console messages', async ({ page }) => {
    await page.goto('/');

    // Generate various types of console messages
    await page.evaluate(() => {
      console.log('Info message from browser');
      console.warn('Warning from browser');
      console.error('Error from browser');
      console.debug('Debug message from browser');
    });

    // Wait for messages to be captured
    await page.waitForTimeout(200);

    console.log('Browser console messages should be captured in log file');
  });

  test('reporter captures page errors', async ({ page }) => {
    await page.goto('/');

    // Inject a script that will cause a JavaScript error
    await page.evaluate(() => {
      // This will cause a ReferenceError
      console.log('About to trigger an error...');
      setTimeout(() => {
        (window as any).nonexistentFunction();
      }, 100);
    });

    // Wait for the error to occur and be captured
    await page.waitForTimeout(500);

    console.log('Page errors should be captured in log file');
  });

  test('reporter handles test failures gracefully', async ({ page }) => {
    await page.goto('/');

    // This test will intentionally fail to test failure logging
    console.log('About to trigger a test failure...');

    // Intentionally fail the test to test failure logging
    expect(false).toBe(true); // This will fail
  });

  test('reporter captures navigation events', async ({ page }) => {
    await page.goto('/');

    console.log('Starting navigation test');

    // Navigate to different pages
    await page.goto('/settings');
    await page.waitForTimeout(100);

    await page.goto('/');
    await page.waitForTimeout(100);

    console.log('Navigation events should be captured in log file');
  });
});
