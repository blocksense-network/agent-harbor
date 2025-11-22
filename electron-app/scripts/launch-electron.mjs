#!/usr/bin/env node
/**
 * Launch Electron directly using the binary path resolved through PnP
 * This avoids PnP resolution issues when launching the built app
 */

import { spawn } from 'child_process';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);

// Resolve Electron binary path using PnP
let electronPath;
try {
  electronPath = require('electron');
} catch (error) {
  console.error('Failed to resolve Electron binary:', error);
  process.exit(1);
}

// Get the path to the built main process
const args = process.argv.slice(2);
if (args.length === 0) {
  console.error('Usage: launch-electron.mjs <path-to-main.js>');
  process.exit(1);
}

console.log('Launching Electron...');
console.log('  Electron binary:', electronPath);
console.log('  Main process:', args[0]);

// Launch Electron with the built main process
const electronProcess = spawn(electronPath, args, {
  stdio: 'inherit',
  env: {
    ...process.env,
    // Ensure we don't try to use PnP in the Electron process
    NODE_OPTIONS: undefined,
  },
});

electronProcess.on('close', code => {
  process.exit(code || 0);
});

// Forward signals
process.on('SIGINT', () => {
  electronProcess.kill('SIGINT');
});

process.on('SIGTERM', () => {
  electronProcess.kill('SIGTERM');
});
