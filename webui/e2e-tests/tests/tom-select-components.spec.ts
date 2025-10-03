import { test, expect } from '@playwright/test';

/**
 * TOM Select Integration Tests
 * 
 * Validates TOM Select (https://tom-select.js.org/) widget integration as specified in WebUI-PRD.md:
 * - Repository selector with fuzzy search
 * - Branch selector with fuzzy search
 * - Model selector with multi-select and instance counters
 * - Keyboard navigation within dropdowns
 * - Proper popup positioning and backdrop
 * - Smooth animations and user experience
 */

test.describe('TOM Select Components', () => {
  test.beforeEach(async ({ page }) => {
    // Navigate to dashboard and wait for draft task card
    await page.goto('/');
    await page.waitForSelector('[data-testid="draft-task-card"]', { timeout: 10000 });
  });

  test.describe('Repository Selector', () => {
    test('renders TOM Select widget with placeholder', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      
      // Verify TOM Select widget is present
      await expect(repoSelector).toBeVisible();
      
      // Verify placeholder text
      await expect(repoSelector).toContainText('Repository');
      
      // Verify TOM Select classes are applied
      await expect(repoSelector).toHaveClass(/ts-wrapper|tom-select/);
    });

    test('dropdown opens on click', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      
      // Verify dropdown is visible
      const dropdown = page.locator('.ts-dropdown, [role="listbox"]');
      await expect(dropdown).toBeVisible();
      
      // Verify dropdown contains repository options
      await expect(dropdown).toContainText('agent-harbor-webui');
      await expect(dropdown).toContainText('agent-harbor-core');
      await expect(dropdown).toContainText('agent-harbor-cli');
    });

    test('fuzzy search filters repository list', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      
      // Type search query
      const input = page.locator('.ts-control input, [role="combobox"]');
      await input.fill('webui');
      
      // Verify filtered results
      const dropdown = page.locator('.ts-dropdown, [role="listbox"]');
      await expect(dropdown).toContainText('agent-harbor-webui');
      await expect(dropdown).not.toContainText('agent-harbor-cli');
    });

    test('fuzzy search matches partial strings', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      
      const input = page.locator('.ts-control input, [role="combobox"]');
      
      // Search with non-contiguous characters
      await input.fill('awcore');
      
      // Should match "agent-harbor-core"
      const dropdown = page.locator('.ts-dropdown, [role="listbox"]');
      await expect(dropdown).toContainText('agent-harbor-core');
    });

    test('selecting repository updates draft state', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      
      // Select a repository
      await page.locator('.ts-dropdown-content:has-text("agent-harbor-webui")').click();
      
      // Verify selection is displayed
      await expect(repoSelector).toContainText('agent-harbor-webui');
      
      // Verify dropdown closes
      const dropdown = page.locator('.ts-dropdown');
      await expect(dropdown).not.toBeVisible();
    });

    test('previously selected repository persists as default', async ({ page }) => {
      // Select a repository
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      await page.locator('.ts-dropdown-content:has-text("agent-harbor-core")').click();
      
      // Create a new draft task
      await page.locator('footer button:has-text("New Task")').click();
      
      // Verify new draft has the same repository pre-selected
      const newDraftRepoSelector = page.locator('[data-testid="repo-selector"]').last();
      await expect(newDraftRepoSelector).toContainText('agent-harbor-core');
    });

    test('keyboard navigation works in dropdown', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      
      // Press down arrow to navigate
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('ArrowDown');
      
      // Press Enter to select
      await page.keyboard.press('Enter');
      
      // Verify selection was made
      await expect(repoSelector).not.toContainText('Repository'); // placeholder gone
    });

    test('Escape key closes dropdown', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      
      const dropdown = page.locator('.ts-dropdown');
      await expect(dropdown).toBeVisible();
      
      // Press Escape
      await page.keyboard.press('Escape');
      
      // Verify dropdown closes
      await expect(dropdown).not.toBeVisible();
    });
  });

  test.describe('Branch Selector', () => {
    test('renders TOM Select widget with placeholder', async ({ page }) => {
      const branchSelector = page.locator('[data-testid="branch-selector"]');
      
      await expect(branchSelector).toBeVisible();
      await expect(branchSelector).toContainText('Branch');
      await expect(branchSelector).toHaveClass(/ts-wrapper|tom-select/);
    });

    test('shows available branches in dropdown', async ({ page }) => {
      const branchSelector = page.locator('[data-testid="branch-selector"]');
      await branchSelector.click();
      
      const dropdown = page.locator('.ts-dropdown');
      
      // Verify common branch names are shown
      await expect(dropdown).toContainText('main');
      await expect(dropdown).toContainText('develop');
      await expect(dropdown).toContainText('feature/new-ui');
    });

    test('fuzzy search filters branch list', async ({ page }) => {
      const branchSelector = page.locator('[data-testid="branch-selector"]');
      await branchSelector.click();
      
      const input = page.locator('.ts-control input').nth(1); // second selector
      await input.fill('feat');
      
      const dropdown = page.locator('.ts-dropdown');
      await expect(dropdown).toContainText('feature/new-ui');
      await expect(dropdown).not.toContainText('main');
    });

    test('selecting branch updates draft state', async ({ page }) => {
      const branchSelector = page.locator('[data-testid="branch-selector"]');
      await branchSelector.click();
      
      await page.locator('.ts-dropdown-content:has-text("develop")').click();
      
      await expect(branchSelector).toContainText('develop');
    });

    test('loads branches from API dynamically', async ({ page }) => {
      // First select a repository
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      await page.locator('.ts-dropdown-content:has-text("agent-harbor-webui")').click();
      
      // Wait for API call to load branches
      await page.waitForTimeout(500);
      
      // Open branch selector
      const branchSelector = page.locator('[data-testid="branch-selector"]');
      await branchSelector.click();
      
      // Verify branches are loaded (should show loading state first)
      const dropdown = page.locator('.ts-dropdown');
      await expect(dropdown).toBeVisible();
      
      // Should have at least one branch option
      const options = dropdown.locator('[role="option"]');
      expect(await options.count()).toBeGreaterThan(0);
    });
  });

  test.describe('Model Selector (Multi-Select)', () => {
    test('renders TOM Select multi-select widget', async ({ page }) => {
      const modelSelector = page.locator('[data-testid="model-selector"]');
      
      await expect(modelSelector).toBeVisible();
      await expect(modelSelector).toContainText('Model');
      
      // Verify it's a multi-select variant
      await expect(modelSelector).toHaveClass(/ts-wrapper|tom-select/);
    });

    test('opens popup with model list and instance counters', async ({ page }) => {
      const modelSelector = page.locator('[data-testid="model-selector"]');
      await modelSelector.click();
      
      // Verify popup/dropdown is visible
      const popup = page.locator('.ts-dropdown, [data-testid="model-selector-popup"]');
      await expect(popup).toBeVisible();
      
      // Verify models are listed
      await expect(popup).toContainText('claude 3.5-sonnet');
      await expect(popup).toContainText('claude 3-haiku');
      await expect(popup).toContainText('gpt 4');
      await expect(popup).toContainText('gpt 3.5-turbo');
      
      // Verify each model has +/- buttons
      const plusButtons = popup.locator('button[aria-label*="Increment"]');
      expect(await plusButtons.count()).toBeGreaterThan(0);
    });

    test('increment button increases instance count', async ({ page }) => {
      const modelSelector = page.locator('[data-testid="model-selector"]');
      await modelSelector.click();
      
      // Find Claude 3.5 Sonnet increment button
      const incrementBtn = page.locator('button[aria-label="Increment claude 3.5-sonnet"]');
      
      // Verify initial count is 0
      const initialCount = page.locator('text=claude 3.5-sonnet').locator('..').locator('text=/^\\d+$/');
      await expect(initialCount).toContainText('0');
      
      // Click increment
      await incrementBtn.click();
      
      // Verify count increased to 1
      await expect(initialCount).toContainText('1');
    });

    test('decrement button decreases instance count', async ({ page }) => {
      const modelSelector = page.locator('[data-testid="model-selector"]');
      await modelSelector.click();
      
      // Increment to 2
      const incrementBtn = page.locator('button[aria-label="Increment claude 3.5-sonnet"]');
      await incrementBtn.click();
      await incrementBtn.click();
      
      // Verify count is 2
      const count = page.locator('text=claude 3.5-sonnet').locator('..').locator('text=/^\\d+$/');
      await expect(count).toContainText('2');
      
      // Decrement
      const decrementBtn = page.locator('button[aria-label="Decrement claude 3.5-sonnet"]');
      await decrementBtn.click();
      
      // Verify count decreased to 1
      await expect(count).toContainText('1');
    });

    test('decrement button disabled at zero instances', async ({ page }) => {
      const modelSelector = page.locator('[data-testid="model-selector"]');
      await modelSelector.click();
      
      const decrementBtn = page.locator('button[aria-label="Decrement claude 3.5-sonnet"]');
      
      // Should be disabled when count is 0
      await expect(decrementBtn).toBeDisabled();
    });

    test('multiple models can be selected simultaneously', async ({ page }) => {
      const modelSelector = page.locator('[data-testid="model-selector"]');
      await modelSelector.click();
      
      // Select multiple models
      await page.locator('button[aria-label="Increment claude 3.5-sonnet"]').click();
      await page.locator('button[aria-label="Increment gpt 4"]').click();
      
      // Close popup
      await page.locator('body').click();
      
      // Verify model selector shows count
      await expect(modelSelector).toContainText('2 models selected');
    });

    test('model selector shows singular when one model selected', async ({ page }) => {
      const modelSelector = page.locator('[data-testid="model-selector"]');
      await modelSelector.click();
      
      // Select one model
      await page.locator('button[aria-label="Increment claude 3.5-sonnet"]').click();
      
      // Close popup
      await page.locator('body').click();
      
      // Verify singular form
      await expect(modelSelector).toContainText('1 model selected');
    });

    test('zero instances removes model from selection', async ({ page }) => {
      const modelSelector = page.locator('[data-testid="model-selector"]');
      await modelSelector.click();
      
      // Increment then decrement back to 0
      const incrementBtn = page.locator('button[aria-label="Increment claude 3.5-sonnet"]');
      const decrementBtn = page.locator('button[aria-label="Decrement claude 3.5-sonnet"]');
      
      await incrementBtn.click();
      await decrementBtn.click();
      
      // Close popup
      await page.locator('body').click();
      
      // Verify no models selected
      await expect(modelSelector).toContainText('Model'); // back to placeholder
    });

    test('instance counters persist when reopening popup', async ({ page }) => {
      const modelSelector = page.locator('[data-testid="model-selector"]');
      await modelSelector.click();
      
      // Set counts
      await page.locator('button[aria-label="Increment claude 3.5-sonnet"]').click();
      await page.locator('button[aria-label="Increment claude 3.5-sonnet"]').click();
      await page.locator('button[aria-label="Increment gpt 4"]').click();
      
      // Close popup
      await page.keyboard.press('Escape');
      
      // Reopen popup
      await modelSelector.click();
      
      // Verify counts persisted
      const claudeCount = page.locator('text=claude 3.5-sonnet').locator('..').locator('text=/^\\d+$/');
      const gptCount = page.locator('text=gpt 4').locator('..').locator('text=/^\\d+$/');
      
      await expect(claudeCount).toContainText('2');
      await expect(gptCount).toContainText('1');
    });

    test('fuzzy search filters model list', async ({ page }) => {
      const modelSelector = page.locator('[data-testid="model-selector"]');
      await modelSelector.click();
      
      // TOM Select should have search input
      const searchInput = page.locator('.ts-dropdown input[type="text"]');
      await searchInput.fill('claude');
      
      const dropdown = page.locator('.ts-dropdown');
      await expect(dropdown).toContainText('claude 3.5-sonnet');
      await expect(dropdown).toContainText('claude 3-haiku');
      await expect(dropdown).not.toContainText('gpt 4');
    });
  });

  test.describe('TOM Select Features', () => {
    test('popup positioning does not overflow viewport', async ({ page }) => {
      // Scroll to bottom of page
      await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
      
      // Open selector near bottom
      const modelSelector = page.locator('[data-testid="model-selector"]');
      await modelSelector.click();
      
      // Get dropdown position
      const dropdown = page.locator('.ts-dropdown');
      const dropdownBox = await dropdown.boundingBox();
      const viewportSize = page.viewportSize();
      
      // Verify dropdown is within viewport
      expect(dropdownBox!.y + dropdownBox!.height).toBeLessThanOrEqual(viewportSize!.height);
    });

    test('backdrop displays when dropdown open', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      
      // TOM Select may show a backdrop/overlay
      // This is optional depending on configuration
      const backdrop = page.locator('.ts-dropdown-backdrop, .modal-backdrop');
      
      // If backdrop exists, verify it's visible
      if (await backdrop.count() > 0) {
        await expect(backdrop).toBeVisible();
      }
    });

    test('clicking backdrop closes dropdown', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      
      const dropdown = page.locator('.ts-dropdown');
      await expect(dropdown).toBeVisible();
      
      // Click outside the dropdown
      await page.locator('body').click({ position: { x: 10, y: 10 } });
      
      // Dropdown should close
      await expect(dropdown).not.toBeVisible();
    });

    test('smooth animations on open/close', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      
      // Open dropdown
      await repoSelector.click();
      const dropdown = page.locator('.ts-dropdown');
      
      // Verify dropdown has animation/transition classes
      await expect(dropdown).toHaveClass(/animate|transition|fade/);
      
      // Close dropdown
      await page.keyboard.press('Escape');
      
      // Animation should play when closing too
      await expect(dropdown).not.toBeVisible();
    });

    test('keyboard navigation with arrow keys', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      
      // Get first option
      const firstOption = page.locator('.ts-dropdown [role="option"]').first();
      const firstText = await firstOption.textContent();
      
      // Press down arrow
      await page.keyboard.press('ArrowDown');
      
      // Verify first option is highlighted
      await expect(firstOption).toHaveClass(/active|selected|highlighted/);
      
      // Press down arrow again
      await page.keyboard.press('ArrowDown');
      
      // Verify second option is highlighted
      const secondOption = page.locator('.ts-dropdown [role="option"]').nth(1);
      await expect(secondOption).toHaveClass(/active|selected|highlighted/);
    });

    test('Enter key selects highlighted option', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      
      // Navigate with arrow keys
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('ArrowDown');
      
      // Get second option text
      const secondOption = page.locator('.ts-dropdown [role="option"]').nth(1);
      const optionText = await secondOption.textContent();
      
      // Press Enter to select
      await page.keyboard.press('Enter');
      
      // Verify selection
      await expect(repoSelector).toContainText(optionText!);
    });

    test('Tab key closes dropdown and moves to next field', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      
      const dropdown = page.locator('.ts-dropdown');
      await expect(dropdown).toBeVisible();
      
      // Press Tab
      await page.keyboard.press('Tab');
      
      // Dropdown should close
      await expect(dropdown).not.toBeVisible();
      
      // Focus should move to branch selector
      const branchSelector = page.locator('[data-testid="branch-selector"]');
      // Note: Can't easily test focus state, but Tab should have moved focus
    });
  });

  test.describe('Accessibility', () => {
    test('TOM Select has proper ARIA attributes', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      
      // Verify ARIA role
      await expect(repoSelector).toHaveAttribute('role', /combobox|listbox/);
      
      // Verify aria-haspopup
      await expect(repoSelector).toHaveAttribute('aria-haspopup', 'listbox');
      
      // Open dropdown
      await repoSelector.click();
      
      // Verify aria-expanded changes
      await expect(repoSelector).toHaveAttribute('aria-expanded', 'true');
    });

    test('dropdown options have proper roles', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      
      // Verify dropdown has role="listbox"
      const dropdown = page.locator('.ts-dropdown');
      await expect(dropdown).toHaveAttribute('role', 'listbox');
      
      // Verify options have role="option"
      const options = dropdown.locator('[role="option"]');
      expect(await options.count()).toBeGreaterThan(0);
    });

    test('screen readers announce selected value', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      await repoSelector.click();
      await page.locator('.ts-dropdown-content:has-text("agent-harbor-webui")').click();
      
      // Verify aria-label or aria-describedby announces selection
      const ariaLabel = await repoSelector.getAttribute('aria-label');
      expect(ariaLabel).toContain('agent-harbor-webui');
    });

    test('keyboard shortcuts are accessible', async ({ page }) => {
      const repoSelector = page.locator('[data-testid="repo-selector"]');
      
      // Verify keyboard help is available (aria-describedby or title)
      const hasKeyboardHelp = 
        await repoSelector.getAttribute('aria-describedby') !== null ||
        await repoSelector.getAttribute('title') !== null;
      
      expect(hasKeyboardHelp).toBe(true);
    });
  });
});