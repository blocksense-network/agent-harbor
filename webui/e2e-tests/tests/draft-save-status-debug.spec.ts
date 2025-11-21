/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { test, expect } from '@playwright/test';

test.describe('Draft Save Status Debug Tests', () => {
  test('debug - check initial state and component structure', async ({ page }) => {
    await page.goto('/');
    await page.waitForSelector('[data-testid="draft-task-card"]');

    // Check that we have the draft card
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    await expect(draftCard).toBeVisible();

    // Check textarea exists
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    await expect(textarea).toBeVisible();

    // Check save status exists
    const saveStatus = page.locator('[aria-label*="Save status"]').first();
    await expect(saveStatus).toBeVisible();

    // Log initial state
    const initialStatusText = await saveStatus.textContent();
    const initialTextareaValue = await textarea.inputValue();
    console.log(
      `Initial status: "${initialStatusText}", textarea value: "${initialTextareaValue}"`,
    );

    // Check initial visual styling
    await expect(saveStatus).toHaveClass(/text-green-600/);
    await expect(saveStatus).toHaveClass(/bg-green-50/);
  });

  test('debug - monitor status changes during typing', async ({ page }) => {
    await page.goto('/');
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const saveStatus = page.locator('[aria-label*="Save status"]').first();

    // Initial state
    await expect(saveStatus).toHaveText('Saved');

    // Start typing and monitor changes
    await textarea.click();

    // Type one character at a time and check status
    for (let i = 0; i < 5; i++) {
      await textarea.type('a');
      const currentStatus = await saveStatus.textContent();
      const currentClasses = await saveStatus.getAttribute('class');
      console.log(
        `After typing char ${i + 1}: status="${currentStatus}", classes="${currentClasses}"`,
      );
      await page.waitForTimeout(50); // Small delay to see changes
    }

    // Check final state
    const finalStatus = await saveStatus.textContent();
    console.log(`Final status after typing: "${finalStatus}"`);
    await expect(saveStatus).toHaveText('Unsaved');
  });

  test('debug - check save status visual changes', async ({ page }) => {
    await page.goto('/');
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const saveStatus = page.locator('[aria-label*="Save status"]').first();

    // Initial state should be green "Saved"
    await expect(saveStatus).toHaveClass(/text-green-600/);
    await expect(saveStatus).toHaveClass(/bg-green-50/);
    await expect(saveStatus).toHaveText('Saved');

    // Type something
    await textarea.click();
    await textarea.fill('Test content for visual check');

    // Should change to gray "Unsaved"
    await expect(saveStatus).toHaveClass(/text-gray-500/);
    await expect(saveStatus).toHaveClass(/bg-gray-50/);
    await expect(saveStatus).toHaveText('Unsaved');

    // Wait for save to start
    await page.waitForTimeout(600);

    // Should change to orange "Saving..."
    await expect(saveStatus).toHaveClass(/text-orange-600/);
    await expect(saveStatus).toHaveClass(/bg-orange-50/);
    await expect(saveStatus).toHaveText('Saving...');

    // Wait for save to complete
    await page.waitForTimeout(1000);

    // Should change back to green "Saved"
    await expect(saveStatus).toHaveClass(/text-green-600/);
    await expect(saveStatus).toHaveClass(/bg-green-50/);
    await expect(saveStatus).toHaveText('Saved');
  });

  test('debug - focus behavior during typing', async ({ page }) => {
    await page.goto('/');
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();

    // Click to focus
    await textarea.click();
    await expect(textarea).toBeFocused();

    // Type and check focus is maintained
    for (let i = 0; i < 10; i++) {
      await textarea.type('x');
      await expect(textarea).toBeFocused(); // Should still be focused
      await page.waitForTimeout(50);
    }

    // Check final value
    const finalValue = await textarea.inputValue();
    expect(finalValue).toBe('xxxxxxxxxx');
    await expect(textarea).toBeFocused();
  });

  test('debug - rapid typing and status transitions', async ({ page }) => {
    await page.goto('/');
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const saveStatus = page.locator('[aria-label*="Save status"]').first();

    // Monitor status changes during rapid typing
    const statusChanges: string[] = [];

    // Set up monitoring
    const checkStatus = async () => {
      const status = await saveStatus.textContent();
      statusChanges.push(status || '');
    };

    await textarea.click();

    // Type rapidly and check status periodically
    for (let i = 0; i < 20; i++) {
      await textarea.type('x');
      if (i % 5 === 0) {
        // Check every 5 characters
        await checkStatus();
      }
      await page.waitForTimeout(10);
    }

    // Wait for final save
    await page.waitForTimeout(1500);
    await checkStatus();

    console.log('Status changes during rapid typing:', statusChanges);

    // Should end with "Saved"
    await expect(saveStatus).toHaveText('Saved');
    expect(statusChanges.length).toBeGreaterThan(0);
  });

  test('debug - check for console errors during save operations', async ({ page }) => {
    const consoleMessages: string[] = [];
    const errors: string[] = [];

    page.on('console', msg => {
      const text = msg.text();
      consoleMessages.push(text);
      console.log('CONSOLE:', text);
    });

    page.on('pageerror', error => {
      errors.push(error.message);
      console.log('PAGE ERROR:', error.message);
    });

    await page.goto('/');
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const saveStatus = page.locator('[aria-label*="Save status"]').first();

    // Perform save operations
    await textarea.click();
    await textarea.fill('Test content');
    await page.waitForTimeout(2000); // Wait for save cycle

    // Check for errors
    if (errors.length > 0) {
      console.log('JavaScript errors detected:', errors);
      throw new Error(`JavaScript errors: ${errors.join(', ')}`);
    }

    // Check for relevant console messages
    const saveRelatedMessages = consoleMessages.filter(
      msg => msg.includes('save') || msg.includes('Save') || msg.includes('update'),
    );

    console.log('Save-related console messages:', saveRelatedMessages);

    // Should have completed successfully
    await expect(saveStatus).toHaveText('Saved');
  });

  test('debug - check DOM structure and reactivity', async ({ page }) => {
    await page.goto('/');
    await page.waitForSelector('[data-testid="draft-task-card"]');

    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    const saveStatus = page.locator('[aria-label*="Save status"]').first();

    // Check initial DOM structure
    const initialHtml = await saveStatus.innerHTML();
    console.log('Initial save status HTML:', initialHtml);

    // Type and check DOM changes
    await textarea.click();
    await textarea.fill('test');

    // Wait a bit for reactivity
    await page.waitForTimeout(100);

    const afterTypingHtml = await saveStatus.innerHTML();
    console.log('Save status HTML after typing:', afterTypingHtml);

    // Check if the DOM actually changed
    expect(initialHtml).not.toBe(afterTypingHtml);

    // Should show "Unsaved"
    await expect(saveStatus).toHaveText('Unsaved');
  });
});
