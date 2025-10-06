import { defineConfig, devices } from '@playwright/test';

/**
 * @see https://playwright.dev/docs/test-configuration
 */
export default defineConfig({
  testDir: './tests',
  /* Run tests in files in parallel */
  fullyParallel: true,
  /* Fail the build on CI if you accidentally left test.only in the source code. */
  forbidOnly: !!process.env.CI,
  /* Retry on CI only */
  retries: process.env.CI ? 2 : 0,
  /* Opt out of parallel tests on CI. */
  workers: process.env.CI ? 1 : undefined,
  /* Reporter to use. See https://playwright.dev/docs/test-reporters */
  reporter: [
    ['./reporters/minimal-reporter'], // Custom minimal reporter following AGENTS.md guidelines
    ['junit', { outputFile: 'results.xml' }], // Keep JUnit for CI/CD integration
  ],
  /* Shared settings for all the projects below. See https://playwright.dev/docs/api/class-testoptions. */
  use: {
    /* Base URL to use in actions like `await page.goto('/')`. */
    baseURL: process.env.BASE_URL || 'http://localhost:3002',

    /* Collect trace when retrying the failed test. See https://playwright.dev/docs/trace-viewer */
    trace: 'on-first-retry',

    /* Run tests in headless mode to prevent browser windows from opening */
    headless: true,

    /* Capture all browser activity to log files per AGENTS.md guidelines */
    screenshot: 'only-on-failure',
    video: 'off',
  },

  /* Configure projects for different test types */
  projects: [
    {
      name: 'api-tests',
      testMatch: '**/api-contract.spec.ts',
      use: {
        baseURL: process.env.API_BASE_URL || 'http://localhost:3001',
      },
    },
    {
      name: 'build-tooling-tests',
      testMatch: '**/build-tooling.spec.ts',
      use: {
        baseURL: process.env.BASE_URL || 'http://localhost:3002',
      },
    },
    {
      name: 'infrastructure-tests',
      testMatch: '**/example.spec.ts',
      use: {
        baseURL: process.env.BASE_URL || 'http://localhost:3002',
      },
    },
    {
      name: 'browser-tests',
      use: {
        browserName: 'chromium',
        ...devices['Desktop Chrome'],
        baseURL: process.env.BASE_URL || 'http://localhost:3002',
        // Force use of Nix browser (platform-specific path set by Nix devshell)
        executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE
      },
      testMatch: [
        '**/layout-navigation.spec.ts',
        '**/accessibility.spec.ts',
        '**/localstorage-persistence.spec.ts',
        '**/draft-debug.spec.ts',
        '**/keyboard-navigation.spec.ts',
        '**/session-interactions.spec.ts',
        '**/session-management.spec.ts',
        '**/task-creation.spec.ts',
        '**/task-creation-flow.spec.ts',
        '**/sse-live-updates.spec.ts',
        '**/tom-select-customization.spec.ts',
        '**/critical-bugs.spec.ts',
        '**/sse-debug.spec.ts',
        '**/tom-select-direction.spec.ts',
        '**/tomselect-debug.spec.ts',
        '**/fixed-height-cards.spec.ts',
        '**/draft-keyboard-navigation.spec.ts',
        '**/focus-management.spec.ts',
        '**/focus-blur-scrolling.spec.ts',
        '**/reporter-validation.spec.ts',
        '**/simple-test.spec.ts',
        '**/tom-select-upward-positioning.spec.ts',
      ]
    },
  ],

  /* Note: Servers are started via start-servers.sh script for reliable orchestration */
});
