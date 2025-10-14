import { Browser, BrowserContext, chromium } from 'playwright';
import { app } from 'electron';
import { join } from 'path';
import { existsSync, mkdirSync, readdirSync, statSync } from 'fs';

export interface BrowserProfile {
  id: string;
  name: string;
  loginExpectations: {
    origins: string[];
    username?: string;
  };
  createdAt: Date;
  lastUsedAt?: Date;
}

export interface BrowserContextOptions {
  profileId?: string;
  headless?: boolean;
  userAgent?: string;
}

export class PlaywrightManager {
  private browser: Browser | null = null;
  private contexts: Map<string, BrowserContext> = new Map();
  private profilesDir: string;

  constructor() {
    // Set up profiles directory in user data
    this.profilesDir = join(app.getPath('userData'), 'browser-profiles');
    this.ensureProfilesDir();
  }

  private ensureProfilesDir(): void {
    if (!existsSync(this.profilesDir)) {
      mkdirSync(this.profilesDir, { recursive: true });
    }
  }

  async initialize(): Promise<void> {
    if (this.browser) {
      return; // Already initialized
    }

    try {
      // Launch browser using Electron's Chromium
      // Note: In production, you might want to specify the executable path
      this.browser = await chromium.launch({
        headless: true, // Start headless by default
        args: [
          '--no-sandbox',
          '--disable-setuid-sandbox',
          '--disable-dev-shm-usage',
          '--disable-accelerated-2d-canvas',
          '--no-first-run',
          '--no-zygote',
          '--disable-gpu',
        ],
      });
    } catch (error) {
      console.error('Failed to launch Playwright browser:', error);
      throw error;
    }
  }

  async createContext(options: BrowserContextOptions = {}): Promise<BrowserContext> {
    if (!this.browser) {
      await this.initialize();
    }

    if (!this.browser) {
      throw new Error('Browser not initialized');
    }

    const contextId = `context_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

    try {
      // Determine user data directory for persistent context
      let userDataDir: string | undefined;
      if (options.profileId) {
        userDataDir = join(this.profilesDir, options.profileId);
        // Ensure profile directory exists
        if (!existsSync(userDataDir)) {
          mkdirSync(userDataDir, { recursive: true });
        }
      }

      const context = await this.browser.newContext({
        userAgent: options.userAgent,
        viewport: { width: 1280, height: 720 },
        // Use persistent context if profile specified
        ...(userDataDir && {
          storageState: join(userDataDir, 'storage.json'),
          // For persistent contexts, we need to use newPersistentContext instead
        }),
      });

      this.contexts.set(contextId, context);
      return context;
    } catch (error) {
      console.error('Failed to create browser context:', error);
      throw error;
    }
  }

  async closeContext(contextId: string): Promise<void> {
    const context = this.contexts.get(contextId);
    if (context) {
      await context.close();
      this.contexts.delete(contextId);
    }
  }

  getContext(contextId: string): BrowserContext | undefined {
    return this.contexts.get(contextId);
  }

  async listProfiles(): Promise<BrowserProfile[]> {
    const profiles: BrowserProfile[] = [];

    try {
      const entries = readdirSync(this.profilesDir);

      for (const entry of entries) {
        const profilePath = join(this.profilesDir, entry);
        const stat = statSync(profilePath);

        if (stat.isDirectory()) {
          const metadataPath = join(profilePath, 'metadata.json');

          if (existsSync(metadataPath)) {
            try {
              const metadata = require(metadataPath) as BrowserProfile;
              profiles.push(metadata);
            } catch (error) {
              console.warn(`Failed to read profile metadata for ${entry}:`, error);
            }
          } else {
            // Create basic profile metadata for directories without metadata
            profiles.push({
              id: entry,
              name: entry,
              loginExpectations: { origins: [] },
              createdAt: stat.birthtime,
              lastUsedAt: stat.mtime,
            });
          }
        }
      }
    } catch (error) {
      console.warn('Failed to list profiles:', error);
    }

    return profiles;
  }

  async createProfile(name: string, origins: string[] = []): Promise<BrowserProfile> {
    const profileId = `profile_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    const profilePath = join(this.profilesDir, profileId);

    if (!existsSync(profilePath)) {
      mkdirSync(profilePath, { recursive: true });
    }

    const profile: BrowserProfile = {
      id: profileId,
      name,
      loginExpectations: { origins },
      createdAt: new Date(),
    };

    // Save metadata
    const metadataPath = join(profilePath, 'metadata.json');
    require('fs').writeFileSync(metadataPath, JSON.stringify(profile, null, 2));

    return profile;
  }

  async deleteProfile(profileId: string): Promise<boolean> {
    const profilePath = join(this.profilesDir, profileId);

    if (existsSync(profilePath)) {
      try {
        require('fs').rmSync(profilePath, { recursive: true, force: true });
        return true;
      } catch (error) {
        console.error(`Failed to delete profile ${profileId}:`, error);
        return false;
      }
    }

    return false;
  }

  async shutdown(): Promise<void> {
    // Close all contexts
    for (const [contextId, context] of this.contexts) {
      try {
        await context.close();
      } catch (error) {
        console.warn(`Failed to close context ${contextId}:`, error);
      }
    }
    this.contexts.clear();

    // Close browser
    if (this.browser) {
      try {
        await this.browser.close();
      } catch (error) {
        console.warn('Failed to close browser:', error);
      }
      this.browser = null;
    }
  }

  isInitialized(): boolean {
    return this.browser !== null;
  }

  getBrowser(): Browser | null {
    return this.browser;
  }
}

// Export singleton instance
export const playwrightManager = new PlaywrightManager();
