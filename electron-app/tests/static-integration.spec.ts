/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { test, expect } from '@playwright/test';
import { _electron as electron } from 'playwright';
import type { ElectronApplication, Page } from 'playwright';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { createRequire } from 'node:module';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const require = createRequire(import.meta.url);

/**
 * Resolve Electron executable using Node's module resolver (PnP-aware)
 */
function resolveElectronExecutable(): string | undefined {
  try {
    const binPath = require('electron') as unknown as string;
    return binPath;
  } catch {
    return undefined;
  }
}

/**
 * Static Integration Tests
 *
 * These tests verify that Electron can load and render the WebUI
 * from the mock server serving static CSR build files.
 *
 * Prerequisites:
 * - CSR build must exist in webui/app/dist/client/
 * - Mock server must be built and ready to serve static files
 */

test.describe('Static WebUI Integration', () => {
  let electronApp: ElectronApplication;
  let window: Page;
  let mockServer: any;

  test.beforeAll(async () => {
    const { spawn } = await import('child_process');

    // Start mock server
    console.log('Starting mock server...');
    const mockServerPath = path.join(__dirname, '../../webui/mock-server');
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
      } catch {
        // Server not ready yet
      }
      await new Promise(resolve => setTimeout(resolve, 100));
      if (i === maxRetries - 1) {
        throw new Error('Mock server failed to start after 3 seconds');
      }
    }

    const electronExecutable = resolveElectronExecutable();

    console.log('Launching Electron...');
    // Launch Electron app
    electronApp = await electron.launch({
      executablePath: electronExecutable,
      args: [path.join(__dirname, '../dist-electron/index.js')],
      env: {
        ...process.env,
        NODE_ENV: 'test',
        WEBUI_URL: 'http://localhost:3001',
      },
    });

    // Get the first window
    window = await electronApp.firstWindow();

    // Wait for app to be ready
    await window.waitForLoadState('domcontentloaded');
  });

  test.afterAll(async () => {
    await electronApp.close();

    // Stop mock server
    if (mockServer) {
      mockServer.kill();
    }
  });

  test('should load Electron window successfully', async () => {
    expect(window).toBeTruthy();
    const title = await window.title();
    expect(title).toBeTruthy();
  });

  test('should load WebUI from mock server', async () => {
    // Check that we're not on an error page
    const errorText = await window.locator('body').textContent();
    expect(errorText).not.toContain('Failed to load WebUI');

    // Check that the app div exists (created by index.html)
    const appDiv = await window.locator('#app');
    expect(await appDiv.count()).toBe(1);
  });

  test('should render the dashboard', async () => {
    // Wait for the WebUI to hydrate and render
    await window.waitForSelector('h1', { timeout: 10000 });

    // Check for main heading
    const heading = await window.locator('h1').first();
    const headingText = await heading.textContent();
    expect(headingText).toBeTruthy();

    // Dashboard should have the task feed component
    const taskFeed = await window.locator('[data-testid="task-feed"]');
    expect(await taskFeed.count()).toBeGreaterThan(0);
  });

  test('should fetch data from mock API', async () => {
    // Wait for API data to load
    await window.waitForTimeout(2000);

    // Check for session cards (if any sessions exist in mock data)
    // Or check for "no sessions" message
    const body = await window.locator('body').textContent();
    expect(body).toBeTruthy();

    // Verify no console errors about failed API calls
    const logs: string[] = [];
    window.on('console', msg => {
      if (msg.type() === 'error') {
        logs.push(msg.text());
      }
    });

    await window.waitForTimeout(1000);

    // Filter out expected errors (e.g., CORS, dev server notifications)
    const apiErrors = logs.filter(log => log.includes('/api/v1/') && !log.includes('404'));
    expect(apiErrors.length).toBe(0);
  });

  test('should navigate to settings page', async () => {
    // Find and click settings link (adjust selector based on actual UI)
    const settingsLink = window.getByRole('link', { name: /settings/i });

    if ((await settingsLink.count()) > 0) {
      await settingsLink.click();
      await window.waitForTimeout(500);

      // Check that URL changed or content updated
      const url = window.url();
      expect(url).toContain('settings');
    }
  });
});
