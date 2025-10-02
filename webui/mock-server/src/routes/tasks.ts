import express from 'express';
import { scenarioReplayer } from './sessions.js';
import { logger } from '../index.js';

const router = express.Router();

// Mock session storage (shared with sessions route)
// Initial mock sessions for testing - exactly 5 sessions as per PRD:
// - 3 completed sessions
// - 2 active (running) sessions with continuous SSE event streams
let mockSessions: any[] = [
  // Completed Session 1
  {
    id: '01HVZ6K9T1N8S6M3V3Q3F0X1',
    tenantId: 'default',
    projectId: 'default',
    status: 'completed',
    createdAt: new Date(Date.now() - 7200000).toISOString(), // 2 hours ago
    completedAt: new Date(Date.now() - 3600000).toISOString(), // 1 hour ago
    prompt: 'Implement user authentication with email/password',
    repo: {
      mode: 'git',
      url: 'https://github.com/user/my-app',
      branch: 'main',
    },
    runtime: {
      type: 'devcontainer',
    },
    agent: {
      type: 'claude-code',
      version: 'latest',
    },
    delivery: {
      mode: 'pr',
      prUrl: 'https://github.com/user/my-app/pull/123',
    },
    labels: {},
    webhooks: [],
    links: {
      self: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X1',
      events: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X1/events',
      logs: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X1/logs',
    },
  },
  // Completed Session 2
  {
    id: '01HVZ6K9T1N8S6M3V3Q3F0X2',
    tenantId: 'default',
    projectId: 'default',
    status: 'completed',
    createdAt: new Date(Date.now() - 14400000).toISOString(), // 4 hours ago
    completedAt: new Date(Date.now() - 10800000).toISOString(), // 3 hours ago
    prompt: 'Add payment processing with Stripe integration',
    repo: {
      mode: 'git',
      url: 'https://github.com/user/e-commerce',
      branch: 'develop',
    },
    runtime: {
      type: 'devcontainer',
    },
    agent: {
      type: 'openhands',
      version: 'latest',
    },
    delivery: {
      mode: 'pr',
      prUrl: 'https://github.com/user/e-commerce/pull/456',
    },
    labels: {},
    webhooks: [],
    links: {
      self: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X2',
      events: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X2/events',
      logs: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X2/logs',
    },
  },
  // Completed Session 3
  {
    id: '01HVZ6K9T1N8S6M3V3Q3F0X3',
    tenantId: 'default',
    projectId: 'default',
    status: 'completed',
    createdAt: new Date(Date.now() - 21600000).toISOString(), // 6 hours ago
    completedAt: new Date(Date.now() - 18000000).toISOString(), // 5 hours ago
    prompt: 'Fix responsive design issues on mobile devices',
    repo: {
      mode: 'git',
      url: 'https://github.com/user/frontend',
      branch: 'hotfix/mobile-layout',
    },
    runtime: {
      type: 'local',
    },
    agent: {
      type: 'claude-code',
      version: 'latest',
    },
    delivery: {
      mode: 'branch',
    },
    labels: {},
    webhooks: [],
    links: {
      self: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X3',
      events: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X3/events',
      logs: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X3/logs',
    },
  },
  // Active Session 1 (running) - with continuous SSE events
  {
    id: '01HVZ6K9T1N8S6M3V3Q3F0X4',
    tenantId: 'default',
    projectId: 'default',
    status: 'running',
    createdAt: new Date(Date.now() - 1800000).toISOString(), // 30 minutes ago
    prompt: 'Refactor database queries for better performance',
    repo: {
      mode: 'git',
      url: 'https://github.com/user/backend-api',
      branch: 'feature/db-optimization',
    },
    runtime: {
      type: 'devcontainer',
    },
    agent: {
      type: 'openhands',
      version: 'latest',
    },
    delivery: {
      mode: 'pr',
    },
    labels: {},
    webhooks: [],
    links: {
      self: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X4',
      events: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X4/events',
      logs: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X4/logs',
    },
    // Pre-populate last 3 events for SSR
    recent_events: [
      {
        sessionId: '01HVZ6K9T1N8S6M3V3Q3F0X4',
        thought: 'Analyzing query performance in database layer',
        ts: new Date(Date.now() - 300000).toISOString(),
      },
      {
        sessionId: '01HVZ6K9T1N8S6M3V3Q3F0X4',
        tool_name: 'search_codebase',
        tool_output: 'Found 12 slow queries',
        tool_status: 'success',
        ts: new Date(Date.now() - 240000).toISOString(),
      },
      {
        sessionId: '01HVZ6K9T1N8S6M3V3Q3F0X4',
        file_path: 'src/db/queries.ts',
        lines_added: 15,
        lines_removed: 8,
        diff_preview: '+  queryBuilder.where({ status: "active" })',
        ts: new Date(Date.now() - 180000).toISOString(),
      },
    ],
  },
  // Active Session 2 (running) - with continuous SSE events
  {
    id: '01HVZ6K9T1N8S6M3V3Q3F0X5',
    tenantId: 'default',
    projectId: 'default',
    status: 'running',
    createdAt: new Date(Date.now() - 600000).toISOString(), // 10 minutes ago
    prompt: 'Write comprehensive E2E tests for the checkout flow',
    repo: {
      mode: 'git',
      url: 'https://github.com/user/e-commerce',
      branch: 'feature/e2e-tests',
    },
    runtime: {
      type: 'local',
    },
    agent: {
      type: 'claude-code',
      version: 'latest',
    },
    delivery: {
      mode: 'branch',
    },
    labels: {},
    webhooks: [],
    links: {
      self: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X5',
      events: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X5/events',
      logs: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X5/logs',
    },
    // Pre-populate last 3 events for SSR
    recent_events: [
      {
        sessionId: '01HVZ6K9T1N8S6M3V3Q3F0X5',
        tool_name: 'read_file',
        tool_output: 'File read successfully (342 lines)',
        tool_status: 'success',
        ts: new Date(Date.now() - 200000).toISOString(),
      },
      {
        sessionId: '01HVZ6K9T1N8S6M3V3Q3F0X5',
        thought: 'Planning test scenarios for checkout flow',
        ts: new Date(Date.now() - 150000).toISOString(),
      },
      {
        sessionId: '01HVZ6K9T1N8S6M3V3Q3F0X5',
        file_path: 'tests/e2e/checkout.spec.ts',
        lines_added: 45,
        lines_removed: 0,
        diff_preview: '+  test("completes checkout with valid payment", async ({ page }) => {',
        ts: new Date(Date.now() - 100000).toISOString(),
      },
    ],
  },
];

