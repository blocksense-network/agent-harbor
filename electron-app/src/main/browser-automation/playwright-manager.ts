/**
 * Playwright Manager
 * 
 * Manages Playwright browser instances and persistent contexts for agent automation.
 * Uses Electron's bundled Chromium for consistent automation across platforms.
 * 
 * See: specs/Public/Browser-Automation/README.md
 */

import { chromium, Browser, BrowserContext, LaunchOptions } from 'playwright';
import path from 'node:path';
import { app } from 'electron';

interface BrowserProfile {
  name: string;
  userDataDir: string;
  loginExpectations?: {
    origins: string[];
    username?: string;
  };
}

/**
 * Manages browser automation via Playwright
 */
export class PlaywrightManager {
  private browser: Browser | null = null;
  private contexts: Map<string, BrowserContext> = new Map();

  /**
   * Launch a Playwright browser instance
   * 
   * @param headless - Whether to run in headless mode (default: true)
   */
  async launchBrowser(headless: boolean = true): Promise<Browser> {
    if (this.browser) {
      return this.browser;
    }

    const launchOptions: LaunchOptions = {
      headless,
      // Use Electron's Chromium for consistent automation
      // Note: This may require additional configuration depending on Playwright version
      // For now, use the default Chromium channel
      channel: 'chromium',
    };

    this.browser = await chromium.launch(launchOptions);
    return this.browser;
  }

  /**
   * Create or retrieve a persistent browser context for a profile
   * 
   * @param profile - Browser profile configuration
   * @param headless - Whether to run in headless mode
   */
  async getOrCreateContext(
    profile: BrowserProfile,
    headless: boolean = true
  ): Promise<BrowserContext> {
    const existingContext = this.contexts.get(profile.name);
    if (existingContext) {
      return existingContext;
    }

    // Launch persistent context with profile's user data directory
    const context = await chromium.launchPersistentContext(profile.userDataDir, {
      headless,
      viewport: { width: 1280, height: 720 },
      // Additional options for automation stability
      ignoreHTTPSErrors: true,
    });

    this.contexts.set(profile.name, context);
    return context;
  }

  /**
   * Get the user data directory for agent browser profiles
   */
  getProfilesDir(): string {
    return path.join(app.getPath('userData'), 'browser-profiles');
  }

  /**
   * Close a specific browser context
   */
  async closeContext(profileName: string): Promise<void> {
    const context = this.contexts.get(profileName);
    if (context) {
      await context.close();
      this.contexts.delete(profileName);
    }
  }

  /**
   * Close all browser contexts and the browser instance
   */
  async closeAll(): Promise<void> {
    // Close all contexts
    for (const [_name, context] of this.contexts) {
      await context.close();
    }
    this.contexts.clear();

    // Close browser
    if (this.browser) {
      await this.browser.close();
      this.browser = null;
    }
  }
}

// Singleton instance
let playwrightManager: PlaywrightManager | null = null;

/**
 * Get the singleton Playwright manager instance
 */
export function getPlaywrightManager(): PlaywrightManager {
  if (!playwrightManager) {
    playwrightManager = new PlaywrightManager();
  }
  return playwrightManager;
}
