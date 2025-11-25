/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { test, expect } from '@playwright/test';

test.describe('Draft Save Status Algorithm', () => {
  let baseURL: string;

  test("should show 'Saved' status initially for new drafts", async ({ page }) => {
    await page.goto(baseURL);
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const saveStatus = page.locator('[aria-label*="Save status"]').first();
    await expect(saveStatus).toHaveText('Saved');
  });

  test("should show 'Unsaved' immediately when typing starts", async ({ page }) => {
    await page.goto(baseURL);
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const saveStatus = page.locator('[aria-label*="Save status"]').first();

    await expect(saveStatus).toHaveText('Saved');

    await textarea.click();
    await textarea.type('Hello');

    await expect(saveStatus).toHaveText('Unsaved');
  });

  test("should transition to 'Saving...' after 500ms of inactivity", async ({ page }) => {
    await page.goto(baseURL);
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const saveStatus = page.locator('[aria-label*="Save status"]').first();

    await textarea.click();
    await textarea.fill('Test content');

    await expect(saveStatus).toHaveText('Unsaved');

    await page.waitForTimeout(600);

    await expect(saveStatus).toHaveText('Saving...');
  });

  test("should show 'Saved' after successful save", async ({ page }) => {
    await page.goto(baseURL);
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const saveStatus = page.locator('[aria-label*="Save status"]').first();

    await textarea.click();
    await textarea.fill('Content to save');
    await expect(saveStatus).toHaveText('Unsaved');

    await page.waitForTimeout(1500);
    await expect(saveStatus).toHaveText('Saved');
  });

  test('should invalidate previous save requests when typing continues', async ({ page }) => {
    await page.goto(baseURL);
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const saveStatus = page.locator('[aria-label*="Save status"]').first();

    await textarea.click();
    await textarea.fill('First content');
    await expect(saveStatus).toHaveText('Unsaved');

    await page.waitForTimeout(200);
    await textarea.type(' and more');

    await expect(saveStatus).toHaveText('Unsaved');

    await page.waitForTimeout(1000);
    await expect(saveStatus).toHaveText('Saved');

    await expect(textarea).toHaveValue('First content and more');
  });

  test('should prevent text truncation during concurrent typing', async ({ page }) => {
    await page.goto(baseURL);
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const saveStatus = page.locator('[aria-label*="Save status"]').first();

    await textarea.click();

    await textarea.fill('Initial text');
    await expect(saveStatus).toHaveText('Unsaved');

    await page.waitForTimeout(100);
    await textarea.type(' continued');

    await page.waitForTimeout(100);
    await textarea.type(' and finished');

    await page.waitForTimeout(1500);

    await expect(saveStatus).toHaveText('Saved');
    await expect(textarea).toHaveValue('Initial text continued and finished');
  });

  test('should handle rapid typing sessions without issues', async ({ page }) => {
    await page.goto(baseURL);
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const saveStatus = page.locator('[aria-label*="Save status"]').first();

    const testTexts = [
      'First draft version',
      'Second draft with changes',
      'Third version after review',
      'Final version ready',
    ];

    for (const text of testTexts) {
      await textarea.click();
      await textarea.fill(text);
      await expect(saveStatus).toHaveText('Unsaved');

      await page.waitForTimeout(1000);
      await expect(saveStatus).toHaveText('Saved');
      await expect(textarea).toHaveValue(text);
    }
  });

  test('should maintain status consistency across interactions', async ({ page }) => {
    await page.goto(baseURL);
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const saveStatus = page.locator('[aria-label*="Save status"]').first();

    await textarea.click();
    await textarea.fill('Test content for consistency');
    await page.waitForTimeout(1500);
    await expect(saveStatus).toHaveText('Saved');

    await page.reload();
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const newTextarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const newSaveStatus = page.locator('[aria-label*="Save status"]').first();

    await expect(newSaveStatus).toHaveText('Saved');
    await expect(newTextarea).toHaveValue('Test content for consistency');
  });

  test('should show save status with proper ARIA attributes', async ({ page }) => {
    await page.goto(baseURL);
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const saveStatus = page.locator('[aria-label*="Save status"]').first();
    await expect(saveStatus).toHaveAttribute('role', 'status');
    await expect(saveStatus).toHaveAttribute('aria-live', 'polite');
  });
});
