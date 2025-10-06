import { test, expect } from '@playwright/test';

test.describe('TOM Select Integration', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('load'); // Use 'load' instead of 'networkidle' due to persistent SSE connections
  });

  test('Repository selector renders with TOM Select and allows selection', async ({ page }) => {
    // Find the draft task card
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    await expect(draftCard).toBeVisible();

    // Find the repository selector
    const repoSelector = draftCard.locator('[data-testid="repo-selector"]');
    await expect(repoSelector).toBeVisible();

    // Check that it has the basic TOM Select structure
    const selectElement = repoSelector.locator('select.tom-select-input');
    await expect(selectElement).toBeVisible();

    // Check that TOM Select has been initialized (should have ts-wrapper class)
    const wrapper = repoSelector.locator('.ts-wrapper');
    await expect(wrapper).toBeVisible();
  });

  test('Branch selector renders with TOM Select and allows selection', async ({ page }) => {
    // Find the draft task card
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    await expect(draftCard).toBeVisible();

    // Find the branch selector
    const branchSelector = draftCard.locator('[data-testid="branch-selector"]');
    await expect(branchSelector).toBeVisible();

    // Check that it has the basic TOM Select structure
    const selectElement = branchSelector.locator('select.tom-select-input');
    await expect(selectElement).toBeVisible();

    // Check that TOM Select has been initialized (should have ts-wrapper class)
    const wrapper = branchSelector.locator('.ts-wrapper');
    await expect(wrapper).toBeVisible();
  });

  test('Model selector renders with multi-select TOM Select structure', async ({ page }) => {
    // Find the draft task card
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    await expect(draftCard).toBeVisible();

    // Find the model selector
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');
    await expect(modelSelector).toBeVisible();
    console.log(
      'MODEL SELECTOR HTML:',
      await modelSelector.evaluate((el) => el.outerHTML)
    );

    // Check that it has the basic TOM Select structure
    const selectElement = modelSelector.locator('select.tom-select-input');
    await expect(selectElement).toBeVisible();

    // Check that TOM Select has been initialized (should have ts-wrapper class)
    const wrapper = modelSelector.locator('.ts-wrapper');
    await expect(wrapper).toBeVisible();

    // Check that there are option elements
    const options = selectElement.locator('option');
    await expect(options.first()).toBeVisible();
  });

  test('Dropdown +/- buttons are always visible (no hover hide)', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');

    // Open dropdown
    await modelSelector.locator('.ts-control').click({ force: true });
    const dropdown = page.locator('.ts-dropdown.multi').last();
    await dropdown.waitFor({ state: 'visible' });

    // Get an option
    const option = dropdown.locator('[role="option"]').first();
    const decreaseBtn = option.locator('button.decrease-btn');

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
    await modelSelector.locator('.ts-control').click({ force: true });
    const dropdown = page.locator('.ts-dropdown.multi').last();
    await dropdown.waitFor({ state: 'visible' });

    const option = dropdown.locator('[role="option"]').first();
    const countDisplay = option.locator('.count-display');
    const increaseBtn = option.locator('button.increase-btn');
    const decreaseBtn = option.locator('button.decrease-btn');

    await expect(countDisplay).toHaveText('0');

    // Click increase button
    await increaseBtn.click({ force: true });
    await expect(countDisplay).toHaveText('1');
    console.log(
      'option after increase',
      await option.evaluate((el) => el.innerHTML),
    );

    // Click decrease button
    await decreaseBtn.click({ force: true });
    await expect(countDisplay).toHaveText('0');
  });

  test('Selecting model creates badge with +/- buttons', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');

    // Open dropdown and select first model
    await modelSelector.locator('.ts-control').click({ force: true });
    const dropdown = page.locator('.ts-dropdown.multi').last();
    await dropdown.waitFor({ state: 'visible' });

    const firstOption = dropdown.locator('[role="option"]').first();
    await expect(firstOption).toBeVisible();

    // Select the option by clicking the label area
    await firstOption.locator('.model-label').click({ force: true });

    // Find the badge in the control (Tom Select renders badges as .item elements)
    const badge = modelSelector.locator('.ts-control .model-badge').first();
    await expect(badge).toBeVisible();

    // Verify badge has +/- buttons and remove button
    const badgeDecrease = badge.locator('button.decrease-badge-btn');
    const badgeIncrease = badge.locator('button.increase-badge-btn');
    const badgeRemove = badge.locator('button.remove-badge-btn');
    const countBadge = badge.locator('.count-badge');

    await expect(badgeDecrease).toBeVisible();
    await expect(badgeIncrease).toBeVisible();
    await expect(badgeRemove).toBeVisible();
    await expect(countBadge).toHaveText(/×1/); // Should start with count 1
  });

  test('Badge +/- buttons adjust instance count', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');

    // Select a model first
    await modelSelector.locator('.ts-control').click({ force: true });
    const dropdown = page.locator('.ts-dropdown.multi').last();
    await dropdown.waitFor({ state: 'visible' });
    await dropdown.locator('[role="option"]').first().locator('.model-label').click({ force: true });

    const badge = modelSelector.locator('.ts-control .model-badge').first();
    const countBadge = badge.locator('.count-badge');
    await expect(countBadge).toHaveText(/×1/);

    const increaseBtn = badge.locator('button.increase-badge-btn');
    const decreaseBtn = badge.locator('button.decrease-badge-btn');

    await increaseBtn.click({ force: true });
    await expect(countBadge).toHaveText(/×2/);

    await decreaseBtn.click({ force: true });
    await expect(countBadge).toHaveText(/×1/);
  });

  test('Badge remove button removes the model', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');

    // Select a model
    await modelSelector.locator('.ts-control').click({ force: true });
    const dropdown = page.locator('.ts-dropdown.multi').last();
    await dropdown.waitFor({ state: 'visible' });
    await dropdown.locator('[role="option"]').first().locator('.model-label').click({ force: true });

    const badgeList = modelSelector.locator('.ts-control .model-badge');
    await expect(badgeList).toHaveCount(1);

    const removeBtn = badgeList.first().locator('button.remove-badge-btn');
    await removeBtn.click({ force: true });

    await expect(badgeList).toHaveCount(0);
  });

  test('Count bounds: minimum is 1, maximum is 10', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');

    // Select a model
    await modelSelector.locator('.ts-control').click({ force: true });
    const dropdown = page.locator('.ts-dropdown.multi').last();
    await dropdown.waitFor({ state: 'visible' });
    await dropdown.locator('[role="option"]').first().locator('.model-label').click({ force: true });

    const badge = modelSelector.locator('.ts-control .model-badge').first();
    const countBadge = badge.locator('.count-badge');
    const increaseBtn = badge.locator('button.increase-badge-btn');
    const decreaseBtn = badge.locator('button.decrease-badge-btn');

    await decreaseBtn.click({ force: true });
    await expect(countBadge).toHaveText(/×1/);

    for (let i = 0; i < 9; i++) {
      await increaseBtn.click({ force: true });
    }
    await expect(countBadge).toHaveText(/×10/);

    await increaseBtn.click({ force: true });
    await expect(countBadge).toHaveText(/×10/);
  });

  test('Multiple models can be selected with different counts', async ({ page }) => {
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');

    // Select first model
    await modelSelector.locator('.ts-control').click({ force: true });
    let dropdown = page.locator('.ts-dropdown.multi').last();
    await dropdown.waitFor({ state: 'visible' });
    await dropdown.locator('[role="option"]').first().locator('.model-label').click({ force: true });

    // Select second model
    await modelSelector.locator('.ts-control').click({ force: true });
    dropdown = page.locator('.ts-dropdown.multi').last();
    await dropdown.waitFor({ state: 'visible' });
    await dropdown.locator('[role="option"]').nth(1).locator('.model-label').click({ force: true });

    // Should have 2 badges
    const badges = modelSelector.locator('.ts-control .model-badge');
    await expect(badges).toHaveCount(2);

    // Set different counts
    const firstBadge = badges.first();
    const secondBadge = badges.nth(1);

    // Increase first badge count
    await firstBadge.locator('button.increase-badge-btn').click({ force: true });

    // Increase second badge count twice
    await secondBadge.locator('button.increase-badge-btn').click({ force: true });
    await secondBadge.locator('button.increase-badge-btn').click({ force: true });

    // Verify different counts
    await expect(firstBadge.locator('.count-badge')).toHaveText(/×2/);
    await expect(secondBadge.locator('.count-badge')).toHaveText(/×3/);
  });
});
