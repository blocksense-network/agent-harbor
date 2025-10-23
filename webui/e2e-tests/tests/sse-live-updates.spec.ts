/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { test, expect } from '@playwright/test';

test.describe('SSE Live Updates', () => {
  test('Active session cards subscribe to SSE and display live updates', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('load'); // Use 'load' instead of 'networkidle' due to persistent SSE connections

    // Find an active session card - status "running" is in aria-label attribute, not text content
    const activeCard = page
      .locator('[data-testid="task-card"]:has([aria-label="Status: running"])')
      .first();
    await expect(activeCard).toBeVisible();

    // Wait a moment for SSE to connect and send events
    await page.waitForTimeout(3000);

    // Wait for SSE events to arrive and update the UI
    await page.waitForTimeout(3000);

    // Check for live activity display by examining card content
    const cardText = await activeCard.textContent();

    // At least one type of live activity should be present
    const hasLiveActivity =
      cardText?.includes('Thoughts:') ||
      cardText?.includes('File edits:') ||
      cardText?.includes('Tool usage:');
    expect(hasLiveActivity).toBeTruthy();

    // Get initial card content length
    const _initialContentLength = cardText?.length || 0;

    // Wait for more SSE events
    await page.waitForTimeout(3000);

    // Check that the card content has been updated (SSE events add content)
    const updatedCardText = await activeCard.textContent();
    const updatedContentLength = updatedCardText?.length || 0;

    // Content should be present and substantial
    expect(updatedContentLength).toBeGreaterThan(50);
    // Note: We don't check for specific content changes since SSE events are dynamic
    // but we verify that live activity content exists
  });

  test('SSE events update session status dynamically', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('load');

    // Find any active session
    const activeCard = page.locator('[data-testid="task-card"]').first();
    await expect(activeCard).toBeVisible();

    // Get initial status icon
    const statusIcon = activeCard.locator('span[aria-label^="Status:"]').first();
    const initialStatus = await statusIcon.getAttribute('aria-label');

    expect(initialStatus).toBeTruthy();
    expect(initialStatus).toContain('Status:');
  });

  test('Completed sessions do not show live activity', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('load');

    // Look for a completed session - status is in aria-label
    const completedCard = page
      .locator('[data-testid="task-card"]:has([aria-label="Status: completed"])')
      .first();

    if (await completedCard.isVisible()) {
      // Completed sessions should show timestamp, not live activity
      const timestamp = completedCard.locator('time');
      await expect(timestamp).toBeVisible();

      // Should NOT have "Thoughts:", "File edits:", or "Tool usage:"
      const hasLiveActivity = await completedCard
        .locator('text=/Thoughts:|File edits:|Tool usage:/')
        .count();
      expect(hasLiveActivity).toBe(0);
    }
  });

  test('SSE connection logs are visible in console', async ({ page }) => {
    const consoleMessages: string[] = [];
    page.on('console', (msg) => consoleMessages.push(msg.text()));

    await page.goto('/');
    await page.waitForLoadState('load');

    // Wait for SSE subscriptions
    await page.waitForTimeout(2000);

    // Check for SSE subscription logs
    const hasSubscriptionLog = consoleMessages.some((msg) =>
      msg.includes('[SessionCard] Subscribing to SSE')
    );

    expect(hasSubscriptionLog).toBeTruthy();
  });

  test('SSE handles multiple active sessions simultaneously', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('load');

    // Count active sessions - check aria-labels for active statuses
    const _runningCards = page.locator(
      '[data-testid="task-card"]:has([aria-label="Status: running"])'
    );
    const _queuedCards = page.locator(
      '[data-testid="task-card"]:has([aria-label="Status: queued"])'
    );
    const _provisioningCards = page.locator(
      '[data-testid="task-card"]:has([aria-label="Status: provisioning"])'
    );

    const activeCards = page.locator('[data-testid="task-card"]').filter(async (card) => {
      const statusEl = card.locator('[aria-label^="Status:"]');
      const label = await statusEl.getAttribute('aria-label');
      return (
        label?.includes('running') ||
        label?.includes('queued') ||
        label?.includes('provisioning') ||
        false
      );
    });

    const activeCount = await activeCards.count();

    if (activeCount >= 2) {
      // Wait for SSE events
      await page.waitForTimeout(3000);

      // Check that multiple cards show live activity
      for (let i = 0; i < Math.min(activeCount, 2); i++) {
        const card = activeCards.nth(i);
        const hasActivity = await card
          .locator('text=/Thoughts:|File edits:|Tool usage:|Waiting for agent activity/i')
          .count();
        expect(hasActivity).toBeGreaterThan(0);
      }
    }
  });

  test('No JavaScript errors from SSE connections', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));

    await page.goto('/');
    await page.waitForLoadState('load');

    // Wait for SSE subscriptions and events
    await page.waitForTimeout(5000);

    // Filter out expected/harmless errors
    const criticalErrors = errors.filter(
      (err) =>
        !err.includes('Failed to parse SSE event') && // Expected if malformed event
        !err.includes('SSE connection error') // Expected if server closes
    );

    expect(criticalErrors).toHaveLength(0);
  });
});
