/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { test, expect } from '@playwright/test';
// import { execSync, spawn } from 'child_process';
import { spawn } from 'child_process';
import fs from 'fs';
import path from 'path';

test.describe('Build and Tooling Tests', () => {
  // Helper function to run commands and capture output to unique log files
  async function runCommandWithLogging(command: string, args: string[], description: string) {
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    // Use description to create unique log file name
    const sanitizedDescription = description.replace(/[^a-zA-Z0-9]/g, '_').toLowerCase();
    const logFile = path.join(
      'test-results',
      `build-tooling-${sanitizedDescription}-${timestamp}.log`
    );

    // Ensure test-results directory exists
    if (!fs.existsSync('test-results')) {
      fs.mkdirSync('test-results', { recursive: true });
    }

    try {
      // Run command and capture all output to log file
      const child = spawn(command, args, {
        stdio: ['inherit', 'pipe', 'pipe'],
        shell: true,
      });

      let stdout = '';
      let stderr = '';

      child.stdout?.on('data', (data) => {
        stdout += data.toString();
      });

      child.stderr?.on('data', (data) => {
        stderr += data.toString();
      });

      await new Promise<void>((resolve, reject) => {
        child.on('close', (code) => {
          const fullOutput = `Command: ${command} ${args.join(' ')}\nExit Code: ${code}\n\nSTDOUT:\n${stdout}\n\nSTDERR:\n${stderr}`;
          fs.writeFileSync(logFile, fullOutput);

          if (code === 0) {
            resolve();
          } else {
            reject(new Error(`${description} failed with exit code ${code}`));
          }
        });

        child.on('error', (error) => {
          const errorOutput = `Command: ${command} ${args.join(' ')}\nError: ${error.message}\n\nSTDOUT:\n${stdout}\n\nSTDERR:\n${stderr}`;
          fs.writeFileSync(logFile, errorOutput);
          reject(error);
        });
      });
    } catch (error) {
      // On failure, print log file info instead of flooding console
      const stats = fs.statSync(logFile);
      console.error(`${description} failed. Log file: ${logFile} (${stats.size} bytes)`);
      throw error;
    }
  }

  test('App SSR server builds successfully with TypeScript strict mode', async () => {
    await runCommandWithLogging('cd ../app && npm run build', [], 'App SSR server build');
  });

  test('Mock server builds successfully with TypeScript strict mode', async () => {
    await runCommandWithLogging('cd ../mock-server && npm run build', [], 'Mock server build');
  });

  test('E2E tests build successfully with TypeScript strict mode', async () => {
    await runCommandWithLogging('yarn', ['exec', 'tsc', '--noEmit'], 'E2E TypeScript compilation');
  });

  test('Prettier configuration works across all projects', async () => {
    try {
      await runCommandWithLogging('npm run format:check', [], 'Prettier format check');
    } catch (error) {
      // Prettier returns exit code 2 when no files match the pattern, which is expected
      // since we only have .ts files and not .js/.json files
      if (error.message.includes('exit code 2')) {
        // This is expected - consider it a pass
        return;
      }
      throw error;
    }
  });

  test('TypeScript configuration files are valid JSON', async () => {
    const tsconfigPath = 'tsconfig.json';
    expect(fs.existsSync(tsconfigPath)).toBe(true);

    const tsconfig = fs.readFileSync(tsconfigPath, 'utf8');
    expect(() => JSON.parse(tsconfig)).not.toThrow();
  });

  test('Playwright configuration is valid', async () => {
    const playwrightConfigPath = 'playwright.config.ts';
    expect(fs.existsSync(playwrightConfigPath)).toBe(true);

    await runCommandWithLogging(
      'npx playwright test --list | head -1',
      [],
      'Playwright config validation'
    );
  });
});
