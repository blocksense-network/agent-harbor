/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { playwrightManager } from './playwright-manager';

/**
 * Basic browser automation test harness
 * Demonstrates Playwright integration with Electron's Chromium
 */
export class BrowserAutomationTestHarness {
  async runBasicTest(): Promise<{ success: boolean; message: string }> {
    try {
      console.log('Starting browser automation test...');

      // Initialize Playwright manager
      await playwrightManager.initialize();

      // Create a test context
      const context = await playwrightManager.createContext({
        headless: true,
      });

      // Create a new page
      const page = await context.newPage();

      // Navigate to a simple test page
      await page.goto('data:text/html,<html><body><h1>Test Page</h1><p>Browser automation is working!</p></body></html>');

      // Verify content
      const title = await page.title();
      const heading = await page.locator('h1').textContent();

      // Clean up
      await page.close();
      await playwrightManager.closeContext(context.toString());

      if (title === '' && heading === 'Test Page') {
        return { success: true, message: 'Browser automation test passed' };
      } else {
        return { success: false, message: `Test failed: expected title and heading, got title="${title}", heading="${heading}"` };
      }

    } catch (error) {
      console.error('Browser automation test failed:', error);
      return { success: false, message: `Test failed with error: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  async runProfileTest(): Promise<{ success: boolean; message: string }> {
    try {
      console.log('Starting profile management test...');

      // List existing profiles
      const initialProfiles = await playwrightManager.listProfiles();
      console.log(`Found ${initialProfiles.length} existing profiles`);

      // Create a test profile
      const testProfile = await playwrightManager.createProfile('Test Profile', ['https://example.com']);
      console.log(`Created test profile: ${testProfile.id}`);

      // List profiles again
      const updatedProfiles = await playwrightManager.listProfiles();
      console.log(`Now found ${updatedProfiles.length} profiles`);

      // Verify profile was created
      const foundProfile = updatedProfiles.find(p => p.id === testProfile.id);
      if (!foundProfile) {
        return { success: false, message: 'Profile creation test failed: profile not found after creation' };
      }

      // Clean up - delete test profile
      const deleted = await playwrightManager.deleteProfile(testProfile.id);
      if (!deleted) {
        console.warn('Failed to delete test profile, but test passed');
      }

      return { success: true, message: 'Profile management test passed' };

    } catch (error) {
      console.error('Profile management test failed:', error);
      return { success: false, message: `Test failed with error: ${error instanceof Error ? error.message : String(error)}` };
    }
  }

  async runAllTests(): Promise<{ passed: number; failed: number; results: Array<{ test: string; success: boolean; message: string }> }> {
    const results = [];

    // Run basic browser test
    const basicResult = await this.runBasicTest();
    results.push({ test: 'Basic Browser Automation', ...basicResult });

    // Run profile test
    const profileResult = await this.runProfileTest();
    results.push({ test: 'Profile Management', ...profileResult });

    const passed = results.filter(r => r.success).length;
    const failed = results.filter(r => !r.success).length;

    console.log(`\nTest Results: ${passed} passed, ${failed} failed`);

    return { passed, failed, results };
  }
}

// Export singleton instance
export const testHarness = new BrowserAutomationTestHarness();
