import { test, expect } from '@playwright/test';

test.describe.skip('TOM Select Customization', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('load'); // Use 'load' instead of 'networkidle' due to persistent SSE connections
  });

  test('Model selector has custom TOM Select dropdown with +/- buttons', async ({ page }) => {
    // Find the draft task card
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    await expect(draftCard).toBeVisible();

    // Find the model selector
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');
    await expect(modelSelector).toBeVisible();
    
    // Click the TOM Select control input area to open dropdown
    const tomSelectInput = modelSelector.locator('.ts-control input');
    await tomSelectInput.click();
    
    // Wait for dropdown to appear (Tom Select appends it to body and makes it visible)
    // Use class that indicates visibility, not just presence
    const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
    await expect(dropdown).toBeVisible({ timeout: 3000 });

    // Check for options with +/- buttons
    const option = dropdown.locator('[role="option"]').first();
    await expect(option).toBeVisible();
    
    // Verify +/- buttons are present in the option
    const decreaseBtn = option.locator('button.ts-count-minus');
    const increaseBtn = option.locator('button.ts-count-plus');
    const countDisplay = option.locator('.ts-count-display');
    
    await expect(decreaseBtn).toBeVisible();
    await expect(increaseBtn).toBeVisible();
    await expect(countDisplay).toBeVisible();
  });

  test('Dropdown +/- buttons are always visible (no hover hide)', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');
    
    // Open dropdown by clicking the input
    await modelSelector.locator('.ts-control input').click();
    const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
    await expect(dropdown).toBeVisible();

    // Get an option
    const option = dropdown.locator('[role="option"]').first();
    const decreaseBtn = option.locator('button.ts-count-minus');
    
    // Verify button is visible without hovering
    await expect(decreaseBtn).toBeVisible();
    
    // Hover over the option
    await option.hover();
    
    // Button should STILL be visible (PRD requirement)
    await expect(decreaseBtn).toBeVisible();
    
    // Move mouse away
    await page.mouse.move(0, 0);
    
    // Button should STILL be visible
    await expect(decreaseBtn).toBeVisible();
  });

  test('Dropdown +/- buttons adjust instance count', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');
    
    // Open dropdown
    await modelSelector.locator('.ts-control input').click();
    const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
    await expect(dropdown).toBeVisible();
    
    const option = dropdown.locator('[role="option"]').first();
    const countDisplay = option.locator('.ts-count-display');
    const increaseBtn = option.locator('button.ts-count-plus');
    const decreaseBtn = option.locator('button.ts-count-minus');
    
    // Get initial count
    const initialCount = await countDisplay.textContent();
    expect(initialCount).toBeTruthy();
    const initialNum = parseInt(initialCount || '1');
    
    // Click increase button
    await increaseBtn.click({ force: true });
    await page.waitForTimeout(100);
    
    // Count should increase
    const newCount = await countDisplay.textContent();
    expect(parseInt(newCount || '1')).toBe(initialNum + 1);
    
    // Click decrease button
    await decreaseBtn.click({ force: true });
    await page.waitForTimeout(100);
    
    // Count should go back
    const finalCount = await countDisplay.textContent();
    expect(parseInt(finalCount || '1')).toBe(initialNum);
  });

  test('Selecting model creates badge with +/- buttons', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');
    
    // Open dropdown and select first model
    await modelSelector.locator('.ts-control input').click();
    const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
    await expect(dropdown).toBeVisible();
    
    const firstOption = dropdown.locator('[role="option"]').first();
    await expect(firstOption).toBeVisible();
    
    // Click the option text area (not the +/- buttons)
    await firstOption.locator('span').first().click();
    
    // Wait for badge to appear
    await page.waitForTimeout(500);
    
    // Find the badge in the control (TOM Select creates .item divs for selected items)
    const badge = modelSelector.locator('.ts-control .item').first();
    await expect(badge).toBeVisible({ timeout: 3000 });
    
    // Verify badge has +/- buttons (they appear on hover, so hover first)
    await badge.hover();
    const badgeDecrease = badge.locator('button.ts-count-minus');
    const badgeIncrease = badge.locator('button.ts-count-plus');
    const badgeRemove = badge.locator('button.ts-count-remove');
    
    // The buttons are in the overlay div that appears on hover
    await expect(badgeDecrease).toBeVisible({ timeout: 2000 });
    await expect(badgeIncrease).toBeVisible();
    await expect(badgeRemove).toBeVisible();
  });

  test('Badge +/- buttons adjust instance count', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');
    
    // Select a model first
    await modelSelector.locator('.ts-control input').click();
    const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
    await expect(dropdown).toBeVisible();
    await dropdown.locator('[role="option"]').first().locator('span').first().click();
    
    await page.waitForTimeout(500);
    
    const badge = modelSelector.locator('.ts-control .item').first();
    
    // Get initial count from badge text (should contain ×1)
    let badgeText = await badge.textContent();
    expect(badgeText).toContain('×1');
    
    await badge.hover(); // Hover to show overlay buttons
    const increaseBtn = badge.locator('button.ts-count-plus');
    const decreaseBtn = badge.locator('button.ts-count-minus');
    await expect(increaseBtn).toBeVisible();
    
    // Click increase
    await increaseBtn.click({ force: true });
    await page.waitForTimeout(200);
    
    // Count should show ×2
    badgeText = await badge.textContent();
    expect(badgeText).toContain('×2');
    
    // Hover again after click
    await badge.hover();
    await expect(decreaseBtn).toBeVisible();
    
    // Click decrease
    await decreaseBtn.click({ force: true });
    await page.waitForTimeout(200);
    
    // Count should go back to ×1
    badgeText = await badge.textContent();
    expect(badgeText).toContain('×1');
  });

  test('Badge remove button removes the model', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');
    
    // Select a model
    await modelSelector.locator('.ts-control input').click();
    const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
    await expect(dropdown).toBeVisible();
    await dropdown.locator('[role="option"]').first().locator('span').first().click();
    await page.waitForTimeout(500);
    
    const badge = modelSelector.locator('.ts-control .item').first();
    await expect(badge).toBeVisible();
    
    await badge.hover();
    // Click remove button
    const removeBtn = badge.locator('button.ts-count-remove');
    await expect(removeBtn).toBeVisible();
    await removeBtn.click({ force: true });
    
    // Badge should disappear
    await expect(badge).not.toBeVisible({ timeout: 2000 });
  });

  test('Count bounds: minimum is 1, maximum is 10', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');
    
    // Select a model
    await modelSelector.locator('.ts-control input').click();
    const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
    await expect(dropdown).toBeVisible();
    await dropdown.locator('[role="option"]').first().locator('span').first().click();
    await page.waitForTimeout(500);
    
    const badge = modelSelector.locator('.ts-control .item').first();
    await badge.hover();
    const increaseBtn = badge.locator('button.ts-count-plus');
    const decreaseBtn = badge.locator('button.ts-count-minus');
    
    await expect(decreaseBtn).toBeVisible();
    
    // Try to decrease below 1 (should stay at 1)
    await decreaseBtn.click({ force: true });
    await page.waitForTimeout(100);
    let badgeText = await badge.textContent();
    expect(badgeText).toContain('×1');
    
    // Increase to maximum (9 clicks to go from 1 to 10)
    await badge.hover(); // Re-hover after click
    for (let i = 0; i < 9; i++) {
      await increaseBtn.click({ force: true });
      await page.waitForTimeout(50);
    }
    
    // Should be at ×10
    badgeText = await badge.textContent();
    const maxNum = parseInt((badgeText || '').replace(/.*×/, ''));
    expect(maxNum).toBe(10);
  });

  test('Multiple models can be selected with different counts', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');
    
    // Select first model
    await modelSelector.locator('.ts-control input').click();
    let dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
    await expect(dropdown).toBeVisible();
    await dropdown.locator('[role="option"]').first().locator('span').first().click();
    await page.waitForTimeout(300);
    
    // Select second model
    await modelSelector.locator('.ts-control input').click();
    dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
    await expect(dropdown).toBeVisible();
    await dropdown.locator('[role="option"]').nth(1).locator('span').first().click();
    await page.waitForTimeout(300);
    
    // Should have 2 badges
    const badges = modelSelector.locator('.ts-control .item');
    const badgeCount = await badges.count();
    expect(badgeCount).toBeGreaterThanOrEqual(2);
    
    // Set different counts
    const firstBadge = badges.first();
    const secondBadge = badges.nth(1);
    
    // Increase first badge count
    await firstBadge.hover();
    await firstBadge.locator('button.ts-count-plus').click({ force: true });
    await page.waitForTimeout(100);
    
    // Increase second badge count twice
    await secondBadge.hover();
    await secondBadge.locator('button.ts-count-plus').click({ force: true });
    await page.waitForTimeout(50);
    await secondBadge.hover(); // Re-hover
    await secondBadge.locator('button.ts-count-plus').click({ force: true });
    await page.waitForTimeout(100);
    
    // Verify different counts
    const firstCount = await firstBadge.textContent();
    const secondCount = await secondBadge.textContent();
    
    expect(firstCount).toContain('×2');
    expect(secondCount).toContain('×3');
  });

  test('TOM Select has proper ARIA attributes', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');
    
    // Check for TOM Select wrapper
    const tomSelectWrapper = modelSelector.locator('.ts-wrapper');
    await expect(tomSelectWrapper).toBeVisible();
    
    // Original select should have aria-label
    const select = modelSelector.locator('select');
    const ariaLabel = await select.getAttribute('aria-label');
    expect(ariaLabel).toBeTruthy();
  });
});
