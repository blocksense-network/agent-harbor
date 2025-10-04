import { test, expect } from '@playwright/test';

test.describe('Layout and Navigation Tests', () => {
  test('Simplified task-centric layout renders correctly', async ({ page }) => {
    await page.goto('/');

    // Wait for header to be rendered by client-side JavaScript
    await page.waitForFunction(() => !!document.querySelector('header'), { timeout: 20000 });

    // Check header is present and correct
    await expect(page.locator('h1')).toContainText('Agent Harbor');

    // Check main content structure
    await expect(page.locator('.flex.flex-col.h-screen')).toBeVisible();

    // Check task feed area exists
    await expect(page.locator('.flex-1.overflow-y-auto')).toBeVisible();

    // Check footer exists
    await expect(page.locator('footer')).toBeVisible();

    // Check that Agent Harbor logo is visible
    await expect(page.locator('img[alt="Agent Harbor Logo"]')).toBeVisible();
  });

  test('Header contains correct branding and navigation', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and execute
    await page.waitForFunction(() => !!document.querySelector('header h1'), { timeout: 20000 });

    // Check Agent Harbor title
    const title = page.locator('h1');
    await expect(title).toContainText('Agent Harbor');
    await expect(title).toBeVisible();

    // Check navigation links exist (only Settings link in current design)
    await expect(page.locator('nav')).toContainText('Settings');

    // Check that the logo is present
    await expect(page.locator('img[alt="Agent Harbor Logo"]')).toBeVisible();
  });

  test('Task feed loads and displays content', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and execute
    await page.waitForFunction(() => !!document.querySelector('header h1'), { timeout: 20000 });

    // Check that the task feed header is visible (use specific selector to avoid strict mode violation)
    await expect(page.getByRole('heading', { name: 'Task Feed' })).toBeVisible();

    // Check that the status filter is present
    await expect(page.locator('select#status-filter')).toBeVisible();

    // Wait a moment for content to load
    await page.waitForTimeout(2000);

    // Should show either sessions or empty state
    const taskFeed = page.locator('.flex-1.overflow-y-auto');
    await expect(taskFeed).toBeVisible();
  });

  test('Draft task cards are always visible at bottom', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and execute
    await page.waitForFunction(
      () => {
        const app = document.getElementById('app');
        return app && app.innerHTML && app.innerHTML.includes('Agent Harbor');
      },
      { timeout: 15000 }
    );

    // Wait for draft card to load from API (with proper wait, not fixed timeout)
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    await expect(draftCard).toBeVisible({ timeout: 10000 });

    // Check that it has the expected form elements
    await expect(draftCard.locator('textarea')).toBeVisible();
    await expect(draftCard.locator('button').filter({ hasText: 'Go' })).toBeVisible();
  });

  test('Footer displays context-sensitive keyboard shortcuts', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render content
    await page.waitForFunction(() => !!document.querySelector('footer'), { timeout: 20000 });

    // Check footer exists
    const footer = page.locator('footer');
    await expect(footer).toBeVisible();

    // Verify footer shows task feed context shortcuts (default view)
    // When on task feed, should show navigation shortcuts
    await expect(footer).toContainText('Navigate');
    await expect(footer).toContainText('Go');

    // Focus on draft textarea to trigger draft-task context
    const draftTextarea = page.locator('[data-testid="draft-task-textarea"]').first();
    await draftTextarea.click();
    
    // Footer should update to show draft-specific shortcuts
    await expect(footer).toContainText('Launch');
    await expect(footer).toContainText('New Line');
  });

  test('Layout is responsive and adapts to different screen sizes', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and execute
    await page.waitForFunction(() => !!document.querySelector('header'), { timeout: 20000 });

    // Test desktop layout
    await page.setViewportSize({ width: 1200, height: 800 });
    await expect(page.locator('.flex.flex-col.h-screen')).toBeVisible();

    // Test mobile layout
    await page.setViewportSize({ width: 375, height: 667 });
    await expect(page.locator('.flex.flex-col.h-screen')).toBeVisible();

    // Wait for draft card to load from API and verify it's visible on mobile
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    await expect(draftCard).toBeVisible({ timeout: 10000 });
  });

  test('Session cards are displayed in task feed', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and execute
    await page.waitForFunction(() => !!document.querySelector('header'), { timeout: 20000 });

    // Wait for sessions to load
    await page.waitForTimeout(2000);

    // Check that session cards are displayed (using data-testid for robustness)
    const sessionCards = page.locator('[data-testid="task-card"]');
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();

    // Should have at least the draft card
    await expect(draftCard).toBeVisible();

    // If there are session cards (beyond the draft card), they should be visible
    const totalCards = await sessionCards.count();
    if (totalCards > 1) {
      // There are session cards - they should be displayed properly
      const firstSessionCard = sessionCards.first();
      await expect(firstSessionCard).toBeVisible();
    }
  });

  test.skip('Task details page navigation - not yet implemented', async ({ page: _page }) => {
    // Task details pages are not yet implemented in the current UI
    // The current UI shows sessions in a feed format, not individual task detail pages
    expect(true).toBe(true);
  });

  test.skip('Task details page action buttons - not yet implemented', async ({ page: _page }) => {
    // Task details pages with action buttons are not yet implemented
    expect(true).toBe(true);
  });

  test.skip('Navigation maintains browser history - not yet implemented', async ({ page: _page }) => {
    // Task details page navigation is not yet implemented
    expect(true).toBe(true);
  });

  test('Content loads properly with draft tasks always visible', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and execute
    await page.waitForFunction(() => !!document.querySelector('header'), { timeout: 20000 });

    // Wait for content to load
    await page.waitForTimeout(2000);

    // Draft task card should always be visible (use data-testid)
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    await expect(draftCard).toBeVisible();

    // Task feed heading should be visible (use semantic selector)
    await expect(page.getByRole('heading', { name: 'Task Feed' })).toBeVisible();
  });

  test('Session cards display status information', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and execute
    await page.waitForFunction(() => !!document.querySelector('header'), { timeout: 20000 });

    // Wait for sessions to load
    await page.waitForTimeout(2000);

    // Find session cards (excluding draft cards)
    const sessionCards = page.locator('.bg-white.border').filter({ hasNotText: 'Describe what you want the agent to do' });

    if (await sessionCards.first().isVisible()) {
      // Check that session cards are displayed properly
      const firstCard = sessionCards.first();
      await expect(firstCard).toBeVisible();

      // Cards should have some text content
      const cardText = await firstCard.textContent();
      expect(cardText && cardText.length > 0).toBe(true);
    }
  });

  test('Status filter works correctly', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and execute
    await page.waitForFunction(
      () => {
        const app = document.getElementById('app');
        return app && app.innerHTML && app.innerHTML.includes('Agent Harbor');
      },
      { timeout: 15000 }
    );

    // Check that status filter select is present (use specific ID to avoid Tom Select selects)
    const statusFilter = page.locator('#status-filter');
    await expect(statusFilter).toBeVisible();

    // Check that it has expected options (scope to status filter)
    await expect(statusFilter.locator('option[value=""]')).toContainText('All Sessions');
    await expect(statusFilter.locator('option[value="running"]')).toContainText('Running');
    await expect(statusFilter.locator('option[value="completed"]')).toContainText('Completed');
  });
});
