import { test, expect } from '@playwright/test';

/**
 * SSE Debugging Test
 * 
 * This test helps identify why SSE events arrive but UI doesn't update.
 */

test.describe('SSE Live Updates Debug', () => {
  test('Debug SSE event flow and UI updates', async ({ page }) => {
    const consoleLogs: string[] = [];
    const consoleErrors: string[] = [];
    
    // Capture all console logs
    page.on('console', msg => {
      const text = msg.text();
      consoleLogs.push(text);
      if (msg.type() === 'error') {
        consoleErrors.push(text);
      }
    });

    await page.goto('/');
    await page.waitForLoadState('load');

    // Find active session cards
    const activeCards = page.locator('[data-testid="task-card"]:has([aria-label="Status: running"])');
    const activeCount = await activeCards.count();
    
    console.log(`Found ${activeCount} active session cards`);
    
    if (activeCount > 0) {
      const firstActiveCard = activeCards.first();
      
      // Get initial text
      const initialText = await firstActiveCard.textContent();
      console.log('Initial card text:', initialText);
      
      // Wait for SSE subscription logs
      await page.waitForTimeout(1000);
      
      // Check for SSE subscription logs
      const subscriptionLogs = consoleLogs.filter(log => log.includes('[SessionCard] Subscribing to SSE'));
      console.log(`SSE subscription logs: ${subscriptionLogs.length}`);
      subscriptionLogs.forEach(log => console.log('  -', log));
      
      // Wait for SSE events (mock server sends every 2 seconds)
      await page.waitForTimeout(5000);
      
      // Check for SSE event logs
      const eventLogs = consoleLogs.filter(log => log.includes('SSE event received'));
      console.log(`\nSSE event logs: ${eventLogs.length}`);
      eventLogs.slice(0, 5).forEach(log => console.log('  -', log));
      
      // Check for state update logs (new patterns)
      const stateUpdateLogs = consoleLogs.filter(log => 
        log.includes('Added thinking row') || 
        log.includes('Added tool start row') || 
        log.includes('Updated tool last_line') ||
        log.includes('Tool completed') ||
        log.includes('Added file edit row')
      );
      console.log(`\nState update logs: ${stateUpdateLogs.length}`);
      stateUpdateLogs.slice(0, 5).forEach(log => console.log('  -', log));
      
      // Check for liveActivity logs (rows count)
      const activityLogs = consoleLogs.filter(log => log.includes('Live activity rows:'));
      console.log(`\nLive activity logs: ${activityLogs.length}`);
      activityLogs.slice(0, 3).forEach(log => console.log('  -', log));
      
      // Get current text
      const currentText = await firstActiveCard.textContent();
      console.log('\nCurrent card text:', currentText);
      
      // Check if text changed
      const textChanged = currentText !== initialText;
      console.log(`Text changed: ${textChanged}`);
      
      // Check for live activity elements
      const hasThinking = await firstActiveCard.locator('text=/Thoughts:/').count();
      const hasFileEdit = await firstActiveCard.locator('text=/File edits:/').count();
      const hasToolUsage = await firstActiveCard.locator('text=/Tool usage:/').count();
      const hasWaiting = await firstActiveCard.locator('text=/Waiting for agent activity/').count();
      
      console.log('\nLive activity elements:');
      console.log(`  - Thoughts: ${hasThinking}`);
      console.log(`  - File edits: ${hasFileEdit}`);
      console.log(`  - Tool usage: ${hasToolUsage}`);
      console.log(`  - Waiting: ${hasWaiting}`);
      
      // Check for errors
      console.log(`\nConsole errors: ${consoleErrors.length}`);
      consoleErrors.forEach(err => console.log('  ERROR:', err));
      
      // Analyze the issue
      console.log('\n=== ANALYSIS ===');
      if (eventLogs.length === 0) {
        console.log('ISSUE: SSE events are NOT arriving in the callback');
      } else if (stateUpdateLogs.length === 0) {
        console.log('ISSUE: SSE events arrive but rows are NOT being added/updated');
      } else if (activityLogs.length === 0) {
        console.log('ISSUE: Rows added but live activity not being logged');
      } else if (hasWaiting > 0 && (hasThinking === 0 && hasFileEdit === 0 && hasToolUsage === 0)) {
        console.log('ISSUE: Rows updated but UI shows "Waiting for agent activity"');
      } else if (textChanged && (hasThinking > 0 || hasFileEdit > 0 || hasToolUsage > 0)) {
        console.log('SUCCESS: Live updates are working! UI updated with activity.');
      } else {
        console.log('PARTIAL SUCCESS: Events arriving and state updating, but UI may not reflect changes yet');
      }
      
      // This test is for debugging, so we'll just report findings
      // Not making hard assertions to allow test to complete and show all logs
      expect(subscriptionLogs.length).toBeGreaterThan(0); // At least this should work
    } else {
      console.log('No active sessions found - skipping SSE test');
    }
  });
});
