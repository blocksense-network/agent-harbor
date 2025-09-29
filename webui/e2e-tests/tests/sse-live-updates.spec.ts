import { test, expect } from '@playwright/test';

test.describe.skip('SSE Live Updates', () => {
  test('Active session cards subscribe to SSE and display live updates', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('load'); // Use 'load' instead of 'networkidle' due to persistent SSE connections

    // Find an active session card - status "running" is in aria-label attribute, not text content
    const activeCard = page.locator('[data-testid="task-card"]:has([aria-label="Status: running"])').first();
    await expect(activeCard).toBeVisible();

    // Wait a moment for SSE to connect and send events
    await page.waitForTimeout(3000);

    // Check for live activity display
    const liveActivity = activeCard.locator('text=/Thoughts:|File edits:|Tool usage:/').first();
    await expect(liveActivity).toBeVisible({ timeout: 5000 });

    // Verify the content updates (SSE events arrive every 2 seconds)
    const initialText = await liveActivity.textContent();
    
    // Wait for another SSE event
    await page.waitForTimeout(3000);
    
    // Check if content might have changed (SSE updates are dynamic)
    // Note: We can't guarantee change, but we can verify structure
    const hasThinking = await activeCard.locator('text=/Thoughts:/').isVisible();
    const hasFileEdit = await activeCard.locator('text=/File edits:/').isVisible();
    const hasToolUsage = await activeCard.locator('text=/Tool usage:/').isVisible();
    
    // At least one type of live activity should be present
    expect(hasThinking || hasFileEdit || hasToolUsage).toBeTruthy();
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
    const completedCard = page.locator('[data-testid="task-card"]:has([aria-label="Status: completed"])').first();
    
    if (await completedCard.isVisible()) {
      // Completed sessions should show timestamp, not live activity
      const timestamp = completedCard.locator('time');
      await expect(timestamp).toBeVisible();
      
      // Should NOT have "Thoughts:", "File edits:", or "Tool usage:"
      const hasLiveActivity = await completedCard.locator('text=/Thoughts:|File edits:|Tool usage:/').count();
      expect(hasLiveActivity).toBe(0);
    }
  });

  test('SSE connection logs are visible in console', async ({ page }) => {
    const consoleMessages: string[] = [];
    page.on('console', msg => consoleMessages.push(msg.text()));

    await page.goto('/');
    await page.waitForLoadState('load');
    
    // Wait for SSE subscriptions
    await page.waitForTimeout(2000);

    // Check for SSE subscription logs
    const hasSubscriptionLog = consoleMessages.some(msg => 
      msg.includes('[SessionCard] Subscribing to SSE')
    );
    
    expect(hasSubscriptionLog).toBeTruthy();
  });

  test('SSE handles multiple active sessions simultaneously', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('load');

    // Count active sessions - check aria-labels for active statuses
    const runningCards = page.locator('[data-testid="task-card"]:has([aria-label="Status: running"])');
    const queuedCards = page.locator('[data-testid="task-card"]:has([aria-label="Status: queued"])');
    const provisioningCards = page.locator('[data-testid="task-card"]:has([aria-label="Status: provisioning"])');
    
    const activeCards = page.locator('[data-testid="task-card"]').filter(async (card) => {
      const statusEl = card.locator('[aria-label^="Status:"]');
      const label = await statusEl.getAttribute('aria-label');
      return label?.includes('running') || label?.includes('queued') || label?.includes('provisioning') || false;
    });
    
    const activeCount = await activeCards.count();
    
    if (activeCount >= 2) {
      // Wait for SSE events
      await page.waitForTimeout(3000);
      
      // Check that multiple cards show live activity
      for (let i = 0; i < Math.min(activeCount, 2); i++) {
        const card = activeCards.nth(i);
        const hasActivity = await card.locator('text=/Thoughts:|File edits:|Tool usage:|Waiting for agent activity/i').count();
        expect(hasActivity).toBeGreaterThan(0);
      }
    }
  });

  test('No JavaScript errors from SSE connections', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));

    await page.goto('/');
    await page.waitForLoadState('load');
    
    // Wait for SSE subscriptions and events
    await page.waitForTimeout(5000);

    // Filter out expected/harmless errors
    const criticalErrors = errors.filter(err => 
      !err.includes('Failed to parse SSE event') && // Expected if malformed event
      !err.includes('SSE connection error') // Expected if server closes
    );
    
    expect(criticalErrors).toHaveLength(0);
  });
});
