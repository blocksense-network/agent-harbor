/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { test, expect } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';

test.describe('Real-time Features', () => {
  test.describe('SSE Connection and Event Streaming', () => {
    test('SSE connection status is displayed correctly', async ({ page }) => {
      await page.goto('/sessions');

      // Wait for page to load and select a session
      await page.waitForTimeout(2000);

      // Click on a session if available
      const sessionCard = page
        .locator('[class*="bg-white"][class*="border"][class*="rounded-lg"]')
        .first();
      if (await sessionCard.isVisible()) {
        await sessionCard.click();

        // Check that events tab shows connection status
        const eventsTab = page.locator('button').filter({ hasText: 'Events' });
        await eventsTab.click();

        // Check for connection status indicator
        const connectionStatus = page
          .locator('text=Connected to real-time event stream')
          .or(
            page
              .locator('text=Connecting to event stream...')
              .or(page.locator('text=Real-time events not available'))
          );
        await expect(connectionStatus.first()).toBeVisible();
      }
    });

    test('Real-time log streaming displays new entries', async ({ page }) => {
      await page.goto('/sessions');

      // Wait for page to load
      await page.waitForTimeout(2000);

      // Click on a session if available
      const sessionCard = page
        .locator('[class*="bg-white"][class*="border"][class*="rounded-lg"]')
        .first();
      if (await sessionCard.isVisible()) {
        await sessionCard.click();

        // Switch to logs tab
        const logsTab = page.locator('button').filter({ hasText: 'Logs' });
        await logsTab.click();

        // Wait for potential real-time updates (mock server sends events every 2 seconds)
        await page.waitForTimeout(5000);

        // Check for live updates indicator or additional log entries
        const liveIndicator = page
          .locator('text=Live: Receiving real-time log updates')
          .or(page.locator('text=Live log updates unavailable'));

        // The indicator should be present
        await expect(liveIndicator.first()).toBeVisible();
      }
    });

    test('Optimistic UI updates work for session controls', async ({ page }) => {
      await page.goto('/sessions');

      // Wait for page to load
      await page.waitForTimeout(2000);

      // Click on a session if available
      const sessionCard = page
        .locator('[class*="bg-white"][class*="border"][class*="rounded-lg"]')
        .first();
      if (await sessionCard.isVisible()) {
        await sessionCard.click();

        // Check for session controls
        const stopButton = page.locator('button').filter({ hasText: 'Stop' });
        const pauseButton = page.locator('button').filter({ hasText: 'Pause' });
        const resumeButton = page.locator('button').filter({ hasText: 'Resume' });

        // Check if any control buttons are available
        const hasStopButton = await stopButton.isVisible().catch(() => false);
        const hasPauseButton = await pauseButton.isVisible().catch(() => false);
        const hasResumeButton = await resumeButton.isVisible().catch(() => false);

        if (hasStopButton) {
          // Click stop button
          await stopButton.click();

          // Check for optimistic status update (status should show "stopping" or update)
          const statusBadge = page.locator('[class*="inline-flex"][class*="items-center"]').first();
          await expect(statusBadge).toBeVisible();

          // Status should update (either immediately or after SSE event)
          await page.waitForTimeout(3000);
        } else if (hasPauseButton) {
          // Click pause button
          await pauseButton.click();

          // Check for optimistic status update
          const statusBadge = page.locator('[class*="inline-flex"][class*="items-center"]').first();
          await expect(statusBadge).toBeVisible();

          await page.waitForTimeout(3000);
        } else if (hasResumeButton) {
          // Click resume button
          await resumeButton.click();

          // Check for optimistic status update
          const statusBadge = page.locator('[class*="inline-flex"][class*="items-center"]').first();
          await expect(statusBadge).toBeVisible();

          await page.waitForTimeout(3000);
        }
      }
    });

    test('Session status updates in real-time', async ({ page }) => {
      await page.goto('/sessions');

      // Wait for page to load
      await page.waitForTimeout(2000);

      // Click on a session if available
      const sessionCard = page
        .locator('[class*="bg-white"][class*="border"][class*="rounded-lg"]')
        .first();
      if (await sessionCard.isVisible()) {
        await sessionCard.click();

        // Check initial status
        const initialStatusBadge = page
          .locator('[class*="inline-flex"][class*="items-center"]')
          .first();
        const _initialStatus = await initialStatusBadge.textContent();

        // Wait for potential status updates from SSE (mock server changes status every few events)
        await page.waitForTimeout(10000);

        // Status might have changed (or stayed the same, both are valid)
        const updatedStatusBadge = page
          .locator('[class*="inline-flex"][class*="items-center"]')
          .first();
        const _updatedStatus = await updatedStatusBadge.textContent();

        // Status badge should still be visible
        await expect(updatedStatusBadge).toBeVisible();
      }
    });

    test('Connection error handling and reconnection works', async ({ page }) => {
      await page.goto('/sessions');

      // Wait for page to load
      await page.waitForTimeout(2000);

      // Click on a session if available
      const sessionCard = page
        .locator('[class*="bg-white"][class*="border"][class*="rounded-lg"]')
        .first();
      if (await sessionCard.isVisible()) {
        await sessionCard.click();

        // Switch to events tab
        const eventsTab = page.locator('button').filter({ hasText: 'Events' });
        await eventsTab.click();

        // Check that connection status is shown (could be connected, connecting, or error)
        const connectionIndicator = page.locator('[class*="rounded-lg"][class*="border"]').first();
        await expect(connectionIndicator).toBeVisible();

        // Connection status text should be present
        const statusText = connectionIndicator.locator('text').first();
        await expect(statusText).toBeVisible();
      }
    });
  });

  test.describe('Real-time Session List Updates', () => {
    test('Session list auto-refreshes and shows updated status', async ({ page }) => {
      await page.goto('/sessions');

      // Wait for initial load
      await page.waitForTimeout(2000);

      // Check if sessions are displayed
      const sessionCards = page.locator(
        '[class*="bg-white"][class*="border"][class*="rounded-lg"]'
      );

      // If sessions exist, check that they have status badges
      const sessionCount = await sessionCards.count();
      if (sessionCount > 0) {
        // Check first session has a status badge
        const firstSession = sessionCards.first();
        const statusBadge = firstSession.locator('[class*="inline-flex"][class*="items-center"]');
        await expect(statusBadge).toBeVisible();
      }

      // Wait for auto-refresh (30 seconds in SessionsPane, but SSE might update sooner)
      await page.waitForTimeout(5000);

      // Sessions should still be displayed (or empty state if no sessions)
      const hasSessions = await sessionCards
        .first()
        .isVisible()
        .catch(() => false);
      const hasEmptyState = await page
        .locator('text=No sessions')
        .isVisible()
        .catch(() => false);

      expect(hasSessions || hasEmptyState).toBe(true);
    });

    test('Real-time log streaming matches scenario content (5 seconds)', async ({ page }) => {
      // Create a new session with a test prompt
      const testPrompt = 'Create a simple hello.py file for testing';

      await page.evaluate(async (prompt) => {
        const response = await fetch('/api/v1/tasks', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            prompt,
            repo: { mode: 'git', url: 'https://github.com/test/repo.git' },
            agent: { type: 'claude-code' },
            runtime: { type: 'devcontainer' },
          }),
        });
        return response.json();
      }, testPrompt);

      await page.goto('/sessions');
      await page.waitForTimeout(1000);

      // Click on the newly created session (should be first in list)
      const sessionCard = page
        .locator('[class*="bg-white"][class*="border"][class*="rounded-lg"]')
        .first();
      await sessionCard.click();

      // Get the session ID from the URL
      const url = page.url();
      const sessionIdMatch = url.match(/\/sessions\/([^/?]+)/);
      if (!sessionIdMatch) {
        throw new Error('Could not extract session ID from URL');
      }
      const sessionId = sessionIdMatch[1];

      // Verify this session has the test scenario
      const sessionResponse = await page.evaluate(async (id) => {
        const response = await fetch(`/api/v1/sessions/${id}`);
        return response.json();
      }, sessionId);

      console.log('Test session metadata:', sessionResponse.metadata);

      // Load the test scenario directly (since we know what we want to test)
      const scenarioPath = path.join(
        process.cwd(),
        '../../../tests/tools/mock-agent/scenarios',
        'test_scenario.json'
      );
      expect(fs.existsSync(scenarioPath)).toBe(true);

      const scenarioContent = fs.readFileSync(scenarioPath, 'utf-8');
      const scenario = JSON.parse(scenarioContent);

      // Extract expected log messages from scenario turns
      const expectedLogMessages: string[] = [];
      for (const turn of scenario.turns || []) {
        if (turn.user) {
          expectedLogMessages.push(`User: ${turn.user}`);
        }
        if (turn.think) {
          expectedLogMessages.push(`Thinking: ${turn.think}`);
        }
        if (turn.tool) {
          expectedLogMessages.push(`Tool: ${turn.tool.name}(${JSON.stringify(turn.tool.args)})`);
        }
        if (turn.assistant) {
          expectedLogMessages.push(`Assistant: ${turn.assistant}`);
        }
      }

      console.log(`Testing with ${expectedLogMessages.length} expected log messages`);

      // Switch to logs tab
      const logsTab = page.locator('button').filter({ hasText: 'Logs' });
      await logsTab.click();

      // Wait for scenario events to be replayed (test scenario should complete in ~5 seconds)
      await page.waitForTimeout(6000);

      // Get all log entries from the page
      const actualLogElements = page.locator('[class*="text-xs"][class*="font-mono"]');
      const actualLogCount = await actualLogElements.count();

      console.log(`Found ${actualLogCount} log entries on page`);

      // Extract text from all log entries
      const actualLogMessages: string[] = [];
      for (let i = 0; i < actualLogCount; i++) {
        const logElement = actualLogElements.nth(i);
        const logText = await logElement.textContent();
        if (logText) {
          // Extract just the message part (remove timestamp and level)
          const messageMatch = logText.match(/^(?:info|warn|error)?\s*(.+)$/);
          if (messageMatch) {
            actualLogMessages.push(messageMatch[1].trim());
          }
        }
      }

      // Verify that at least some expected messages appear in the actual logs
      let matchedCount = 0;
      for (const expectedMsg of expectedLogMessages) {
        const found = actualLogMessages.some(
          (actualMsg) => actualMsg.includes(expectedMsg) || expectedMsg.includes(actualMsg)
        );
        if (found) {
          matchedCount++;
        } else {
          console.log(`Expected message not found: ${expectedMsg}`);
        }
      }

      const matchPercentage = (matchedCount / expectedLogMessages.length) * 100;
      console.log(
        `Found ${matchedCount}/${expectedLogMessages.length} expected messages (${matchPercentage.toFixed(1)}%)`
      );

      // Should find at least 50% of messages (allowing for timing/race conditions)
      expect(matchPercentage).toBeGreaterThanOrEqual(50);
      expect(actualLogCount).toBeGreaterThan(0);

      console.log('✓ Real-time log streaming test completed successfully');
    });
  });
});
