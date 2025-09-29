import { test, expect } from '@playwright/test';

test.describe.skip('Draft Card Keyboard Navigation', () => {
  test('arrow keys navigate to draft cards', async ({ page }) => {
    await page.goto('/');
    
    // Wait for content to load
    await page.waitForSelector('[data-testid="task-card"], [data-testid="draft-task-card"]');
    
    const allCards = page.locator('[data-testid="task-card"], [data-testid="draft-task-card"]');
    const totalCards = await allCards.count();
    
    expect(totalCards).toBeGreaterThan(0);
    
    // Press ArrowDown multiple times to navigate through all cards
    for (let i = 0; i < totalCards; i++) {
      await page.keyboard.press('ArrowDown');
      
      // One card should be selected (blue border + blue background)
      const selectedCards = page.locator('[data-testid="task-card"].ring-2.ring-blue-500, [data-testid="draft-task-card"]').filter({
        has: page.locator('.border-blue-500')
      });
      
      await expect(selectedCards).toHaveCount(1);
    }
  });

  test('draft card receives blue border when keyboard-selected', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="draft-task-card"]');
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    
    // Navigate to draft card using arrow keys
    // First, count how many session cards exist
    const sessionCards = page.locator('[data-testid="task-card"]');
    const sessionCount = await sessionCards.count();
    
    // Press ArrowDown enough times to reach first draft
    for (let i = 0; i <= sessionCount; i++) {
      await page.keyboard.press('ArrowDown');
    }
    
    // Draft card should have blue border
    await expect(draftCard).toHaveClass(/border-blue-500/);
    await expect(draftCard).toHaveClass(/border-2/);
  });

  test('draft card receives blue background when keyboard-selected', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="draft-task-card"]');
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    
    // Navigate to draft card
    const sessionCards = page.locator('[data-testid="task-card"]');
    const sessionCount = await sessionCards.count();
    
    for (let i = 0; i <= sessionCount; i++) {
      await page.keyboard.press('ArrowDown');
    }
    
    // Draft card should have blue background
    await expect(draftCard).toHaveClass(/bg-blue-50/);
  });

  test('draft textarea auto-focuses when card is keyboard-selected', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    
    // Navigate to draft card
    const sessionCards = page.locator('[data-testid="task-card"]');
    const sessionCount = await sessionCards.count();
    
    for (let i = 0; i <= sessionCount; i++) {
      await page.keyboard.press('ArrowDown');
    }
    
    // Wait a bit for the focus effect to run
    await page.waitForTimeout(100);
    
    // Textarea should be focused
    await expect(textarea).toBeFocused();
  });

  test('user can immediately start typing in selected draft card', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    
    // Clear any existing text
    await textarea.clear();
    
    // Navigate to draft card
    const sessionCards = page.locator('[data-testid="task-card"]');
    const sessionCount = await sessionCards.count();
    
    for (let i = 0; i <= sessionCount; i++) {
      await page.keyboard.press('ArrowDown');
    }
    
    // Wait for focus
    await page.waitForTimeout(100);
    
    // Start typing immediately
    await page.keyboard.type('Test task description');
    
    // Text should appear in textarea
    await expect(textarea).toHaveValue(/Test task description/);
  });

  test('Enter key submits draft task', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    
    // Enter task description
    await textarea.fill('Deploy new feature to production');
    
    // Select repository and model (required for submission)
    const repoSelect = page.locator('[data-testid="repository-select"]').first();
    const modelSelect = page.locator('[data-testid="model-multi-select"]').first();
    
    // Set required fields if they exist
    if (await repoSelect.isVisible()) {
      await repoSelect.selectOption({ index: 0 });
    }
    
    // Focus textarea and press Enter
    await textarea.focus();
    await page.keyboard.press('Enter');
    
    // Task should be submitted (draft card should be removed or updated)
    // Wait for API call to complete
    await page.waitForTimeout(1000);
    
    // Either the draft is removed, or a new session appears
    const sessions = page.locator('[data-testid="task-card"]');
    const sessionCount = await sessions.count();
    expect(sessionCount).toBeGreaterThan(0);
  });

  test('Shift+Enter creates new line in draft textarea', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="draft-task-card"]');
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    
    await textarea.fill('First line');
    await textarea.focus();
    
    // Press Shift+Enter to create new line
    await page.keyboard.press('Shift+Enter');
    await page.keyboard.type('Second line');
    
    // Should contain newline
    const value = await textarea.inputValue();
    expect(value).toContain('\n');
    expect(value).toContain('First line');
    expect(value).toContain('Second line');
  });

  test('arrow keys navigate between session and draft cards', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="task-card"], [data-testid="draft-task-card"]');
    
    const sessionCards = page.locator('[data-testid="task-card"]');
    const draftCards = page.locator('[data-testid="draft-task-card"]');
    
    const sessionCount = await sessionCards.count();
    const draftCount = await draftCards.count();
    
    expect(sessionCount + draftCount).toBeGreaterThan(1);
    
    // Navigate down from first session to draft
    for (let i = 0; i < sessionCount; i++) {
      await page.keyboard.press('ArrowDown');
    }
    
    // Should now be on first draft
    const firstDraft = draftCards.first();
    await expect(firstDraft).toHaveClass(/border-blue-500/);
    
    // Navigate back up
    await page.keyboard.press('ArrowUp');
    
    // Should be on last session
    const lastSession = sessionCards.nth(sessionCount - 1);
    await expect(lastSession).toHaveClass(/ring-blue-500/);
  });

  test('Enter key on session card navigates to details page', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="task-card"]');
    
    const firstSession = page.locator('[data-testid="task-card"]').first();
    const sessionId = await firstSession.getAttribute('data-task-id');
    
    // Navigate to first session
    await page.keyboard.press('ArrowDown');
    
    // Press Enter
    await page.keyboard.press('Enter');
    
    // Should navigate to task details page
    await page.waitForURL(`**/tasks/${sessionId}`);
    expect(page.url()).toContain(`/tasks/${sessionId}`);
  });

  test('keyboard selection state persists during typing', async ({ page }) => {
    await page.goto('/');
    
    await page.waitForSelector('[data-testid="draft-task-card"]');
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const textarea = page.locator('[data-testid="draft-task-textarea"]').first();
    
    // Navigate to draft
    const sessionCards = page.locator('[data-testid="task-card"]');
    const sessionCount = await sessionCards.count();
    
    for (let i = 0; i <= sessionCount; i++) {
      await page.keyboard.press('ArrowDown');
    }
    
    // Should be selected
    await expect(draftCard).toHaveClass(/border-blue-500/);
    
    // Type some text
    await page.keyboard.type('Testing selection state');
    
    // Should still be selected
    await expect(draftCard).toHaveClass(/border-blue-500/);
    await expect(draftCard).toHaveClass(/bg-blue-50/);
  });
});
