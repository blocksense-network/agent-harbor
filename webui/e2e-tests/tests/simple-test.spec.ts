import { test, expect } from '@playwright/test';

test('simple test for reporter validation', async ({ page }) => {
  await page.goto('/');

  // Generate some console output to test logging
  await page.evaluate(() => {
    console.log('Test console log message');
    console.warn('Test console warning');
  });

  // Wait a moment for logging to complete
  await page.waitForTimeout(100);

  // Basic assertion
  await expect(page.locator('h1')).toContainText('Agent Harbor');

  console.log('Test completed successfully');
});
