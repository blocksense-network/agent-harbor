import { test, expect } from '@playwright/test';

/**
 * TOM Select Dropdown Direction Test
 * 
 * Verifies that TOM Select dropdowns open UPWARD (above the control)
 * instead of downward, to prevent them from covering the footer.
 */

test.describe.skip('TOM Select Dropdown Direction', () => {
  test('Dropdowns should open upward', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('load');

    // Find the draft card
    const draftCard = page.locator('[data-testid="draft-task-card"]');
    await expect(draftCard).toBeVisible();

    // Click on the model selector to open dropdown
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');
    const modelControl = modelSelector.locator('.ts-control');
    
    await modelControl.click();
    await page.waitForTimeout(300); // Wait for dropdown animation

    // Find the dropdown
    const dropdown = page.locator('.ts-dropdown');
    await expect(dropdown).toBeVisible();

    // Get bounding boxes
    const controlBox = await modelControl.boundingBox();
    const dropdownBox = await dropdown.boundingBox();

    if (!controlBox || !dropdownBox) {
      throw new Error('Could not get bounding boxes');
    }

    // Dropdown should be ABOVE the control
    // dropdownBox.y + dropdownBox.height should be less than controlBox.y
    console.log('Control top:', controlBox.y);
    console.log('Dropdown bottom:', dropdownBox.y + dropdownBox.height);
    
    const dropdownBottom = dropdownBox.y + dropdownBox.height;
    const controlTop = controlBox.y;
    
    // Allow small margin for styling (border, spacing)
    const margin = 10;
    
    expect(dropdownBottom).toBeLessThanOrEqual(controlTop + margin);
    
    console.log('✓ Dropdown opens upward as expected');
  });

  test('All selectors on draft card should have upward dropdowns', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('load');

    const draftCard = page.locator('[data-testid="draft-task-card"]');
    await expect(draftCard).toBeVisible();

    // Test repository selector
    const repoSelector = draftCard.locator('[data-testid="repo-selector"]');
    const repoControl = repoSelector.locator('.ts-control');
    
    await repoControl.click();
    await page.waitForTimeout(200);
    
    let dropdown = page.locator('.ts-dropdown.ts-dropdown-up');
    await expect(dropdown).toBeVisible();
    console.log('✓ Repository dropdown has ts-dropdown-up class');
    
    // Close dropdown
    await page.keyboard.press('Escape');
    await page.waitForTimeout(200);

    // Test branch selector
    const branchSelector = draftCard.locator('[data-testid="branch-selector"]');
    const branchControl = branchSelector.locator('.ts-control');
    
    await branchControl.click();
    await page.waitForTimeout(200);
    
    dropdown = page.locator('.ts-dropdown.ts-dropdown-up');
    await expect(dropdown).toBeVisible();
    console.log('✓ Branch dropdown has ts-dropdown-up class');
    
    // Close dropdown
    await page.keyboard.press('Escape');
    await page.waitForTimeout(200);

    // Test model selector
    const modelSelector = draftCard.locator('[data-testid="model-selector"]');
    const modelControl = modelSelector.locator('.ts-control');
    
    await modelControl.click();
    await page.waitForTimeout(200);
    
    dropdown = page.locator('.ts-dropdown.ts-dropdown-up');
    await expect(dropdown).toBeVisible();
    console.log('✓ Model dropdown has ts-dropdown-up class');
  });
});
