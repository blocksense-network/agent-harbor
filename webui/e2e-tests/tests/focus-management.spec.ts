import { test, expect } from '@playwright/test';

test.describe.skip('Focus Management and Dynamic Shortcuts', () => {
  test('selecting session card removes focus from draft textarea', async ({ page }) => {
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
      // Press ArrowUp to go to session cards
      await page.keyboard.press('ArrowUp');
      
      // Textarea should no longer be focused
      await expect(textarea).not.toBeFocused();
      
      // Session card should be selected (blue border)
      const selectedSession = sessionCards.first();
      await expect(selectedSession).toHaveClass(/ring-blue-500/);
    }
  });

  test('selecting draft card focuses its textarea', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    
    // Navigate to draft card using arrow keys
    const sessionCards = page.locator('[data-testid="task-card"]');
    const sessionCount = await sessionCards.count();
    
    // Press ArrowDown enough times to reach first draft
    for (let i = 0; i <= sessionCount; i++) {
      await page.keyboard.press('ArrowDown');
    }
    
    // Wait for focus effect
    await page.waitForTimeout(100);
    
    // Textarea should be focused
    await expect(textarea).toBeFocused();
    
    // Draft card should be selected (blue border)
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    await expect(draftCard).toHaveClass(/border-blue-500/);
  });

  test('footer shows "Launch Agent" when draft textarea is focused', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    
    // Focus the textarea
    await textarea.click();
    
    // Footer should show "Launch Agent" shortcut
    const enterShortcut = page.locator('footer').locator('text=/Enter.*Launch Agent/');
    await expect(enterShortcut).toBeVisible();
  });

  test('footer shows "Review Session Details" when session card is selected', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="task-card"]');
    
    // Navigate to first session card
    await page.keyboard.press('ArrowDown');
    
    // Footer should show "Review Session Details" shortcut
    const enterShortcut = page.locator('footer').locator('text=/Enter.*Review Session Details/');
    await expect(enterShortcut).toBeVisible();
  });

  test('footer shows "New Task" when no specific focus', async ({ page }) => {
    await page.goto('/');
    
    // Click somewhere neutral (not on cards or textarea)
    await page.click('body');
    
    // Footer should show "New Task" shortcut
    const enterShortcut = page.locator('footer').locator('text=/Enter.*New Task/');
    await expect(enterShortcut).toBeVisible();
  });

  test('shortcuts update dynamically as focus changes', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="task-card"], [data-testid="draft-task-card"]');
    
    // Start with neutral focus
    await page.click('body');
    await expect(page.locator('footer').locator('text=/Enter.*New Task/')).toBeVisible();
    
    // Navigate to session card
    await page.keyboard.press('ArrowDown');
    await expect(page.locator('footer').locator('text=/Enter.*Review Session Details/')).toBeVisible();
    
    // Navigate to draft card
    const sessionCards = page.locator('[data-testid="task-card"]');
    const sessionCount = await sessionCards.count();
    
    for (let i = 0; i <= sessionCount; i++) {
      await page.keyboard.press('ArrowDown');
    }
    
    await page.waitForTimeout(100); // Wait for focus
    await expect(page.locator('footer').locator('text=/Enter.*Launch Agent/')).toBeVisible();
  });

  test('multiple agents show "Launch Agents" (plural)', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    
    // Focus the textarea
    await textarea.click();
    
    // Check if multiple agents are selected (this depends on mock data)
    // For now, just verify the shortcut is visible
    const launchShortcut = page.locator('footer').locator('text=/Enter.*Launch Agent/');
    await expect(launchShortcut).toBeVisible();
    
    // Note: Testing plural form would require setting up multiple agents in the draft
    // This would need to be done through the ModelMultiSelect component
  });

  test('focus state persists during typing in draft', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    
    // Focus the textarea
    await textarea.click();
    
    // Verify "Launch Agent" shortcut is shown
    await expect(page.locator('footer').locator('text=/Enter.*Launch Agent/')).toBeVisible();
    
    // Type some text
    await page.keyboard.type('Test task description');
    
    // Shortcut should still be "Launch Agent"
    await expect(page.locator('footer').locator('text=/Enter.*Launch Agent/')).toBeVisible();
    
    // Textarea should still be focused
    await expect(textarea).toBeFocused();
  });

  test('clicking outside removes focus and resets shortcuts', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    
    // Focus the textarea
    await textarea.click();
    await expect(page.locator('footer').locator('text=/Enter.*Launch Agent/')).toBeVisible();
    
    // Click outside (on body)
    await page.click('body');
    
    // Should show default shortcuts
    await expect(page.locator('footer').locator('text=/Enter.*New Task/')).toBeVisible();
  });

  test('keyboard navigation maintains correct focus state', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="task-card"], [data-testid="draft-task-card"]');
    
    const sessionCards = page.locator('[data-testid="task-card"]');
    const draftCards = page.locator('[data-testid="draft-task-card"]');
    
    const sessionCount = await sessionCards.count();
    const draftCount = await draftCards.count();
    
    // Test navigating through all cards
    for (let i = 0; i < sessionCount + draftCount; i++) {
      await page.keyboard.press('ArrowDown');
      
      // Verify appropriate shortcuts are shown
      if (i < sessionCount) {
        // On session card
        await expect(page.locator('footer').locator('text=/Enter.*Review Session Details/')).toBeVisible();
      } else {
        // On draft card
        await page.waitForTimeout(100); // Wait for focus
        await expect(page.locator('footer').locator('text=/Enter.*Launch Agent/')).toBeVisible();
      }
    }
  });

  test('focus management works with multiple draft cards', async ({ page }) => {
    await page.goto('/');
    
    // This test assumes there might be multiple draft cards
    // If there's only one, we'll test the single case
    const draftCards = page.locator('[data-testid="draft-task-card"]');
    const draftCount = await draftCards.count();
    
    if (draftCount > 1) {
      const firstTextarea = page.locator('[data-testid="draft-task-textarea"]').first();
      const secondTextarea = page.locator('[data-testid="draft-task-textarea"]').nth(1);
      
      // Focus first textarea
      await firstTextarea.click();
      await expect(firstTextarea).toBeFocused();
      await expect(page.locator('footer').locator('text=/Enter.*Launch Agent/')).toBeVisible();
      
      // Focus second textarea
      await secondTextarea.click();
      await expect(secondTextarea).toBeFocused();
      await expect(firstTextarea).not.toBeFocused();
      await expect(page.locator('footer').locator('text=/Enter.*Launch Agent/')).toBeVisible();
    } else {
      // Test single draft card
      const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
      await textarea.click();
      await expect(textarea).toBeFocused();
      await expect(page.locator('footer').locator('text=/Enter.*Launch Agent/')).toBeVisible();
    }
  });

  test('Enter key behavior matches displayed shortcut', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="task-card"]');
    
    // Navigate to session card
    await page.keyboard.press('ArrowDown');
    
    // Verify shortcut shows "Review Session Details"
    await expect(page.locator('footer').locator('text=/Enter.*Review Session Details/')).toBeVisible();
    
    // Press Enter - should navigate to task details
    const firstSession = page.locator('[data-testid="task-card"]').first();
    const sessionId = await firstSession.getAttribute('data-task-id');
    
    await page.keyboard.press('Enter');
    
    // Should navigate to task details page
    await page.waitForURL(`**/tasks/${sessionId}`);
    expect(page.url()).toContain(`/tasks/${sessionId}`);
  });
});
