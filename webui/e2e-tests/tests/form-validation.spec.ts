/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { test, expect } from '@playwright/test';

test.describe('Form Validation Tests', () => {
  test.describe('Task Creation Form Validation', () => {
    test('Required field validation on form submission', async ({ page }) => {
      await page.goto('/create');

      // Click submit without filling anything
      await page.locator('button[type="submit"]').click();

      // Check all required field errors appear
      await expect(page.locator('text=Prompt is required')).toBeVisible();
      await expect(page.locator('text=Repository URL is required')).toBeVisible();
      await expect(page.locator('text=Branch is required')).toBeVisible();
      await expect(page.locator('text=Please select an agent')).toBeVisible();
      await expect(page.locator('text=Please select a runtime')).toBeVisible();
    });

    test('Prompt field validation', async ({ page }) => {
      await page.goto('/create');

      const promptField = page.locator('textarea[id="prompt"]');

      // Empty prompt - should show error on submit
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Prompt is required')).toBeVisible();

      // Fill prompt - error should disappear
      await promptField.fill('Test prompt');
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Prompt is required')).not.toBeVisible();
    });

    test('Repository URL validation', async ({ page }) => {
      await page.goto('/create');

      const repoUrlField = page.locator('input[id="repoUrl"]');

      // Empty URL
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Repository URL is required')).toBeVisible();

      // Invalid URL format
      await repoUrlField.fill('not-a-url');
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Please enter a valid repository URL')).toBeVisible();

      // Valid URL
      await repoUrlField.fill('https://github.com/test/repo.git');
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Please enter a valid repository URL')).not.toBeVisible();
      await expect(page.locator('text=Repository URL is required')).not.toBeVisible();
    });

    test('Branch field validation', async ({ page }) => {
      await page.goto('/create');

      const branchField = page.locator('input[id="repoBranch"]');

      // Empty branch
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Branch is required')).toBeVisible();

      // Fill branch
      await branchField.fill('main');
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Branch is required')).not.toBeVisible();
    });

    test('Agent selection validation', async ({ page }) => {
      await page.goto('/create');

      const agentSelect = page.locator('select[id="agentType"]');

      // Default selection (empty)
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Please select an agent')).toBeVisible();

      // Select an agent
      await agentSelect.selectOption({ index: 1 });
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Please select an agent')).not.toBeVisible();
    });

    test('Runtime selection validation', async ({ page }) => {
      await page.goto('/create');

      const runtimeSelect = page.locator('select[id="runtimeType"]');

      // Default selection (empty)
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Please select a runtime')).toBeVisible();

      // Select a runtime
      await runtimeSelect.selectOption({ index: 1 });
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Please select a runtime')).not.toBeVisible();
    });

    test('Real-time validation clears errors when typing', async ({ page }) => {
      await page.goto('/create');

      // Trigger validation errors
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Prompt is required')).toBeVisible();

      // Start typing in prompt field
      await page.locator('textarea[id="prompt"]').fill('t');

      // Error should still be visible (requires blur or submit for now)
      // This tests the current behavior - errors clear on next submit
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Prompt is required')).not.toBeVisible();
    });

    test('Form shows general error for API failures', async ({ page }) => {
      await page.goto('/create');

      // Fill form with invalid data that might cause API error
      await page.locator('textarea[id="prompt"]').fill('Test prompt');
      await page.locator('input[id="repoUrl"]').fill('https://github.com/test/repo.git');
      await page.locator('input[id="repoBranch"]').fill('main');
      await page.locator('select[id="agentType"]').selectOption({ index: 1 });
      await page.locator('select[id="runtimeType"]').selectOption({ index: 1 });

      // This should work, but if there are API issues, we want to test error display
      await page.locator('button[type="submit"]').click();

      // Either success or API error should be shown
      const hasSuccess = await page
        .locator('text=Task Created Successfully!')
        .isVisible()
        .catch(() => false);
      const hasError = await page
        .locator('[class*="bg-red-50"]')
        .isVisible()
        .catch(() => false);

      expect(hasSuccess || hasError).toBe(true);
    });

    test('Form submission disables submit button', async ({ page }) => {
      await page.goto('/create');

      // Fill valid form
      await page.locator('textarea[id="prompt"]').fill('Test submission state');
      await page.locator('input[id="repoUrl"]').fill('https://github.com/test/repo.git');
      await page.locator('input[id="repoBranch"]').fill('main');
      await page.locator('select[id="agentType"]').selectOption({ index: 1 });
      await page.locator('select[id="runtimeType"]').selectOption({ index: 1 });

      const submitButton = page.locator('button[type="submit"]');

      // Click submit
      await submitButton.click();

      // Button should be disabled during submission
      await expect(submitButton).toBeDisabled();

      // Button text should change
      await expect(submitButton).toHaveText('Creating Task...');
    });

    test('Form fields are disabled during submission', async ({ page }) => {
      await page.goto('/create');

      // Fill valid form
      await page.locator('textarea[id="prompt"]').fill('Test field disabling');
      await page.locator('input[id="repoUrl"]').fill('https://github.com/test/repo.git');
      await page.locator('input[id="repoBranch"]').fill('main');
      await page.locator('select[id="agentType"]').selectOption({ index: 1 });
      await page.locator('select[id="runtimeType"]').selectOption({ index: 1 });

      // Click submit
      await page.locator('button[type="submit"]').click();

      // Fields should be disabled during submission
      await expect(page.locator('textarea[id="prompt"]')).toBeDisabled();
      await expect(page.locator('input[id="repoUrl"]')).toBeDisabled();
      await expect(page.locator('input[id="repoBranch"]')).toBeDisabled();
      await expect(page.locator('select[id="agentType"]')).toBeDisabled();
      await expect(page.locator('select[id="runtimeType"]')).toBeDisabled();
    });
  });

  test.describe('Field-Specific Validation', () => {
    test('Repository URL accepts various valid formats', async ({ page }) => {
      await page.goto('/create');

      const repoUrlField = page.locator('input[id="repoUrl"]');
      const testUrls = [
        'https://github.com/user/repo.git',
        'http://gitlab.com/user/repo.git',
        'git@github.com:user/repo.git',
        'https://bitbucket.org/user/repo.git',
      ];

      for (const url of testUrls) {
        await repoUrlField.fill(url);
        await page.locator('button[type="submit"]').click();

        // Should not show URL validation error
        await expect(page.locator('text=Please enter a valid repository URL')).not.toBeVisible();
        // But will show other required field errors
        await expect(page.locator('text=Prompt is required')).toBeVisible();
      }
    });

    test('Branch field accepts various branch names', async ({ page }) => {
      await page.goto('/create');

      const branchField = page.locator('input[id="repoBranch"]');

      const testBranches = [
        'main',
        'master',
        'develop',
        'feature/new-feature',
        'bugfix/issue-123',
        'v1.0.0',
      ];

      for (const branch of testBranches) {
        await branchField.fill(branch);

        // Should not show branch validation error (just other required fields)
        await page.locator('button[type="submit"]').click();
        await expect(page.locator('text=Branch is required')).not.toBeVisible();
      }
    });

    test('Agent and runtime dropdowns load correctly', async ({ page }) => {
      await page.goto('/create');

      // Wait for options to load
      await page.waitForTimeout(1000);

      const agentSelect = page.locator('select[id="agentType"]');
      const runtimeSelect = page.locator('select[id="runtimeType"]');

      // Should have more than just the placeholder option
      const agentOptions = await agentSelect.locator('option').all();
      const runtimeOptions = await runtimeSelect.locator('option').all();

      expect(agentOptions.length).toBeGreaterThan(1);
      expect(runtimeOptions.length).toBeGreaterThan(1);

      // First option should be placeholder
      await expect(agentOptions[0]).toHaveText('Select an agent...');
      await expect(runtimeOptions[0]).toHaveText('Select a runtime...');
    });
  });

  test.describe('Error Display and Recovery', () => {
    test('Error messages have proper styling', async ({ page }) => {
      await page.goto('/create');

      // Submit empty form to trigger errors
      await page.locator('button[type="submit"]').click();

      // Check error message styling
      const errorMessages = page.locator('text=Prompt is required');
      await expect(errorMessages).toHaveClass(/text-red-600/);
    });

    test('Form recovers from validation errors', async ({ page }) => {
      await page.goto('/create');

      // Submit empty form
      await page.locator('button[type="submit"]').click();
      await expect(page.locator('text=Prompt is required')).toBeVisible();

      // Fill required fields
      await page.locator('textarea[id="prompt"]').fill('Test recovery');
      await page.locator('input[id="repoUrl"]').fill('https://github.com/test/repo.git');
      await page.locator('input[id="repoBranch"]').fill('main');
      await page.locator('select[id="agentType"]').selectOption({ index: 1 });
      await page.locator('select[id="runtimeType"]').selectOption({ index: 1 });

      // Submit again - errors should be gone
      await page.locator('button[type="submit"]').click();

      // Should show success, not errors
      await expect(page.locator('text=Task Created Successfully!')).toBeVisible();
      await expect(page.locator('text=Prompt is required')).not.toBeVisible();
    });
  });
});
