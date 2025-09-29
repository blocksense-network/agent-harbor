import { test, expect } from '@playwright/test';

/**
 * Keyboard Navigation Tests
 * 
 * Validates keyboard-driven interface as specified in WebUI-PRD.md:
 * - Arrow key navigation (↑↓) between task cards
 * - Visual selection state for selected task
 * - Enter key navigates to task details page
 * - Context-sensitive keyboard shortcuts in footer
 * - Draft task text area shortcuts (Enter = Launch, Shift+Enter = New Line)
 * - Ctrl+N (Cmd+N on macOS) creates new draft task
 */

test.describe.skip('Keyboard Navigation', () => {
  test.beforeEach(async ({ page }) => {
    // Navigate to dashboard and wait for content to load
    await page.goto('/');
    await page.waitForSelector('header h1', { timeout: 10000 });
    // Wait for sessions to load
    await page.waitForTimeout(2000);
  });

  test.describe('Arrow Key Navigation', () => {
    test('pressing down arrow key moves selection down in task feed', async ({ page }) => {
      // Focus on the task feed
      await page.keyboard.press('Tab');
      
      // Get the first task card
      const firstCard = page.locator('[data-testid="task-card"]').first();
      
      // Press down arrow - should select first card
      await page.keyboard.press('ArrowDown');
      
      // Verify first card has visual selection indicator (e.g., border, background color)
      await expect(firstCard).toHaveClass(/selected|highlighted|active/);
      
      // Press down arrow again - should move to second card
      await page.keyboard.press('ArrowDown');
      const secondCard = page.locator('[data-testid="task-card"]').nth(1);
      await expect(secondCard).toHaveClass(/selected|highlighted|active/);
      
      // First card should no longer be selected
      await expect(firstCard).not.toHaveClass(/selected|highlighted|active/);
    });

    test('pressing up arrow key moves selection up in task feed', async ({ page }) => {
      // Focus on task feed and select second card
      await page.keyboard.press('Tab');
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('ArrowDown');
      
      const secondCard = page.locator('[data-testid="task-card"]').nth(1);
      await expect(secondCard).toHaveClass(/selected|highlighted|active/);
      
      // Press up arrow - should move back to first card
      await page.keyboard.press('ArrowUp');
      
      const firstCard = page.locator('[data-testid="task-card"]').first();
      await expect(firstCard).toHaveClass(/selected|highlighted|active/);
      await expect(secondCard).not.toHaveClass(/selected|highlighted|active/);
    });

    test('arrow key selection wraps at top of list', async ({ page }) => {
      // Focus on task feed
      await page.keyboard.press('Tab');
      await page.keyboard.press('ArrowDown'); // Select first card
      
      // Press up arrow at first card - should wrap to last card
      await page.keyboard.press('ArrowUp');
      
      const taskCards = page.locator('[data-testid="task-card"]');
      const lastCard = taskCards.last();
      await expect(lastCard).toHaveClass(/selected|highlighted|active/);
    });

    test('arrow key selection wraps at bottom of list', async ({ page }) => {
      const taskCards = page.locator('[data-testid="task-card"]');
      const cardCount = await taskCards.count();
      
      // Focus on task feed and navigate to last card
      await page.keyboard.press('Tab');
      for (let i = 0; i < cardCount; i++) {
        await page.keyboard.press('ArrowDown');
      }
      
      // Press down arrow at last card - should wrap to first card
      await page.keyboard.press('ArrowDown');
      
      const firstCard = taskCards.first();
      await expect(firstCard).toHaveClass(/selected|highlighted|active/);
    });

    test('visual selection indicator is clearly visible', async ({ page }) => {
      // Focus and select first card
      await page.keyboard.press('Tab');
      await page.keyboard.press('ArrowDown');
      
      const selectedCard = page.locator('[data-testid="task-card"]').first();
      
      // Verify visual selection styles (border, background, or both)
      const styles = await selectedCard.evaluate((el) => {
        const computed = window.getComputedStyle(el);
        return {
          borderColor: computed.borderColor,
          borderWidth: computed.borderWidth,
          backgroundColor: computed.backgroundColor,
        };
      });
      
      // Selection should have either a colored border or background change
      const hasVisualSelection = 
        styles.borderWidth !== '0px' || 
        styles.backgroundColor !== 'rgb(255, 255, 255)'; // not white
      
      expect(hasVisualSelection).toBe(true);
    });
  });

  test.describe('Enter Key Navigation', () => {
    test('pressing Enter on selected task navigates to task details page', async ({ page }) => {
      // Focus and select first task
      await page.keyboard.press('Tab');
      await page.keyboard.press('ArrowDown');
      
      const firstCard = page.locator('[data-testid="task-card"]').first();
      const taskId = await firstCard.getAttribute('data-task-id');
      
      // Press Enter - should navigate to task details page
      await page.keyboard.press('Enter');
      
      // Verify navigation to task details page
      await expect(page).toHaveURL(new RegExp(`/tasks/${taskId}`));
      
      // Verify task details page renders
      await expect(page.locator('[data-testid="task-details"]')).toBeVisible();
    });

    test('Esc key returns from task details to task feed', async ({ page }) => {
      // Navigate to task details
      await page.keyboard.press('Tab');
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Enter');
      
      // Wait for task details page
      await expect(page.locator('[data-testid="task-details"]')).toBeVisible();
      
      // Press Esc - should return to task feed
      await page.keyboard.press('Escape');
      
      // Verify back on main dashboard
      await expect(page).toHaveURL('/');
      await expect(page.locator('[data-testid="task-feed"]')).toBeVisible();
    });

    test('browser back button works after Enter navigation', async ({ page }) => {
      // Navigate to task details with Enter
      await page.keyboard.press('Tab');
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Enter');
      
      await expect(page.locator('[data-testid="task-details"]')).toBeVisible();
      
      // Use browser back button
      await page.goBack();
      
      // Verify back on main dashboard
      await expect(page).toHaveURL('/');
      await expect(page.locator('[data-testid="task-feed"]')).toBeVisible();
    });
  });

  test.describe('Context-Sensitive Keyboard Shortcuts Footer', () => {
    test('task feed focused shows navigation shortcuts', async ({ page }) => {
      // Focus on task feed (not in draft text area)
      await page.keyboard.press('Tab');
      
      const footer = page.locator('footer');
      
      // Verify footer shows task feed shortcuts
      await expect(footer).toContainText('↑↓ Navigate');
      await expect(footer).toContainText('Enter Select Task');
      await expect(footer).toContainText('Ctrl+N New Task');
    });

    test('new task text area focused shows draft task shortcuts', async ({ page }) => {
      // Focus on the draft task text area
      const draftTextArea = page.locator('[data-testid="draft-task-textarea"]');
      await draftTextArea.click();
      
      const footer = page.locator('footer');
      
      // Verify footer shows draft task shortcuts
      await expect(footer).toContainText('Enter Launch Agent');
      await expect(footer).toContainText('Shift+Enter New Line');
      await expect(footer).toContainText('Tab Next Field');
    });

    test('footer shows singular "Agent" when one agent selected', async ({ page }) => {
      const draftTextArea = page.locator('[data-testid="draft-task-textarea"]');
      await draftTextArea.click();
      
      // Select only one agent
      const modelSelector = page.locator('[aria-label="Select models"]');
      await modelSelector.click();
      await page.locator('text=claude 3.5-sonnet').click();
      
      // Verify footer shows singular form
      const footer = page.locator('footer');
      await expect(footer).toContainText('Enter Launch Agent'); // singular
    });

    test('footer shows plural "Agents" when multiple agents selected', async ({ page }) => {
      const draftTextArea = page.locator('[data-testid="draft-task-textarea"]');
      await draftTextArea.click();
      
      // Select multiple agents
      const modelSelector = page.locator('[aria-label="Select models"]');
      await modelSelector.click();
      await page.locator('button[aria-label="Increment claude 3.5-sonnet"]').click();
      await page.locator('button[aria-label="Increment gpt 4"]').click();
      
      // Verify footer shows plural form
      const footer = page.locator('footer');
      await expect(footer).toContainText('Enter Launch Agents'); // plural
    });

    test('footer dynamically updates when focus changes', async ({ page }) => {
      const footer = page.locator('footer');
      
      // Initial state - task feed focused
      await page.keyboard.press('Tab');
      await expect(footer).toContainText('↑↓ Navigate');
      
      // Change focus to draft text area
      const draftTextArea = page.locator('[data-testid="draft-task-textarea"]');
      await draftTextArea.click();
      
      // Footer should update
      await expect(footer).toContainText('Enter Launch Agent');
      await expect(footer).not.toContainText('↑↓ Navigate');
      
      // Change focus back to task feed
      await page.keyboard.press('Escape');
      await page.keyboard.press('Tab');
      
      // Footer should update again
      await expect(footer).toContainText('↑↓ Navigate');
      await expect(footer).not.toContainText('Enter Launch Agent');
    });
  });

  test.describe('Draft Task Keyboard Shortcuts', () => {
    test('Enter key in draft text area launches task (if valid)', async ({ page }) => {
      // Fill out a valid draft task
      const draftTextArea = page.locator('[data-testid="draft-task-textarea"]');
      await draftTextArea.fill('Implement feature X');
      
      // Select repository
      await page.locator('button:has-text("Repository")').click();
      await page.locator('text=agents-workflow-webui').click();
      
      // Select branch
      await page.locator('button:has-text("Branch")').click();
      await page.locator('text=main').click();
      
      // Select model
      await page.locator('[aria-label="Select models"]').click();
      await page.locator('button[aria-label="Increment claude 3.5-sonnet"]').click();
      await page.locator('body').click(); // Close popup
      
      // Focus back on text area and press Enter
      await draftTextArea.focus();
      await page.keyboard.press('Enter');
      
      // Verify task was created
      await expect(page.locator('[data-testid="task-card"]')).toContainText('Implement feature X');
    });

    test('Enter key does nothing if draft task is invalid', async ({ page }) => {
      const draftTextArea = page.locator('[data-testid="draft-task-textarea"]');
      await draftTextArea.fill('Incomplete task');
      
      // Don't fill other required fields (repo, branch, model)
      
      // Press Enter - should not create task
      await draftTextArea.focus();
      await page.keyboard.press('Enter');
      
      // Verify no new task card appears with this text
      await expect(page.locator('[data-testid="task-card"]:has-text("Incomplete task")')).not.toBeVisible();
    });

    test('Shift+Enter creates new line in draft text area', async ({ page }) => {
      const draftTextArea = page.locator('[data-testid="draft-task-textarea"]');
      await draftTextArea.fill('Line 1');
      
      // Press Shift+Enter to create new line
      await page.keyboard.press('Shift+Enter');
      await page.keyboard.type('Line 2');
      
      // Verify text area contains both lines
      const textContent = await draftTextArea.inputValue();
      expect(textContent).toContain('Line 1');
      expect(textContent).toContain('Line 2');
      expect(textContent).toMatch(/Line 1[\r\n]+Line 2/);
    });

    test('Tab key navigates between draft form fields', async ({ page }) => {
      const draftTextArea = page.locator('[data-testid="draft-task-textarea"]');
      await draftTextArea.focus();
      
      // Tab should move to repository selector
      await page.keyboard.press('Tab');
      const repoSelector = page.locator('button:has-text("Repository")');
      await expect(repoSelector).toBeFocused();
      
      // Tab should move to branch selector
      await page.keyboard.press('Tab');
      const branchSelector = page.locator('button:has-text("Branch")');
      await expect(branchSelector).toBeFocused();
      
      // Tab should move to model selector
      await page.keyboard.press('Tab');
      const modelSelector = page.locator('[aria-label="Select models"]');
      await expect(modelSelector).toBeFocused();
      
      // Tab should move to Go button
      await page.keyboard.press('Tab');
      const goButton = page.locator('button:has-text("Go")');
      await expect(goButton).toBeFocused();
    });
  });

  test.describe('New Task Button Shortcut', () => {
    test('Ctrl+N creates new draft task', async ({ page }) => {
      const initialDraftCount = await page.locator('[data-testid="draft-task-card"]').count();
      
      // Press Ctrl+N (Cmd+N on macOS)
      const isMac = process.platform === 'darwin';
      await page.keyboard.press(isMac ? 'Meta+KeyN' : 'Control+KeyN');
      
      // Verify new draft task card appears
      const newDraftCount = await page.locator('[data-testid="draft-task-card"]').count();
      expect(newDraftCount).toBe(initialDraftCount + 1);
    });

    test('New Task button displays platform-specific shortcut', async ({ page }) => {
      const newTaskButton = page.locator('footer button:has-text("New Task")');
      
      // Verify button shows either Ctrl+N or Cmd+N
      const buttonText = await newTaskButton.textContent();
      expect(buttonText).toMatch(/(?:Ctrl|Cmd)\+N/);
    });

    test('clicking New Task button creates new draft', async ({ page }) => {
      const initialDraftCount = await page.locator('[data-testid="draft-task-card"]').count();
      
      // Click New Task button in footer
      const newTaskButton = page.locator('footer button:has-text("New Task")');
      await newTaskButton.click();
      
      // Verify new draft task card appears
      const newDraftCount = await page.locator('[data-testid="draft-task-card"]').count();
      expect(newDraftCount).toBe(initialDraftCount + 1);
      
      // Verify focus moves to new draft text area
      const newDraftTextArea = page.locator('[data-testid="draft-task-textarea"]').last();
      await expect(newDraftTextArea).toBeFocused();
    });
  });

  test.describe('Accessibility - Screen Reader Announcements', () => {
    test('arrow key navigation is announced to screen readers', async ({ page }) => {
      // Focus and select task
      await page.keyboard.press('Tab');
      await page.keyboard.press('ArrowDown');
      
      // Verify ARIA live region announces selection
      const liveRegion = page.locator('[role="status"][aria-live="polite"]');
      await expect(liveRegion).toContainText(/selected|navigated/i);
    });

    test('selected task state is announced', async ({ page }) => {
      await page.keyboard.press('Tab');
      await page.keyboard.press('ArrowDown');
      
      const selectedCard = page.locator('[data-testid="task-card"][aria-selected="true"]').first();
      await expect(selectedCard).toHaveAttribute('aria-selected', 'true');
    });

    test('keyboard shortcuts are accessible to screen readers', async ({ page }) => {
      const footer = page.locator('footer');
      
      // Verify footer has appropriate ARIA role
      await expect(footer).toHaveAttribute('role', /contentinfo|complementary/);
      
      // Verify shortcuts are in accessible format
      const shortcuts = page.locator('footer [aria-label*="shortcut"]');
      expect(await shortcuts.count()).toBeGreaterThan(0);
    });
  });
});
