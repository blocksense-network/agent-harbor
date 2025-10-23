/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { test, expect } from '@playwright/test';

test.describe('API Contract Tests', () => {
  test.describe('Agents API', () => {
    test('GET /api/v1/agents returns correct schema', async ({ request }) => {
      const response = await request.get('/api/v1/agents');
      expect(response.ok()).toBeTruthy();

      const data = await response.json();
      expect(data).toHaveProperty('items');
      expect(Array.isArray(data.items)).toBe(true);

      // Check each agent has required properties
      data.items.forEach((agent: any) => {
        expect(agent).toHaveProperty('type');
        expect(typeof agent.type).toBe('string');
        expect(agent).toHaveProperty('versions');
        expect(Array.isArray(agent.versions)).toBe(true);
        expect(agent).toHaveProperty('settingsSchemaRef');
        expect(typeof agent.settingsSchemaRef).toBe('string');
      });
    });
  });

  test.describe('Runtimes API', () => {
    test('GET /api/v1/runtimes returns correct schema', async ({ request }) => {
      const response = await request.get('/api/v1/runtimes');
      expect(response.ok()).toBeTruthy();

      const data = await response.json();
      expect(data).toHaveProperty('items');
      expect(Array.isArray(data.items)).toBe(true);

      // Check each runtime has required properties
      data.items.forEach((runtime: any) => {
        expect(runtime).toHaveProperty('type');
        expect(typeof runtime.type).toBe('string');
      });
    });
  });

  test.describe('Executors API', () => {
    test('GET /api/v1/executors returns correct schema', async ({ request }) => {
      const response = await request.get('/api/v1/executors');
      expect(response.ok()).toBeTruthy();

      const data = await response.json();
      expect(data).toHaveProperty('items');
      expect(Array.isArray(data.items)).toBe(true);

      // Check each executor has required properties
      data.items.forEach((executor: any) => {
        expect(executor).toHaveProperty('id');
        expect(executor).toHaveProperty('os');
        expect(executor).toHaveProperty('arch');
        expect(executor).toHaveProperty('snapshotCapabilities');
        expect(Array.isArray(executor.snapshotCapabilities)).toBe(true);
      });
    });
  });

  test.describe('Sessions API', () => {
    test('GET /api/v1/sessions returns correct schema', async ({ request }) => {
      const response = await request.get('/api/v1/sessions');
      expect(response.ok()).toBeTruthy();

      const data = await response.json();
      expect(data).toHaveProperty('items');
      expect(Array.isArray(data.items)).toBe(true);
      expect(data).toHaveProperty('pagination');
      expect(data.pagination).toHaveProperty('page');
      expect(data.pagination).toHaveProperty('perPage');
      expect(data.pagination).toHaveProperty('total');
      expect(data.pagination).toHaveProperty('totalPages');
    });

    test('GET /api/v1/sessions with filters works', async ({ request }) => {
      const response = await request.get('/api/v1/sessions?status=running&page=1&perPage=10');
      expect(response.ok()).toBeTruthy();

      const data = await response.json();
      expect(data).toHaveProperty('items');
      expect(data).toHaveProperty('pagination');
    });

    test('POST /api/v1/tasks creates session correctly', async ({ request }) => {
      const taskData = {
        prompt: 'Test task creation',
        repo: {
          mode: 'git',
          url: 'https://github.com/test/repo.git',
          branch: 'main',
        },
        agent: {
          type: 'claude-code',
          version: 'latest',
        },
        runtime: {
          type: 'devcontainer',
        },
      };

      const response = await request.post('/api/v1/tasks', {
        headers: {
          'Content-Type': 'application/json',
        },
        data: taskData,
      });

      expect(response.status()).toBe(201);

      const data = await response.json();
      expect(data).toHaveProperty('id');
      expect(data).toHaveProperty('status');
      expect(data).toHaveProperty('links');
      expect(data.links).toHaveProperty('self');
      expect(data.links).toHaveProperty('events');
      expect(data.links).toHaveProperty('logs');
    });

    test('GET /api/v1/sessions/:id returns session details', async ({ request }) => {
      // First create a session
      const taskData = {
        prompt: 'Test session retrieval',
        repo: {
          mode: 'git',
          url: 'https://github.com/test/repo.git',
          branch: 'main',
        },
        agent: {
          type: 'claude-code',
          version: 'latest',
        },
        runtime: {
          type: 'devcontainer',
        },
      };

      const createResponse = await request.post('/api/v1/tasks', {
        data: taskData,
      });
      const createData = await createResponse.json();
      const sessionId = createData.id;

      // Now retrieve the session
      const response = await request.get(`/api/v1/sessions/${sessionId}`);
      expect(response.ok()).toBeTruthy();

      const session = await response.json();
      expect(session).toHaveProperty('id', sessionId);
      expect(session).toHaveProperty('status');
      expect(session).toHaveProperty('createdAt');
      expect(session).toHaveProperty('links');
    });

    test('GET /api/v1/sessions/:id returns 404 for non-existent session', async ({ request }) => {
      const response = await request.get('/api/v1/sessions/non-existent-id');
      expect(response.status()).toBe(404);

      const error = await response.json();
      expect(error).toHaveProperty('type');
      expect(error).toHaveProperty('title');
      expect(error).toHaveProperty('status', 404);
      expect(error).toHaveProperty('detail');
    });

    test('POST /api/v1/sessions/:id/stop works correctly', async ({ request }) => {
      // First create a session
      const taskData = {
        prompt: 'Test session stop',
        repo: {
          mode: 'git',
          url: 'https://github.com/test/repo.git',
          branch: 'main',
        },
        agent: {
          type: 'claude-code',
          version: 'latest',
        },
        runtime: {
          type: 'devcontainer',
        },
      };

      const createResponse = await request.post('/api/v1/tasks', {
        data: taskData,
      });
      const createData = await createResponse.json();
      const sessionId = createData.id;

      // Now stop the session
      const response = await request.post(`/api/v1/sessions/${sessionId}/stop`);
      expect(response.ok()).toBeTruthy();

      const data = await response.json();
      expect(data).toHaveProperty('status', 'accepted');
    });

    test('DELETE /api/v1/sessions/:id cancels session', async ({ request }) => {
      // First create a session
      const taskData = {
        prompt: 'Test session cancellation',
        repo: {
          mode: 'git',
          url: 'https://github.com/test/repo.git',
          branch: 'main',
        },
        agent: {
          type: 'claude-code',
          version: 'latest',
        },
        runtime: {
          type: 'devcontainer',
        },
      };

      const createResponse = await request.post('/api/v1/tasks', {
        data: taskData,
      });
      const createData = await createResponse.json();
      const sessionId = createData.id;

      // Now cancel the session
      const response = await request.delete(`/api/v1/sessions/${sessionId}`);
      expect(response.status()).toBe(204);

      // Verify session is gone
      const getResponse = await request.get(`/api/v1/sessions/${sessionId}`);
      expect(getResponse.status()).toBe(404);
    });

    test('GET /api/v1/sessions/:id/logs returns logs', async ({ request }) => {
      // First create a session
      const taskData = {
        prompt: 'Test logs retrieval',
        repo: {
          mode: 'git',
          url: 'https://github.com/test/repo.git',
          branch: 'main',
        },
        agent: {
          type: 'claude-code',
          version: 'latest',
        },
        runtime: {
          type: 'devcontainer',
        },
      };

      const createResponse = await request.post('/api/v1/tasks', {
        data: taskData,
      });
      const createData = await createResponse.json();
      const sessionId = createData.id;

      // Get logs
      const response = await request.get(`/api/v1/sessions/${sessionId}/logs`);
      expect(response.ok()).toBeTruthy();

      const data = await response.json();
      expect(data).toHaveProperty('logs');
      expect(Array.isArray(data.logs)).toBe(true);

      // Check log structure
      if (data.logs.length > 0) {
        const log = data.logs[0];
        expect(log).toHaveProperty('level');
        expect(log).toHaveProperty('message');
        expect(log).toHaveProperty('ts');
      }
    });

    test('GET /api/v1/sessions/:id/events establishes SSE stream', async ({ request }) => {
      // First create a session
      const taskData = {
        prompt: 'Test SSE events',
        repo: {
          mode: 'git',
          url: 'https://github.com/test/repo.git',
          branch: 'main',
        },
        agent: {
          type: 'claude-code',
          version: 'latest',
        },
        runtime: {
          type: 'devcontainer',
        },
      };

      const createResponse = await request.post('/api/v1/tasks', {
        headers: {
          'Content-Type': 'application/json',
        },
        data: taskData,
      });
      const createData = await createResponse.json();
      const sessionId = createData.id;

      // Test SSE endpoint (basic connectivity test)
      const response = await request.get(`/api/v1/sessions/${sessionId}/events`);

      expect(response.status()).toBe(200);
      expect(response.headers()['content-type']).toContain('text/event-stream');
      expect(response.headers()['cache-control']).toBe('no-cache');
    });
  });

  test.describe('Error Handling', () => {
    test('Invalid JSON returns 400 error', async ({ request }) => {
      const response = await request.post('/api/v1/tasks', {
        data: 'invalid json',
        headers: {
          'Content-Type': 'application/json',
        },
      });

      // Note: Express may handle this differently, but we test the pattern
      expect(response.status()).toBeGreaterThanOrEqual(400);
    });

    test('Missing required fields returns validation error', async ({ request }) => {
      const response = await request.post('/api/v1/tasks', {
        data: {},
      });

      expect(response.status()).toBeGreaterThanOrEqual(400);
    });
  });
});
