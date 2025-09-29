import { test, expect } from '@playwright/test';

test.describe.skip('TOM Select Upward Positioning', () => {
  test('dropdown opens upward when not enough space below', async ({ page }) => {
    await page.goto('/');

    // Set viewport to small height to force upward positioning
    await page.setViewportSize({ width: 1024, height: 400 });

    // Scroll to near bottom of page to limit space below
    await page.evaluate(() => {
      window.scrollTo(0, document.body.scrollHeight - 300);
    });

    await page.waitForSelector('[data-testid="draft-task-card"]');
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();

    // Wait for TOM Select components to initialize
    await page.waitForTimeout(1000);

    // Click on model selector to open dropdown
    const modelSelector = draftCard.locator('[data-testid="model-multi-select"]');
    await modelSelector.click();

    // Wait for dropdown to appear
    const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
    await expect(dropdown).toBeVisible();

    // Get positions of control and dropdown
    const controlBox = await modelSelector.boundingBox();
    const dropdownBox = await dropdown.boundingBox();

    if (controlBox && dropdownBox) {
      // Dropdown bottom edge should be 5px above control top edge
      expect(dropdownBox.y + dropdownBox.height).toBeGreaterThanOrEqual(controlBox.y - 10); // Allow some tolerance
      expect(dropdownBox.y + dropdownBox.height).toBeLessThanOrEqual(controlBox.y); // Should be above control
    }
  });

  test('dropdown always opens upward for consistency', async ({ page }) => {
    await page.goto('/');

    // Use normal viewport size
    await page.setViewportSize({ width: 1024, height: 768 });

    // Scroll to top (plenty of space above)
    await page.evaluate(() => {
      window.scrollTo(0, 0);
    });

    await page.waitForSelector('[data-testid="draft-task-card"]');
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();

    // Wait for TOM Select components to initialize
    await page.waitForTimeout(1000);

    // Click on model selector to open dropdown
    const modelSelector = draftCard.locator('[data-testid="model-multi-select"]');
    await modelSelector.click();

    // Wait for dropdown to appear
    const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
    await expect(dropdown).toBeVisible();

    // Get positions of control and dropdown
    const controlBox = await modelSelector.boundingBox();
    const dropdownBox = await dropdown.boundingBox();

    if (controlBox && dropdownBox) {
      // Dropdown bottom edge should be 5px above control top edge
      expect(dropdownBox.y + dropdownBox.height).toBeGreaterThanOrEqual(controlBox.y - 10); // Allow some tolerance
      expect(dropdownBox.y + dropdownBox.height).toBeLessThanOrEqual(controlBox.y); // Should be above control
    }
  });

  test('repository selector always opens upward', async ({ page }) => {
    await page.goto('/');

    // Test with different scroll positions
    const scrollPositions = [0, 200, 400];

    for (const scrollY of scrollPositions) {
      await page.evaluate((y) => window.scrollTo(0, y), scrollY);

      await page.waitForSelector('[data-testid="draft-task-card"]');
      const draftCard = page.locator('[data-testid="draft-task-card"]').first();

      // Wait for TOM Select components to initialize
      await page.waitForTimeout(1000);

      // Click on repository selector
      const repoSelector = draftCard.locator('[data-testid="repo-selector"]');
      await repoSelector.click();

      // Wait for dropdown to appear
      const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
      await expect(dropdown).toBeVisible();

      // Get positions
      const controlBox = await repoSelector.boundingBox();
      const dropdownBox = await dropdown.boundingBox();

      if (controlBox && dropdownBox) {
        // Dropdown bottom edge should be 5px above control top edge
        expect(dropdownBox.y + dropdownBox.height).toBeGreaterThanOrEqual(controlBox.y - 10); // Allow some tolerance
        expect(dropdownBox.y + dropdownBox.height).toBeLessThanOrEqual(controlBox.y); // Should be above control
      }

      // Close dropdown
      await page.keyboard.press('Escape');
    }
  });

  test('dropdown always positions upward regardless of scroll position', async ({ page }) => {
    await page.goto('/');

    await page.setViewportSize({ width: 1024, height: 600 });

    await page.waitForSelector('[data-testid="draft-task-card"]');
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();
    const modelSelector = draftCard.locator('[data-testid="model-multi-select"]');

    // Test with different scroll positions
    const scrollPositions = [0, 200, 400, 600];

    for (const scrollY of scrollPositions) {
      await page.evaluate((y) => window.scrollTo(0, y), scrollY);
      await modelSelector.click();

      const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
      await expect(dropdown).toBeVisible();

      // Check position (should always be above)
      const controlBox = await modelSelector.boundingBox();
      const dropdownBox = await dropdown.boundingBox();

      if (controlBox && dropdownBox) {
        // Dropdown bottom edge should be 5px above control top edge
        expect(dropdownBox.y + dropdownBox.height).toBeGreaterThanOrEqual(controlBox.y - 10); // Allow some tolerance
        expect(dropdownBox.y + dropdownBox.height).toBeLessThanOrEqual(controlBox.y); // Should be above control
      }

      // Close dropdown
      await page.keyboard.press('Escape');
    }
  });

  test('dropdown width matches control width', async ({ page }) => {
    await page.goto('/');

    await page.waitForSelector('[data-testid="draft-task-card"]');
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();

    // Test both selectors
    const selectors = [
      draftCard.locator('[data-testid="repo-selector"]'),
      draftCard.locator('[data-testid="model-multi-select"]')
    ];

    for (const selector of selectors) {
      await selector.click();

      const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
      await expect(dropdown).toBeVisible();

      const controlBox = await selector.boundingBox();
      const dropdownBox = await dropdown.boundingBox();

      if (controlBox && dropdownBox) {
        // Dropdown width should match control width
        expect(Math.abs(dropdownBox.width - controlBox.width)).toBeLessThan(2); // Allow 1px tolerance
      }

      // Close dropdown
      await page.keyboard.press('Escape');
    }
  });

  test('dropdown remains within viewport bounds', async ({ page }) => {
    await page.goto('/');

    await page.setViewportSize({ width: 1024, height: 400 });

    await page.waitForSelector('[data-testid="draft-task-card"]');
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();

    // Test with different scroll positions
    const scrollPositions = [0, 200, 400, 600];

    for (const scrollY of scrollPositions) {
      await page.evaluate((y) => window.scrollTo(0, y), scrollY);

      const modelSelector = draftCard.locator('[data-testid="model-multi-select"]');
      await modelSelector.click();

      const dropdown = page.locator('.ts-dropdown:not(.hidden)').first();
      await expect(dropdown).toBeVisible();

      const dropdownBox = await dropdown.boundingBox();
      const viewportSize = page.viewportSize();

      if (dropdownBox && viewportSize) {
        // Dropdown should be at least partially visible
        const isVisible = dropdownBox.y < viewportSize.height && dropdownBox.y + dropdownBox.height > 0;
        expect(isVisible).toBe(true);
      }

      await page.keyboard.press('Escape');
    }
  });

  test('upward dropdown has proper styling', async ({ page }) => {
    await page.goto('/');

    await page.waitForSelector('[data-testid="draft-task-card"]');
    const draftCard = page.locator('[data-testid="draft-task-card"]').first();

    // Wait for TOM Select components to initialize
    await page.waitForTimeout(1000);

    // Test both selectors
    const selectors = [
      draftCard.locator('[data-testid="repo-selector"]'),
      draftCard.locator('[data-testid="model-multi-select"]')
    ];

    for (const selector of selectors) {
      await selector.click();

      const dropdown = page.locator('.ts-dropdown:not(.hidden).ts-dropdown-upward').first();
      await expect(dropdown).toBeVisible();

      // Check that upward class is applied
      await expect(dropdown).toHaveClass(/ts-dropdown-upward/);

      // Check computed styles
      const backgroundColor = await dropdown.evaluate((el) => getComputedStyle(el).backgroundColor);
      expect(backgroundColor).toBe('rgb(255, 255, 255)'); // White background

      const border = await dropdown.evaluate((el) => getComputedStyle(el).border);
      expect(border).toContain('1px solid rgb(204, 204, 204)'); // Gray border

      const borderRadius = await dropdown.evaluate((el) => getComputedStyle(el).borderRadius);
      expect(borderRadius).toBe('4px 4px 0px 0px'); // Rounded top corners

      await page.keyboard.press('Escape');
    }
  });
});
