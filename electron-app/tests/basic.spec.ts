import { test, expect, _electron as electron } from '@playwright/test';
import { ElectronApplication, Page } from 'playwright';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

let electronApp: ElectronApplication;
let page: Page;
const errors: string[] = [];
const warnings: string[] = [];

test.beforeAll(async () => {
  // Launch Electron app
  electronApp = await electron.launch({
    args: [join(__dirname, '../dist-electron/index.js')],
    env: {
      ...process.env,
      NODE_ENV: 'test',
    },
  });

  // Wait for the first window
  page = await electronApp.firstWindow();

  // Set up error listeners BEFORE the page loads
  page.on('pageerror', (error) => {
    errors.push(`Page error: ${error.message}\n${error.stack}`);
  });

  page.on('console', (msg) => {
    const text = msg.text();
    const type = msg.type();

    if (type === 'error') {
      errors.push(`Console error: ${text}`);
    } else if (type === 'warning') {
      warnings.push(`Console warning: ${text}`);
    }
  });

  // Wait for the app to be ready
  await page.waitForLoadState('domcontentloaded');
});

test.afterAll(async () => {
  await electronApp.close();
});

test('should launch electron app', async () => {
  // Check that the window exists
  expect(page).toBeTruthy();

  // Check window title
  const title = await page.title();
  expect(title).toBe('Agent Harbor');
});

test('should load the WebUI', async () => {
  // Wait for the app div to be present
  await page.waitForSelector('#app', { timeout: 10000 });

  const appDiv = await page.locator('#app');
  expect(await appDiv.isVisible()).toBe(true);
});

test('should not have console errors', async () => {
  // Wait for app to load and potential errors to surface
  await page.waitForTimeout(5000);

  // Filter out expected errors (API 404s when backend isn't running)
  const unexpectedErrors = errors.filter((err) => {
    // Expected: API 404 errors when backend isn't running
    if (err.includes('Failed to fetch sessions') ||
        err.includes('Failed to fetch drafts') ||
        err.includes('Failed to refresh sessions')) {
      return false;
    }
    // Expected: 404 for missing resources during load
    if (err.includes('Failed to load resource') && err.includes('404')) {
      return false;
    }
    return true;
  });

  // Log errors and warnings for debugging
  if (errors.length > 0) {
    console.log('\n❌ Errors detected:');
    errors.forEach((err, i) => console.log(`  ${i + 1}. ${err}`));
  }

  if (warnings.length > 0) {
    console.log('\n⚠️  Warnings detected:');
    warnings.forEach((warn, i) => console.log(`  ${i + 1}. ${warn}`));
  }

  // Fail the test only if there are UNEXPECTED errors
  expect(unexpectedErrors, 'No unexpected console errors should be present').toHaveLength(0);
});

test('should expose preload APIs', async () => {
  // Check that agentHarborConfig is available
  const config = await page.evaluate(() => {
    return (window as any).agentHarborConfig;
  });

  expect(config).toBeTruthy();
  expect(config.isElectron).toBe(true);
  expect(config.apiBaseUrl).toBeTruthy();
});
