/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import express from 'express';

const router = express.Router();

// GET /api/v1/runtimes - List available runtime kinds
router.get('/', (req, res) => {
  res.json({
    items: [
      {
        type: 'devcontainer',
        images: ['ghcr.io/acme/base:latest'],
        paths: ['.devcontainer/devcontainer.json'],
      },
      {
        type: 'local',
        sandboxProfiles: ['default', 'disabled'],
      },
    ],
  });
});

export { router as runtimesRouter };