// POST /api/v1/tasks - Create Task/Session
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

    // Check required fields
    const requiredFields = ['prompt', 'repo', 'agent', 'runtime'];
    const missingFields = requiredFields.filter((field) => !req.body[field]);

    if (missingFields.length > 0) {
      return res.status(400).json({
        type: 'https://docs.example.com/errors/validation-error',
        title: 'Validation Error',
        status: 400,
        detail: `Missing required fields: ${missingFields.join(', ')}`,
        errors: missingFields.map((field) => ({
          field,
          message: `${field} is required`,
        })),
      });
    }

    // Generate a unique session ID
    const sessionId = `01HVZ6K9T1N8S6M3V3Q3F0X${Math.random().toString(36).substr(2, 9).toUpperCase()}`;

    // Create session from task data
    const session: any = {
      id: sessionId,
      tenantId: req.body.tenantId || 'default',
      projectId: req.body.projectId || 'default',
      status: 'running', // Start as running for testing
      createdAt: new Date().toISOString(),
      prompt: req.body.prompt,
      repo: req.body.repo,
      runtime: req.body.runtime,
      workspace: req.body.workspace,
      agent: req.body.agent,
      delivery: req.body.delivery,
      labels: req.body.labels || {},
      webhooks: req.body.webhooks || [],
      links: {
        self: `/api/v1/sessions/${sessionId}`,
        events: `/api/v1/sessions/${sessionId}/events`,
        logs: `/api/v1/sessions/${sessionId}/logs`,
      },
      metadata: {},
    };

    // Add to mock storage
    mockSessions.push(session);

    // Assign a scenario to this session for realistic event streaming
    // Use test_scenario for prompts containing "testing" to enable fast E2E tests
    // For manual testing, cycle through 5 main scenarios
    let assignedScenario;
    if (req.body.prompt && req.body.prompt.toLowerCase().includes('testing')) {
      assignedScenario = scenarioReplayer.assignSpecificScenario(session.id, 'test_scenario');
    } else {
      assignedScenario = scenarioReplayer.assignNextManualScenario(session.id);
    }

    // Add scenario metadata for testing purposes
    if (assignedScenario) {
      session.metadata = session.metadata || {};
      session.metadata.scenario = assignedScenario;
    }

    // Return response as per REST spec
    res.status(201).json({
      id: session.id,
      status: session.status,
      links: session.links,
    });
  } catch (error) {
    logger.error('Error in POST /api/v1/tasks:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Export the mock sessions for use in sessions route
export { mockSessions };
export const tasksRouter = router;
