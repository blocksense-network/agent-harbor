/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { test, expect, _electron as electron } from '@playwright/test';
import { ElectronApplication, Page } from 'playwright';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { createRequire } from 'node:module';
import { spawn, ChildProcess } from 'child_process';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const require = createRequire(import.meta.url);

let mockServer: ChildProcess | null = null;

/**
 * Resolve Electron executable using Node's module resolver (PnP-aware)
 * This matches the approach used in vite.config.ts
 */
function resolveElectronExecutable(): string | undefined {
  try {
    // Electron package exports the binary path as a string
    const binPath = require('electron') as unknown as string;
    return binPath;
  } catch {
    return undefined;
  }
}

let electronApp: ElectronApplication;
let page: Page;
const errors: string[] = [];
const warnings: string[] = [];

test.beforeAll(async () => {
  console.log('Starting mock server...');
  // Start mock server
  const mockServerPath = join(__dirname, '../../webui/mock-server');
  mockServer = spawn('yarn', ['node', 'dist/index.js'], {
    cwd: mockServerPath,
    stdio: 'inherit',
    env: { ...process.env, QUIET_MODE: 'true' },
  });

  // Wait for mock server to be ready
  console.log('Waiting for mock server...');
  const maxRetries = 30;
  for (let i = 0; i < maxRetries; i++) {
    try {
      const response = await fetch('http://localhost:3001/health');
      if (response.ok) {
        console.log('Mock server is ready!');
        break;
      }
    } catch (e) {
      // Server not ready yet
    }
    await new Promise(resolve => setTimeout(resolve, 100));
    if (i === maxRetries - 1) {
      throw new Error('Mock server failed to start after 3 seconds');
    }
  }

  const electronExecutable = resolveElectronExecutable();

  console.log('Launching Electron...');
  console.log('  Executable:', electronExecutable);

  // Launch Electron app
  // Native addon is copied to dist-electron/node_modules by copy-native-addon.sh script
  // This allows Electron to find it without needing PnP resolution
  electronApp = await electron.launch({
    executablePath: electronExecutable,
    args: [join(__dirname, '../dist-electron/index.js')],
    timeout: 30000,
    env: {
      ...process.env,
      NODE_ENV: 'test',
    },
  });

  console.log('Electron launched, waiting for first window...');
  // Wait for the first window
  page = await electronApp.firstWindow();
  console.log('Got first window!');

  // Set up error listeners BEFORE the page loads
  page.on('pageerror', error => {
    errors.push(`Page error: ${error.message}\n${error.stack}`);
  });

  page.on('console', msg => {
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

  // Stop mock server
  if (mockServer) {
    mockServer.kill();
    mockServer = null;
  }
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
  const unexpectedErrors = errors.filter(err => {
    // Expected: API 404 errors when backend isn't running
    if (
      err.includes('Failed to fetch sessions') ||
      err.includes('Failed to fetch drafts') ||
      err.includes('Failed to refresh sessions')
    ) {
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
