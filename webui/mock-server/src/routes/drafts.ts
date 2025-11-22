/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import express from 'express';
import { logger } from '../index.js';

const router = express.Router();

// Mock draft storage - initialize with one empty draft for testing
const mockDrafts: any[] = [
  {
    id: 'draft-default',
    prompt: '',
    repo: {
      mode: 'git',
      url: '',
      branch: 'main',
    },
    agents: [],
    runtime: {
      type: 'devcontainer',
    },
    delivery: {
      mode: 'pr',
    },
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  },
];

// GET /api/v1/drafts - List user's draft tasks
router.get('/', (req, res) => {
  try {
    logger.log(`[DRAFTS] GET /api/v1/drafts - Returning ${mockDrafts.length} drafts`);
    mockDrafts.forEach(d => logger.log(`  - ${d.id}: prompt="${d.prompt}"`));
    // Return all drafts (in a real implementation, this would be filtered by user)
    res.status(200).json({
      items: mockDrafts,
    });
  } catch (error) {
    logger.error('Error in GET /api/v1/drafts:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// POST /api/v1/drafts - Create a new draft task
router.post('/', (req, res) => {
  try {
    // Basic validation
    if (!req.body || typeof req.body !== 'object') {
      return res.status(400).json({
        type: 'https://docs.example.com/errors/bad-request',
        title: 'Bad Request',
        status: 400,
        detail: 'Request body must be a valid JSON object',
      });
    }

    // Generate a unique draft ID
    const draftId = `draft-01HVZ6K9T1N8S6M3V3Q3F0X${Math.random().toString(36).substr(2, 9).toUpperCase()}`;

    const now = new Date().toISOString();

    // Create draft from request data
    const draft: any = {
      id: draftId,
      prompt: req.body.prompt || '',
      repo: req.body.repo || {
        mode: 'git',
        url: '',
        branch: 'main',
      },
      agent: req.body.agent || {
        type: 'claude-code',
        version: 'latest',
      },
      runtime: req.body.runtime || {
        type: 'devcontainer',
      },
      delivery: req.body.delivery || {
        mode: 'pr',
      },
      createdAt: now,
      updatedAt: now,
    };

    // Add to mock storage
    mockDrafts.push(draft);

    // Return response as per REST spec
    res.status(201).json({
      id: draft.id,
      createdAt: draft.createdAt,
      updatedAt: draft.updatedAt,
    });
  } catch (error) {
    logger.error('Error in POST /api/v1/drafts:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// PUT /api/v1/drafts/{id} - Update a draft task
router.put('/:id', (req, res) => {
  try {
    const { id } = req.params;

    // Find the draft
    const draftIndex = mockDrafts.findIndex(draft => draft.id === id);
    if (draftIndex === -1) {
      return res.status(404).json({
        type: 'https://docs.example.com/errors/not-found',
        title: 'Not Found',
        status: 404,
        detail: `Draft with id ${id} not found`,
      });
    }

    // Basic validation
    if (!req.body || typeof req.body !== 'object') {
      return res.status(400).json({
        type: 'https://docs.example.com/errors/bad-request',
        title: 'Bad Request',
        status: 400,
        detail: 'Request body must be a valid JSON object',
      });
    }

    // Update the draft
    const updatedDraft = {
      ...mockDrafts[draftIndex],
      ...req.body,
      id, // Ensure ID doesn't change
      updatedAt: new Date().toISOString(),
    };

    mockDrafts[draftIndex] = updatedDraft;

    // Return the updated draft
    res.status(200).json(updatedDraft);
  } catch (error) {
    logger.error('Error in PUT /api/v1/drafts/:id:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// DELETE /api/v1/drafts/{id} - Delete a draft task
router.delete('/:id', (req, res) => {
  try {
    const { id } = req.params;

    // Find the draft
    const draftIndex = mockDrafts.findIndex(draft => draft.id === id);
    if (draftIndex === -1) {
      return res.status(404).json({
        type: 'https://docs.example.com/errors/not-found',
        title: 'Not Found',
        status: 404,
        detail: `Draft with id ${id} not found`,
      });
    }

    // Remove the draft
    mockDrafts.splice(draftIndex, 1);

    // Return 204 No Content
    res.status(204).send();
  } catch (error) {
    logger.error('Error in DELETE /api/v1/drafts/:id:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

export default router;
