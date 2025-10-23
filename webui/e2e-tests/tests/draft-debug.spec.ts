/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { test, expect } from '@playwright/test';

test.describe('Draft Card Debug', () => {
  test('Debug draft card rendering and API calls', async ({ page }) => {
    // Capture console logs
    const consoleLogs: string[] = [];
    page.on('console', (msg) => {
      consoleLogs.push(`[${msg.type()}] ${msg.text()}`);
    });

    // Capture network requests
    const networkRequests: { url: string; status: number; response?: any }[] = [];
    page.on('response', async (response) => {
      const url = response.url();
      if (url.includes('/api/v1/drafts')) {
        try {
          const body = await response.json();
          networkRequests.push({ url, status: response.status(), response: body });
        } catch {
          networkRequests.push({ url, status: response.status() });
        }
      }
    });

    await page.goto('/');

    // Wait for page to load
    await page.waitForFunction(() => !!document.querySelector('header'), { timeout: 15000 });

    // Wait a bit for async operations
    await page.waitForTimeout(5000);

    // Get page HTML
    const html = await page.content();

    // Extract SSR data from HTML
    const ssrData = await page.evaluate(() => {
      const scripts = Array.from(document.querySelectorAll('script'));
      const ssrScript = scripts.find((s) => s.textContent?.includes('_$HY.r['));
      return ssrScript?.textContent || '';
    });

    // Check what's in the DraftContext
    const draftsCount = await page.evaluate(() => {
      // Try to find draft cards in DOM
      const draftCards = document.querySelectorAll('[data-testid="draft-task-card"]');
      return draftCards.length;
    });

    // Get all textareas
    const textareas = await page.locator('textarea').count();

    console.log('\n========== DEBUG INFO ==========');
    console.log('Network Requests to /api/v1/drafts:');
    console.log(JSON.stringify(networkRequests, null, 2));
    console.log('\nConsole Logs:');
    consoleLogs.forEach((log) => console.log(log));
    console.log('\nDraft Cards Found:', draftsCount);
    console.log('Textareas Found:', textareas);
    console.log('\nHTML Contains "draft-task-card":', html.includes('draft-task-card'));
    console.log('HTML Contains "Describe what you want":', html.includes('Describe what you want'));
    console.log('\nSSR Data (drafts):');
    const draftsMatch = ssrData.match(/_\$HY\.r\["drafts\[\]"\].*?(\$R\[\d+\]=\[.*?\])/);
    console.log(draftsMatch ? draftsMatch[1] : 'Not found in SSR data');
    console.log('================================\n');

    // This test is for debugging only - we'll see the output even if it "fails"
    expect(networkRequests.length).toBeGreaterThan(0);
  });
});
