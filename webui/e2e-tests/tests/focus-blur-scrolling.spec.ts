import { test, expect } from '@playwright/test';

test.describe.skip('Focus Blur and Viewport Scrolling', () => {
  test('navigating away from draft card blurs textarea immediately', async ({ page }) => {
    await page.goto('/');

    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();

    // Focus the textarea first
    await textarea.click();
    await expect(textarea).toBeFocused();

    // Navigate to a session card using arrow keys
    const sessionCards = page.locator('[data-testid="task-card"]');
    const sessionCount = await sessionCards.count();

    if (sessionCount > 0) {
      // Press ArrowUp to go to session cards (assuming we're starting from draft)
      await page.keyboard.press('ArrowUp');

      // Textarea should immediately lose focus
      await expect(textarea).not.toBeFocused();
    }
  });

  test('navigating to draft card then away blurs textarea', async ({ page }) => {
    await page.goto('/');

    await page.waitForSelector('[data-testid="task-card"], [data-testid="draft-task-card"]');

    const sessionCards = page.locator('[data-testid="task-card"]');
    const draftCards = page.locator('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();

    const sessionCount = await sessionCards.count();
    const draftCount = await draftCards.count();

    if (sessionCount > 0 && draftCount > 0) {
      // Navigate to a session first
      await page.keyboard.press('ArrowDown');

      // Navigate to draft (should focus textarea)
      for (let i = 0; i <= sessionCount; i++) {
        await page.keyboard.press('ArrowDown');
      }

      // Textarea should be focused
      await page.waitForTimeout(100);
      await expect(textarea).toBeFocused();

      // Navigate back to session
      await page.keyboard.press('ArrowUp');

      // Textarea should lose focus immediately
      await expect(textarea).not.toBeFocused();
    }
  });

  test('blur on navigation works in both directions', async ({ page }) => {
    await page.goto('/');

    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();

    // Focus textarea
    await textarea.click();
    await expect(textarea).toBeFocused();

    // Navigate down (should blur)
    await page.keyboard.press('ArrowDown');
    await expect(textarea).not.toBeFocused();

    // Navigate back up to draft (should focus again)
    const sessionCards = page.locator('[data-testid="task-card"]');
    const sessionCount = await sessionCards.count();

    for (let i = 0; i <= sessionCount; i++) {
      await page.keyboard.press('ArrowUp');
    }

    await page.waitForTimeout(100);
    await expect(textarea).toBeFocused();

    // Navigate down again (should blur)
    await page.keyboard.press('ArrowDown');
    await expect(textarea).not.toBeFocused();
  });

  test('viewport scrolling when navigating to session card outside viewport', async ({ page }) => {
    await page.goto('/');

    // Set viewport to small height to force scrolling
    await page.setViewportSize({ width: 1024, height: 400 });

    await page.waitForSelector('[data-testid="task-card"]');
    const sessionCards = page.locator('[data-testid="task-card"]');

    // Get the count of session cards
    const sessionCount = await sessionCards.count();

    if (sessionCount >= 3) {
      // Navigate to the last session card (likely outside viewport)
      for (let i = 0; i < sessionCount - 1; i++) {
        await page.keyboard.press('ArrowDown');
      }

      // Wait for smooth scroll
      await page.waitForTimeout(500);

      // Last session card should be visible
      const lastCard = sessionCards.last();
      const isVisible = await lastCard.isVisible();
      expect(isVisible).toBe(true);

      // Check that it's actually in viewport
      const boundingBox = await lastCard.boundingBox();
      const viewportSize = page.viewportSize();

      if (boundingBox && viewportSize) {
        // Card should be at least partially visible
        const isInViewport = boundingBox.y >= 0 &&
                           boundingBox.y + boundingBox.height <= viewportSize.height;
        expect(isInViewport).toBe(true);
      }
    }
  });

  test('viewport scrolling when navigating to draft card outside viewport', async ({ page }) => {
    await page.goto('/');

    // Set viewport to small height to force scrolling
    await page.setViewportSize({ width: 1024, height: 400 });

    await page.waitForSelector('[data-testid="task-card"], [data-testid="draft-task-card"]');

    const sessionCards = page.locator('[data-testid="task-card"]');
    const draftCards = page.locator('[data-testid="draft-task-card"]');

    const sessionCount = await sessionCards.count();
    const draftCount = await draftCards.count();

    if (sessionCount >= 2 && draftCount >= 1) {
      // Navigate past all session cards to draft
      for (let i = 0; i < sessionCount + draftCount; i++) {
        await page.keyboard.press('ArrowDown');
      }

      // Wait for smooth scroll
      await page.waitForTimeout(500);

      // Draft card should be visible
      const draftCard = draftCards.first();
      const isVisible = await draftCard.isVisible();
      expect(isVisible).toBe(true);

      // Check viewport positioning
      const boundingBox = await draftCard.boundingBox();
      const viewportSize = page.viewportSize();

      if (boundingBox && viewportSize) {
        // Card should be at least partially visible
        const isInViewport = boundingBox.y >= 0 &&
                           boundingBox.y + boundingBox.height <= viewportSize.height;
        expect(isInViewport).toBe(true);
      }
    }
  });

  test('smooth scrolling behavior is applied', async ({ page }) => {
    await page.goto('/');

    await page.waitForSelector('[data-testid="task-card"]');
    const sessionCards = page.locator('[data-testid="task-card"]');

    const sessionCount = await sessionCards.count();

    if (sessionCount >= 2) {
      // Navigate to second card
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('ArrowDown');

      // Wait for smooth scroll animation
      await page.waitForTimeout(300);

      // Card should be selected
      const secondCard = sessionCards.nth(1);
      await expect(secondCard).toHaveClass(/ring-blue-500/);
    }
  });

  test('scrolling works with circular navigation (ArrowUp from first to last)', async ({ page }) => {
    await page.goto('/');

    // Set small viewport to test scrolling
    await page.setViewportSize({ width: 1024, height: 300 });

    await page.waitForSelector('[data-testid="task-card"]');
    const sessionCards = page.locator('[data-testid="task-card"]');

    const sessionCount = await sessionCards.count();

    if (sessionCount >= 3) {
      // Start at first card, navigate up (should go to last card)
      await page.keyboard.press('ArrowUp');

      // Wait for scroll
      await page.waitForTimeout(500);

      // Last card should be selected
      const lastCard = sessionCards.last();
      await expect(lastCard).toHaveClass(/ring-blue-500/);

      // Last card should be visible in viewport
      const isVisible = await lastCard.isVisible();
      expect(isVisible).toBe(true);
    }
  });

  test('blur and scroll work together during navigation', async ({ page }) => {
    await page.goto('/');

    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();

    // Focus textarea
    await textarea.click();
    await expect(textarea).toBeFocused();

    // Navigate to a session card (should blur textarea and scroll)
    await page.keyboard.press('ArrowUp');

    // Textarea should be blurred
    await expect(textarea).not.toBeFocused();

    // A session card should be selected
    const selectedSession = page.locator('[data-testid="task-card"]').filter({
      has: page.locator('.ring-blue-500')
    });
    await expect(selectedSession).toHaveCount(1);
  });

  test('no scrolling needed when card is already in viewport', async ({ page }) => {
    await page.goto('/');

    // Use normal viewport size
    await page.setViewportSize({ width: 1024, height: 768 });

    await page.waitForSelector('[data-testid="task-card"]');
    const sessionCards = page.locator('[data-testid="task-card"]');

    const sessionCount = await sessionCards.count();

    if (sessionCount >= 2) {
      // Navigate to second card (likely still in viewport)
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('ArrowDown');

      // Card should be selected without issues
      const secondCard = sessionCards.nth(1);
      await expect(secondCard).toHaveClass(/ring-blue-500/);

      // Should still be visible
      const isVisible = await secondCard.isVisible();
      expect(isVisible).toBe(true);
    }
  });

  test('blur happens before scroll during navigation', async ({ page }) => {
    await page.goto('/');

    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();

    // Focus textarea
    await textarea.click();
    await expect(textarea).toBeFocused();

    // Start navigation (this should blur immediately)
    await page.keyboard.press('ArrowUp');

    // Immediately check blur (before scroll completes)
    await expect(textarea).not.toBeFocused();

    // Wait for scroll to complete
    await page.waitForTimeout(500);

    // Verify navigation completed
    const selectedSession = page.locator('[data-testid="task-card"]').filter({
      has: page.locator('.ring-blue-500')
    });
    await expect(selectedSession).toHaveCount(1);
  });
});
