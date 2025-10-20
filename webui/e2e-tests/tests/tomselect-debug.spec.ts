import { test, expect } from '@playwright/test';

/**
 * Tom Select Debugging Test
 *
 * Identifies why Tom Select dropdowns aren't appearing.
 */

test('Debug Tom Select elements', async ({ page }) => {
  await page.goto('/');
  await page.waitForLoadState('load');

  const draftCard = page.locator('[data-testid="draft-task-card"]');
  await expect(draftCard).toBeVisible();

  // Check if repo selector exists
  const repoSelector = draftCard.locator('[data-testid="repo-selector"]');
  const repoExists = await repoSelector.count();
  console.log(`Repo selector count: ${repoExists}`);

  if (repoExists > 0) {
    const innerHTML = await repoSelector.innerHTML();
    console.log('Repo selector HTML:', innerHTML.substring(0, 200));

    // Check for .ts-control
    const tsControl = repoSelector.locator('.ts-control');
    const tsControlCount = await tsControl.count();
    console.log(`ts-control count: ${tsControlCount}`);

    if (tsControlCount > 0) {
      const controlHTML = await tsControl.innerHTML();
      console.log('ts-control HTML:', controlHTML.substring(0, 200));

      // Try clicking it
      await tsControl.click();
      await page.waitForTimeout(500);

      // Check all .ts-dropdown elements on the page
      const allDropdowns = await page.locator('.ts-dropdown').all();
      console.log(`Total .ts-dropdown elements: ${allDropdowns.length}`);

      for (let i = 0; i < allDropdowns.length; i++) {
        const dd = allDropdowns[i];
        const isVisible = await dd.isVisible();
        const classes = await dd.getAttribute('class');
        console.log(`Dropdown ${i}: visible=${isVisible}, classes=${classes}`);
      }
    }
  }

  // Check for regular select elements
  const selectElements = await draftCard.locator('select').all();
  console.log(`Select elements: ${selectElements.length}`);

  for (let i = 0; i < selectElements.length; i++) {
    const select = selectElements[i];
    const id = await select.getAttribute('id');
    const testId = await select.getAttribute('data-testid');
    console.log(`Select ${i}: id=${id}, testId=${testId}`);
  }
});
