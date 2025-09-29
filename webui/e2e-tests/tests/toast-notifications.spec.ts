import { test, expect } from "@playwright/test";
import { setupTestEnvironment, teardownTestEnvironment } from "../utils/test-helpers";

test.describe("Toast Notifications", () => {
  let baseURL: string;

  test.beforeAll(async () => {
    baseURL = await setupTestEnvironment();
  });

  test.afterAll(async () => {
    await teardownTestEnvironment();
  });

  test("should display error toast for failed session stop", async ({ page }) => {
    await page.goto(baseURL);

    // Wait for the page to load
    await page.waitForSelector('[data-testid="task-card"]');

    // Find a running session and click the stop button
    const stopButton = page.locator('[data-testid="task-card"]').first().locator('button:has-text("Stop")').first();
    await stopButton.click();

    // Check that an error toast appears
    const toast = page.locator('.toast-item').filter({ hasText: 'Failed to stop session' });
    await expect(toast).toBeVisible();

    // Toast should have error styling (red background)
    await expect(toast).toHaveClass(/bg-red-100/);

    // Toast should auto-dismiss after 5 seconds
    await expect(toast).toBeHidden({ timeout: 6000 });
  });

  test("should display error toast for failed session cancel", async ({ page }) => {
    await page.goto(baseURL);

    // Wait for the page to load
    await page.waitForSelector('[data-testid="task-card"]');

    // Find a session and click the cancel button, then confirm
    const cancelButton = page.locator('[data-testid="task-card"]').first().locator('button:has-text("Cancel")').first();

    // Mock the confirm dialog to return true
    page.on('dialog', async dialog => {
      expect(dialog.type()).toBe('confirm');
      await dialog.accept();
    });

    await cancelButton.click();

    // Check that an error toast appears
    const toast = page.locator('.toast-item').filter({ hasText: 'Failed to cancel session' });
    await expect(toast).toBeVisible();

    // Toast should have error styling
    await expect(toast).toHaveClass(/bg-red-100/);
  });

  test("should allow manual dismissal of toasts", async ({ page }) => {
    await page.goto(baseURL);

    // Wait for the page to load and trigger an error
    await page.waitForSelector('[data-testid="task-card"]');

    // Trigger an error by clicking stop on a session
    const stopButton = page.locator('[data-testid="task-card"]').first().locator('button:has-text("Stop")').first();
    await stopButton.click();

    // Wait for toast to appear
    const toast = page.locator('.toast-item').filter({ hasText: 'Failed to stop session' });
    await expect(toast).toBeVisible();

    // Click the dismiss button (×)
    const dismissButton = toast.locator('button[aria-label="Dismiss notification"]');
    await dismissButton.click();

    // Toast should disappear immediately
    await expect(toast).toBeHidden();
  });

  test("should display toasts in top-right corner", async ({ page }) => {
    await page.goto(baseURL);

    // Wait for the page to load and trigger an error
    await page.waitForSelector('[data-testid="task-card"]');

    // Trigger an error
    const stopButton = page.locator('[data-testid="task-card"]').first().locator('button:has-text("Stop")').first();
    await stopButton.click();

    // Check toast positioning
    const toastContainer = page.locator('[aria-label="Notifications"]');
    await expect(toastContainer).toHaveClass(/fixed/);
    await expect(toastContainer).toHaveClass(/top-4/);
    await expect(toastContainer).toHaveClass(/right-4/);
  });

  test("should announce toasts to screen readers", async ({ page }) => {
    await page.goto(baseURL);

    // Wait for the page to load and trigger an error
    await page.waitForSelector('[data-testid="task-card"]');

    // Trigger an error
    const stopButton = page.locator('[data-testid="task-card"]').first().locator('button:has-text("Stop")').first();
    await stopButton.click();

    // Check that toast has proper ARIA attributes
    const toast = page.locator('.toast-item').filter({ hasText: 'Failed to stop session' });
    await expect(toast).toHaveAttribute('role', 'alert');
    await expect(toast).toHaveAttribute('aria-live', 'assertive');
  });
});
