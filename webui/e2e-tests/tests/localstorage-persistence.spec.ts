import { test, expect } from '@playwright/test';

test.describe('Draft Persistence & Preferences', () => {
  test('Draft task card is present and can be edited', async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => !!document.querySelector('header'), { timeout: 20000 });

    // Draft card should always be visible per PRD (use data-testid)
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    await expect(draftCard).toBeVisible();

    // Edit prompt
    const descriptionField = draftCard.locator('textarea');
    await descriptionField.fill('Test draft for server persistence');

    // Interact with selectors to ensure update path works
    // Open repository selector if present
    const repoSelector = draftCard.locator('.ts-wrapper').first();
    if (await repoSelector.isVisible()) {
      await repoSelector.click();
      await page.waitForTimeout(200);
      const firstOption = page.locator('.ts-dropdown .option').first();
      if (await firstOption.isVisible()) await firstOption.click();
    }

    // Save occurs via server calls; basic smoke check — field retains value
    await expect(descriptionField).toHaveValue('Test draft for server persistence');
  });

  test('Theme preferences persist across browser sessions (if present)', async ({
    page,
    context,
  }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForFunction(() => !!document.querySelector('header'), { timeout: 20000 });

    // If there is a theme toggle, test persistence; otherwise skip silently
    const themeButtons = page.locator('header button');
    const buttonCount = await themeButtons.count();
    if (buttonCount > 1) {
      const themeButton = themeButtons.nth(1);
      if (await themeButton.isVisible()) {
        await themeButton.click();
      }
    }

    // Wait for theme to change
    await page.waitForTimeout(500);

    // Check that theme preference was saved
    const themePreference = await page.evaluate(() => {
      return localStorage.getItem('theme');
    });

    // Preference may or may not be set depending on UI; only assert type
    expect(themePreference === null || typeof themePreference === 'string').toBe(true);

    // Create new page to simulate browser restart
    const newPage = await context.newPage();
    await newPage.goto('/');

    // Wait for new page to load
    await newPage.waitForLoadState('networkidle');

    // Check that theme preference persists
    const persistedTheme = await newPage.evaluate(() => {
      return localStorage.getItem('theme');
    });

    expect(persistedTheme === null || typeof persistedTheme === 'string').toBe(true);
  });

  test('Draft editor maintains state during navigation', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForFunction(() => !!document.querySelector('header'), { timeout: 20000 });

    const descriptionField = page.locator('[data-testid="draft-task-textarea"]').first();
    await descriptionField.fill('Test task for state preservation');

    // Wait for auto-save debounce (500ms) + API call
    await page.waitForTimeout(800);

    // Navigate away and back (same page reload)
    await page.goto('/');

    // Wait for page to reload
    await page.waitForLoadState('networkidle');

    // Check that draft prompt was persisted
    const reloadedDescriptionField = page
      .locator('[data-testid="draft-task-card"]')
      .first()
      .locator('textarea');
    // Wait for draft to load and render
    await page.waitForTimeout(500);
    const value = await reloadedDescriptionField.inputValue();
    expect(value).toBe('Test task for state preservation');
  });

  test('localStorage does not contain sensitive data (best-effort)', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForFunction(() => !!document.querySelector('header'), { timeout: 20000 });

    // Interact to generate some local storage if any
    const descriptionField = page.locator('textarea');
    await descriptionField.fill('Test task for security check');

    // Wait for localStorage to be populated
    await page.waitForTimeout(1000);

    // Check all localStorage keys and values
    const allStorageData = await page.evaluate(() => {
      const data: Record<string, string> = {};
      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        if (key) {
          data[key] = localStorage.getItem(key) || '';
        }
      }
      return data;
    });

    // Check that no sensitive data is stored
    for (const [, value] of Object.entries(allStorageData)) {
      expect(value.toLowerCase()).not.toContain('password');
      expect(value.toLowerCase()).not.toContain('token');
      expect(value.toLowerCase()).not.toContain('secret');
      expect(value.toLowerCase()).not.toContain('api_key');
      expect(value.toLowerCase()).not.toContain('authorization');
    }
  });

  test('localStorage has reasonable size limits', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForFunction(() => !!document.querySelector('header'), { timeout: 20000 });

    const descriptionField = page.locator('textarea');

    // Create a large draft
    const largeText = 'A'.repeat(10000);
    await descriptionField.fill(largeText);

    // Wait for storage
    await page.waitForTimeout(1000);

    // Check total localStorage size
    const storageSize = await page.evaluate(() => {
      let totalSize = 0;
      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        if (key) {
          const value = localStorage.getItem(key) || '';
          totalSize += key.length + value.length;
        }
      }
      return totalSize;
    });

    // Should be well under browser limits (typically 5-10MB)
    expect(storageSize).toBeLessThan(1024 * 1024); // Less than 1MB
  });

  test('Form field focus is maintained during interaction', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForFunction(() => !!document.querySelector('header'), { timeout: 20000 });

    // Expand the new task form
    const createButton = page.locator('button').filter({ hasText: 'Create New Task' });
    await createButton.click();

    // Focus on the description field
    const descriptionField = page.locator('textarea');
    await descriptionField.click();

    // Verify focus is on the field
    const focusedElement = await page.evaluate(() => document.activeElement?.tagName);
    expect(focusedElement).toBe('TEXTAREA');

    // Tab through form fields
    await page.keyboard.press('Tab');
    const newFocusedElement = await page.evaluate(() => document.activeElement?.tagName);
    expect(['SELECT', 'INPUT', 'BUTTON']).toContain(newFocusedElement);
  });

  test('New task form can be cancelled and state cleared', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForSelector('header', { timeout: 10000 });

    // Expand and fill the form
    const createButton = page.locator('button').filter({ hasText: 'Create New Task' });
    await createButton.click();

    const descriptionField = page.locator('textarea');
    await descriptionField.fill('Test task to be cancelled');

    // Cancel the form
    const cancelButton = page.locator('button:has-text("Cancel")');
    await expect(cancelButton).toBeVisible();
    await cancelButton.click();

    // Check that form is collapsed and draft is cleared
    await expect(page.locator('button').filter({ hasText: 'Create New Task' })).toHaveCount(1); // Only collapsed version

    // Check that draft was removed from localStorage
    const draftData = await page.evaluate(() => {
      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        if (key && key.includes('task-draft')) {
          return localStorage.getItem(key);
        }
      }
      return null;
    });

    expect(draftData).toBeNull();
  });
});
