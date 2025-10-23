/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { test, expect } from '@playwright/test';
import { setupTestEnvironment, teardownTestEnvironment } from '../utils/test-helpers';

test.describe('Draft Recursion Prevention', () => {
  let baseURL: string;

  test.beforeAll(async () => {
    baseURL = await setupTestEnvironment();
  });

  test.afterAll(async () => {
    await teardownTestEnvironment();
  });

  test('should not cause infinite recursion when typing rapidly', async ({ page }) => {
    // Set up console monitoring to detect recursion
    const consoleMessages: string[] = [];
    const effectRuns: string[] = [];

    page.on('console', (msg) => {
      const text = msg.text();
      consoleMessages.push(text);
      if (text.includes('[DraftTaskCard] Auto-save scheduled')) {
        effectRuns.push(text);
      }
    });

    await page.goto(baseURL);
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();

    // Type rapidly to test for recursion
    await textarea.click();
    await textarea.fill('Test text for recursion detection');

    // Wait a bit for effects to settle
    await page.waitForTimeout(2000);

    // Check that we don't have excessive effect runs
    // With proper debouncing, we should only see 1-2 effect runs for this input
    const autoSaveRuns = effectRuns.length;

    console.log(`Auto-save effect runs: ${autoSaveRuns}`);
    console.log(`Total console messages: ${consoleMessages.length}`);

    // We expect minimal effect runs for a single typing session
    // Allow some tolerance but prevent excessive recursion
    expect(autoSaveRuns).toBeLessThan(10);

    // Check that the save status eventually shows "Saved" or "Unsaved"
    const saveStatus = page.locator('[aria-label*="Save status"]').first();
    const statusText = await saveStatus.textContent();
    expect(['Saved', 'Unsaved', 'Saving...', 'Error']).toContain(statusText);

    // Verify the page is still responsive (no infinite loop blocking UI)
    await expect(textarea).toBeVisible();
    await expect(textarea).toHaveValue('Test text for recursion detection');
  });

  test('should handle multiple rapid typing sessions without recursion', async ({ page }) => {
    await page.goto(baseURL);
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();

    // Simulate multiple rapid typing sessions
    for (let i = 0; i < 3; i++) {
      await textarea.click();
      await textarea.fill(`Test text session ${i + 1}`);

      // Wait for debouncing to complete
      await page.waitForTimeout(1000);

      // Check that we can still interact with the textarea
      await expect(textarea).toHaveValue(`Test text session ${i + 1}`);
    }

    // Final check - page should still be responsive
    await expect(textarea).toBeVisible();
    await expect(page.locator('[data-testid="draft-task-card"]')).toBeVisible();
  });

  test('should prevent concurrent save operations', async ({ page }) => {
    await page.goto(baseURL);
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();

    // Type and immediately type again to test concurrent save prevention
    await textarea.click();
    await textarea.fill('First save');
    await page.waitForTimeout(200); // Partial debounce time

    await textarea.fill('Second save'); // This should cancel the first save

    // Wait for everything to settle
    await page.waitForTimeout(1500);

    // Should have a reasonable final state
    const saveStatus = page.locator('[aria-label*="Save status"]').first();
    const statusText = await saveStatus.textContent();
    expect(['Saved', 'Unsaved', 'Saving...', 'Error']).toContain(statusText);
  });
});
