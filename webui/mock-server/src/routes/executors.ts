/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import express from 'express';

const router = express.Router();

// GET /api/v1/executors - List execution hosts
router.get('/', (req, res) => {
  res.json({
    items: [
      {
        id: 'executor-linux-01',
        os: 'linux',
        arch: 'x86_64',
        snapshotCapabilities: ['zfs', 'btrfs', 'overlay', 'copy'],
        status: 'online',
      },
      {
        id: 'executor-macos-01',
        os: 'macos',
        arch: 'arm64',
        snapshotCapabilities: ['overlay', 'copy'],
        status: 'online',
      },
    ],
  });
});

export { router as executorsRouter };
