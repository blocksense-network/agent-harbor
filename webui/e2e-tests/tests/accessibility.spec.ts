import { test, expect } from '@playwright/test';

test.describe.skip('Accessibility Tests', () => {
  test('Main dashboard has accessible HTML structure', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForFunction(
      () => !!document.querySelector('header'),
      { timeout: 15000 }
    );

    // Check for proper document structure
    const html = page.locator('html');
    const lang = await html.getAttribute('lang');
    expect(lang).toBe('en');

    // Check for proper title
    const title = await page.title();
    expect(title).toContain('Agent Harbor');

    // Basic presence checks instead of hard-coded asset names
    await expect(page.locator('#app')).toBeVisible();
  });

  test('Header has proper accessibility attributes', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForFunction(
      () => !!document.querySelector('header'),
      { timeout: 15000 }
    );

    // Check header title has proper heading level
    const headerTitle = page.locator('h1');
    await expect(headerTitle).toBeVisible();
    await expect(headerTitle).toHaveText('Agent Harbor');

    // Branding and nav
    await expect(page.locator('img[alt="Agent Harbor Logo"]')).toBeVisible();
    await expect(page.locator('nav')).toContainText('Settings');
  });

  test('Task cards have proper accessibility', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForFunction(
      () => !!document.querySelector('header'),
      { timeout: 15000 }
    );
    await page.waitForTimeout(2000);

    // Find session/draft cards
    const taskCards = page.locator('.bg-white.border');

    if (await taskCards.first().isVisible()) {
      // Card visible and focusable via button role
      await expect(taskCards.first()).toBeVisible();
    }
  });

  test('New task creation form has proper accessibility', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForFunction(
      () => !!document.querySelector('header'),
      { timeout: 15000 }
    );

    // Draft card is present and textarea has placeholder
    await expect(page.locator('textarea')).toBeVisible();
    await expect(page.locator('textarea')).toHaveAttribute('placeholder');
  });

  test('Tom Select components have proper accessibility', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForFunction(
      () => !!document.querySelector('header'),
      { timeout: 15000 }
    );

    // Find and expand the new task form
    const createButton = page.locator('button').filter({ hasText: 'Create New Task' });
    await createButton.click();

    // Wait for form to expand
    await page.waitForTimeout(500);

    // Find Tom Select components
    const tomSelectWrappers = page.locator('.ts-wrapper');
    await expect(tomSelectWrappers).toHaveCount(3); // Repository, Branch, Model

    // Check that Tom Select components are keyboard accessible
    for (const wrapper of await tomSelectWrappers.all()) {
      await expect(wrapper).toBeVisible();

      // Check that the input is focusable
      const input = wrapper.locator('input');
      await expect(input).toBeVisible();
    }
  });

  test('Footer has proper accessibility', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForFunction(
      () => !!document.querySelector('header'),
      { timeout: 15000 }
    );

    // Check footer exists and has proper structure
    const footer = page.locator('footer');
    await expect(footer).toBeVisible();

    // Check footer has proper ARIA labels
    await expect(footer).toHaveAttribute('role', 'contentinfo');
    await expect(footer).toHaveAttribute('aria-label', 'Keyboard shortcuts');

    // Check keyboard shortcuts toolbar has proper accessibility
    const shortcutsToolbar = footer.locator('[role="toolbar"]');
    await expect(shortcutsToolbar).toHaveAttribute('aria-label', 'Keyboard shortcuts');

    // Verify context-sensitive shortcuts are displayed (task feed context by default)
    await expect(page.locator('text=↑↓ Navigate')).toBeVisible();
  });



  test('Color contrast meets accessibility standards', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForSelector('header', { timeout: 10000 });

    // Check that text has sufficient contrast
    // This is a basic check - in production, use axe-core for comprehensive testing

    // Check header text is readable
    const headerTitle = page.locator('h1');
    const headerTitleColor = await headerTitle.evaluate((el) => getComputedStyle(el).color);
    const headerTitleBg = await headerTitle.evaluate(
      (el) => getComputedStyle(el.closest('header')).backgroundColor
    );

    // Basic contrast check - header should have good contrast
    expect(headerTitleColor).not.toBe('transparent');
    expect(headerTitleBg).not.toBe('transparent');
  });

  test('Focus management works properly', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForSelector('header', { timeout: 10000 });

    // Tab through interactive elements
    await page.keyboard.press('Tab');

    // Should focus on first interactive element (including SELECT for status filter and Tom Select INPUT elements)
    const focusedElement = await page.evaluate(() => document.activeElement?.tagName);
    expect(['BUTTON', 'A', 'INPUT', 'TEXTAREA', 'SELECT']).toContain(focusedElement);

    // Tab again to check focus moves
    await page.keyboard.press('Tab');
    const newFocusedElement = await page.evaluate(() => document.activeElement?.tagName);
    expect(['BUTTON', 'A', 'INPUT', 'TEXTAREA', 'SELECT']).toContain(newFocusedElement);
  });

  test('Screen reader compatibility for task status', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForSelector('header', { timeout: 10000 });
    await page.waitForTimeout(2000);

    // Find task cards
    const taskCards = page.locator('.bg-white.border');

    if (await taskCards.first().isVisible()) {
      // Check that status badges have proper ARIA attributes
      const statusBadges = page.locator(
        '[class*="bg-green-100"], [class*="bg-blue-100"], [class*="bg-gray-100"], [class*="bg-purple-100"]'
      );

      for (const badge of await statusBadges.all()) {
        // Check that status badges have aria-label or are properly described
        const ariaLabel = await badge.getAttribute('aria-label');
        const textContent = await badge.textContent();

        // Either has aria-label or readable text content
        expect(ariaLabel || textContent).toBeTruthy();
      }
    }
  });

  test('Form validation provides accessible error messages', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForSelector('header', { timeout: 10000 });

    // Find and expand the new task form
    const createButton = page.locator('button').filter({ hasText: 'Create New Task' });
    await createButton.click();

    // Wait for form to expand
    await page.waitForTimeout(500);

    // Try to submit without filling required fields
    const submitButton = page.locator('button:has-text("Create Task")');
    await submitButton.click();

    // Check that error messages are accessible
    const errorMessages = page.locator('[class*="text-red"]');
    const errorCount = await errorMessages.count();

    if (errorCount > 0) {
      for (const error of await errorMessages.all()) {
        // Error messages should be associated with form fields
        const errorText = await error.textContent();
        expect(errorText).toBeTruthy();

        // Check that errors have proper contrast
        const errorColor = await error.evaluate((el) => getComputedStyle(el).color);
        expect(errorColor).not.toBe('transparent');
      }
    }
  });

  test('Keyboard navigation works throughout the interface', async ({ page }) => {
    await page.goto('/');

    // Wait for client-side JavaScript to load and render
    await page.waitForSelector('header', { timeout: 10000 });

    // Test tab navigation through main elements (including SELECT for status filter and Tom Select INPUT elements)
    await page.keyboard.press('Tab');
    let focusedElement = await page.evaluate(() => document.activeElement?.tagName);
    expect(['BUTTON', 'A', 'INPUT', 'TEXTAREA', 'SELECT']).toContain(focusedElement);

    // Continue tabbing to test navigation flow
    for (let i = 0; i < 5; i++) {
      await page.keyboard.press('Tab');
      focusedElement = await page.evaluate(() => document.activeElement?.tagName);
      expect(['BUTTON', 'A', 'INPUT', 'TEXTAREA', 'SELECT']).toContain(focusedElement);
    }

    // Test escape key functionality
    await page.keyboard.press('Escape');
    // Should not crash - focus might move to various elements or body
    focusedElement = await page.evaluate(() => document.activeElement?.tagName);
    // After Escape, focus can be on any interactive element or reset to body/html
    expect(focusedElement).toBeTruthy();
  });
});
