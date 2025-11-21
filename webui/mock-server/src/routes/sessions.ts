/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import express from 'express';
import { mockSessions } from './tasks.js';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { setInterval, clearInterval, setTimeout, clearTimeout } from 'timers';
import { logger } from '../index.js';
import { scenarioRunner } from '../index.js';

// ES module equivalent of __dirname
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const router = express.Router();

interface ScenarioTurn {
  user?: string;
  think?: string;
  tool?: {
    name: string;
    args: Record<string, any>;
  };
  assistant?: string;
  shell?: {
    cmd: string;
  };
}

interface ScenarioMeta {
  instructions?: string;
  turn_context?: Record<string, any>;
}

interface Scenario {
  description?: string;
  meta?: ScenarioMeta;
  turns: ScenarioTurn[];
}

interface ScenarioEvent {
  type: 'status' | 'log' | 'progress';
  sessionId: string;
  status?: string;
  level?: string;
  message?: string;
  progress?: number;
  stage?: string;
  ts: string;
}

class ScenarioReplayer {
  private scenarios: Map<string, Scenario> = new Map();
  private sessionScenarios: Map<string, string> = new Map();

  constructor() {
    this.loadScenarios();
  }

  private loadScenarios() {
    const scenarioDirs = [
      path.join(__dirname, '../../../tests/tools/mock-agent/examples'),
      path.join(__dirname, '../../../tests/tools/mock-agent/scenarios'),
    ];

    for (const scenarioDir of scenarioDirs) {
      if (fs.existsSync(scenarioDir)) {
        const files = fs.readdirSync(scenarioDir).filter(f => f.endsWith('.json'));
        for (const file of files) {
          try {
            const filePath = path.join(scenarioDir, file);
            const content = fs.readFileSync(filePath, 'utf-8');
            const scenario: Scenario = JSON.parse(content);
            const scenarioName = path.basename(file, '.json');
            this.scenarios.set(scenarioName, scenario);
            logger.log(`Loaded scenario: ${scenarioName}`);
          } catch (error) {
            logger.error(`Failed to load scenario ${file}:`, error);
          }
        }
      }
    }
  }

  assignScenarioToSession(sessionId: string): string | null {
    const scenarioNames = Array.from(this.scenarios.keys());
    if (scenarioNames.length > 0) {
      const randomScenario = scenarioNames[Math.floor(Math.random() * scenarioNames.length)];
      this.sessionScenarios.set(sessionId, randomScenario);
      logger.log(`Assigned scenario '${randomScenario}' to session ${sessionId}`);
      return randomScenario;
    }
    return null;
  }

  assignSpecificScenario(sessionId: string, scenarioName: string): string | null {
    if (this.scenarios.has(scenarioName)) {
      this.sessionScenarios.set(sessionId, scenarioName);
      logger.log(`Assigned specific scenario '${scenarioName}' to session ${sessionId}`);
      return scenarioName;
    }
    logger.log(`Scenario '${scenarioName}' not found, falling back to random assignment`);
    return this.assignScenarioToSession(sessionId);
  }

  private manualScenarioIndex = 0;
  private readonly manualScenarios = [
    'bug_fix_scenario',
    'code_refactoring_scenario',
    'documentation_scenario',
    'feature_implementation_scenario',
    'testing_workflow_scenario',
  ];

  assignNextManualScenario(sessionId: string): string | null {
    const scenarioName = this.manualScenarios[this.manualScenarioIndex];
    this.manualScenarioIndex = (this.manualScenarioIndex + 1) % this.manualScenarios.length;

    if (this.scenarios.has(scenarioName)) {
      this.sessionScenarios.set(sessionId, scenarioName);
      logger.log(
        `Assigned manual scenario '${scenarioName}' to session ${sessionId} (next: ${this.manualScenarios[this.manualScenarioIndex]})`,
      );
      return scenarioName;
    }
    logger.log(`Manual scenario '${scenarioName}' not found, falling back to random assignment`);
    return this.assignScenarioToSession(sessionId);
  }

  getScenarioForSession(sessionId: string): Scenario | null {
    const scenarioName = this.sessionScenarios.get(sessionId);
    return scenarioName ? this.scenarios.get(scenarioName) || null : null;
  }

  replayScenario(sessionId: string, eventIndex: number): ScenarioEvent | null {
    const scenario = this.getScenarioForSession(sessionId);
    if (!scenario || !scenario.turns) {
      return null;
    }

    const turnIndex = Math.floor(eventIndex / 2); // Map events to turns (2 events per turn)
    if (turnIndex >= scenario.turns.length) {
      return null;
    }

    const turn = scenario.turns[turnIndex];
    const eventOffset = eventIndex % 2;

    // Convert scenario turn to events
    if (eventOffset === 0) {
      // First event: log the user/assistant message
      if (turn.user) {
        return {
          type: 'log',
          sessionId,
          level: 'info',
          message: `User: ${turn.user}`,
          ts: new Date().toISOString(),
        };
      } else if (turn.assistant) {
        return {
          type: 'log',
          sessionId,
          level: 'info',
          message: `Assistant: ${turn.assistant}`,
          ts: new Date().toISOString(),
        };
      } else if (turn.think) {
        return {
          type: 'log',
          sessionId,
          level: 'info',
          message: `Thinking: ${turn.think}`,
          ts: new Date().toISOString(),
        };
      }
    } else {
      // Second event: tool execution or status update
      if (turn.tool) {
        return {
          type: 'log',
          sessionId,
          level: 'info',
          message: `Tool: ${turn.tool.name}(${JSON.stringify(turn.tool.args)})`,
          ts: new Date().toISOString(),
        };
      } else {
        // Status/progress update
        return {
          type: 'status',
          sessionId,
          status: turnIndex === scenario.turns.length - 1 ? 'completed' : 'running',
          ts: new Date().toISOString(),
        };
      }
    }

    return null;
  }

  getAvailableScenarios(): string[] {
    return Array.from(this.scenarios.keys());
  }
}

// Global scenario replayer instance
const scenarioReplayer = new ScenarioReplayer();

// Add a default session for testing
if (mockSessions.length === 0) {
  // Create default session
  mockSessions.push({
    id: '01HVZ6K9T1N8S6M3V3Q3F0X5B7',
    tenantId: 'acme',
    projectId: 'storefront',
    status: 'running',
    createdAt: '2025-01-01T12:00:00Z',
    prompt: 'Default test session',
    repo: { mode: 'git', url: 'https://github.com/test/repo.git', branch: 'main' },
    agent: { type: 'claude-code', version: 'latest' },
    runtime: { type: 'devcontainer' },
    links: {
      self: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X5B7',
      events: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X5B7/events',
      logs: '/api/v1/sessions/01HVZ6K9T1N8S6M3V3Q3F0X5B7/logs',
    },
  });

  // Assign a scenario to the default session too
  const assignedScenario = scenarioReplayer.assignScenarioToSession('01HVZ6K9T1N8S6M3V3Q3F0X5B7');
  if (assignedScenario) {
    const defaultSession = mockSessions.find(s => s.id === '01HVZ6K9T1N8S6M3V3Q3F0X5B7');
    if (defaultSession) {
      defaultSession.metadata = { scenario: assignedScenario };
    }
  }
}

// Sessions creation is now handled by POST /api/v1/tasks

// GET /api/v1/sessions - List Sessions
router.get('/', (req, res) => {
  const { status, projectId, page = 1, perPage = 20 } = req.query;

  let filteredSessions = [...mockSessions];

  // Add scenario sessions if scenario runner is active
  if (scenarioRunner) {
    const scenarioSessions = scenarioRunner.getActiveSessions().map(session => ({
      id: session.id,
      tenantId: 'scenario',
      projectId: 'scenario-project',
      status: session.status,
      createdAt: session.startTime.toISOString(),
      prompt: `Scenario: ${session.scenarioName}`,
      repo: { mode: 'git', url: 'https://github.com/scenario/repo.git', branch: 'main' },
      agent: { type: 'mock', version: 'latest' },
      runtime: { type: 'mock' },
      metadata: { scenario: session.scenarioName },
      links: {
        self: `/api/v1/sessions/${session.id}`,
        events: `/api/v1/sessions/${session.id}/events`,
        logs: `/api/v1/sessions/${session.id}/logs`,
      },
    }));

    // Add completed sessions if they should be merged
    const completedSessions = scenarioRunner
      .getCompletedSessions()
      .filter(session => session.status === 'completed' && session.merged)
      .map(session => ({
        id: session.id,
        tenantId: 'scenario',
        projectId: 'scenario-project',
        status: 'completed',
        createdAt: session.startTime.toISOString(),
        prompt: `Scenario: ${session.scenarioName}`,
        repo: { mode: 'git', url: 'https://github.com/scenario/repo.git', branch: 'main' },
        agent: { type: 'mock', version: 'latest' },
        runtime: { type: 'mock' },
        metadata: { scenario: session.scenarioName, merged: true },
        links: {
          self: `/api/v1/sessions/${session.id}`,
          events: `/api/v1/sessions/${session.id}/events`,
          logs: `/api/v1/sessions/${session.id}/logs`,
        },
      }));

    filteredSessions = [...filteredSessions, ...scenarioSessions, ...completedSessions];
  }

  if (status) {
    filteredSessions = filteredSessions.filter(s => s.status === status);
  }

  if (projectId) {
    filteredSessions = filteredSessions.filter(s => s.projectId === projectId);
  }

  const start = (Number(page) - 1) * Number(perPage);
  const end = start + Number(perPage);
  const paginatedSessions = filteredSessions.slice(start, end);

  res.json({
    items: paginatedSessions,
    pagination: {
      page: Number(page),
      perPage: Number(perPage),
      total: filteredSessions.length,
      totalPages: Math.ceil(filteredSessions.length / Number(perPage)),
    },
  });
});

// GET /api/v1/sessions/:id - Get Session
router.get('/:id', (req, res) => {
  // First check regular mock sessions
  let session = mockSessions.find(s => s.id === req.params.id);

  // If not found and scenario runner is active, check scenario sessions
  if (!session && scenarioRunner) {
    const scenarioSession = scenarioRunner.getSessionById(req.params.id);
    if (scenarioSession) {
      session = {
        id: scenarioSession.id,
        tenantId: 'scenario',
        projectId: 'scenario-project',
        status: scenarioSession.status,
        createdAt: scenarioSession.startTime.toISOString(),
        prompt: `Scenario: ${scenarioSession.scenarioName}`,
        repo: { mode: 'git', url: 'https://github.com/scenario/repo.git', branch: 'main' },
        agent: { type: 'mock', version: 'latest' },
        runtime: { type: 'mock' },
        metadata: {
          scenario: scenarioSession.scenarioName,
          merged: scenarioSession.merged,
          completed: scenarioSession.completed,
          currentEventIndex: scenarioSession.currentEventIndex,
          totalEvents: scenarioSession.scenario.timeline.length,
        },
        links: {
          self: `/api/v1/sessions/${scenarioSession.id}`,
          events: `/api/v1/sessions/${scenarioSession.id}/events`,
          logs: `/api/v1/sessions/${scenarioSession.id}/logs`,
        },
      };
    }
  }

  if (!session) {
    return res.status(404).json({
      type: 'https://docs.example.com/errors/not-found',
      title: 'Session Not Found',
      status: 404,
      detail: `Session ${req.params.id} not found`,
    });
  }

  res.json(session);
});

// POST /api/v1/sessions/:id/stop - Stop Session
router.post('/:id/stop', (req, res) => {
  const session = mockSessions.find(s => s.id === req.params.id);

  if (!session) {
    return res.status(404).json({
      type: 'https://docs.example.com/errors/not-found',
      title: 'Session Not Found',
      status: 404,
      detail: `Session ${req.params.id} not found`,
    });
  }

  session.status = 'stopping';
  res.json({ status: 'accepted' });
});

// DELETE /api/v1/sessions/:id - Cancel Session
router.delete('/:id', (req, res) => {
  const index = mockSessions.findIndex(s => s.id === req.params.id);

  if (index === -1) {
    return res.status(404).json({
      type: 'https://docs.example.com/errors/not-found',
      title: 'Session Not Found',
      status: 404,
      detail: `Session ${req.params.id} not found`,
    });
  }

  mockSessions.splice(index, 1);
  res.status(204).send();
});

// GET /api/v1/sessions/:id/events - SSE Events
router.get('/:id/events', (req, res) => {
  const sessionId = req.params.id;

  // First check regular mock sessions
  let session = mockSessions.find(s => s.id === sessionId);

  // If not found and scenario runner is active, check scenario sessions
  let isScenarioSession = false;
  if (!session && scenarioRunner) {
    const scenarioSession = scenarioRunner.getSessionById(sessionId);
    if (scenarioSession) {
      isScenarioSession = true;
      session = {
        id: scenarioSession.id,
        status: scenarioSession.status,
      } as any;
    }
  }

  if (!session) {
    return res.status(404).json({
      type: 'https://docs.example.com/errors/not-found',
      title: 'Session Not Found',
      status: 404,
      detail: `Session ${sessionId} not found`,
    });
  }

  // Check if this is a test request (no Accept header or specific test header)
  const isTestRequest =
    !req.headers.accept?.includes('text/event-stream') || req.headers['x-test-request'] === 'true';

  // Set SSE headers
  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache');
  res.setHeader('Connection', 'keep-alive');
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Headers', 'Cache-Control');

  // Send initial heartbeat
  const sendEvent = (eventType: string, data: any) => {
    res.write(`event: ${eventType}\n`);
    res.write(`data: ${JSON.stringify(data)}\n\n`);
  };

  // Send initial status
  sendEvent('status', {
    sessionId: session.id,
    status: session.status,
    ts: new Date().toISOString(),
  });

  if (isTestRequest) {
    // For test requests, send one event and close immediately
    res.end();
    return;
  }

  // For scenario sessions, handle differently
  if (isScenarioSession && scenarioRunner) {
    const scenarioSession = scenarioRunner.getSessionById(sessionId);
    if (!scenarioSession) {
      res.end();
      return;
    }

    // Send all accumulated events for scenario sessions
    const events = scenarioRunner.getSessionEvents(sessionId);
    for (const event of events) {
      sendEvent(event.type, event);
    }

    // If session is completed, send final status and close
    if (scenarioSession.status === 'completed' || scenarioSession.status === 'failed') {
      sendEvent('status', {
        sessionId: scenarioSession.id,
        status: scenarioSession.status,
        ts: new Date().toISOString(),
      });
      res.end();
      return;
    }

    // For active scenario sessions, poll for new events
    let lastEventIndex = events.length;
    const pollInterval = setInterval(() => {
      if (!scenarioRunner) {
        clearInterval(pollInterval);
        res.end();
        return;
      }

      const newEvents = scenarioRunner.getSessionEvents(sessionId, lastEventIndex);
      for (const event of newEvents) {
        sendEvent(event.type, event);
      }
      lastEventIndex += newEvents.length;

      // Check if session completed
      const currentSession = scenarioRunner.getSessionById(sessionId);
      if (
        currentSession &&
        (currentSession.status === 'completed' || currentSession.status === 'failed')
      ) {
        sendEvent('status', {
          sessionId: currentSession.id,
          status: currentSession.status,
          ts: new Date().toISOString(),
        });
        clearInterval(pollInterval);
        res.end();
      }
    }, 1000); // Poll every second

    req.on('close', () => {
      clearInterval(pollInterval);
      res.end();
    });

    return;
  }

  // For regular sessions, continue with existing logic
  // For completed sessions, just send initial status and close
  if (
    session.status === 'completed' ||
    session.status === 'failed' ||
    session.status === 'cancelled'
  ) {
    sendEvent('status', {
      sessionId: session.id,
      status: session.status,
      ts: new Date().toISOString(),
    });
    res.end();
    return;
  }

  // For active sessions, generate continuous event streams
  let eventCount = 0;
  const maxEvents = 100; // Extended for longer-running sessions

  // Use faster interval for test scenarios and for last_line events
  const scenario = scenarioReplayer.getScenarioForSession(session.id);
  const isTestScenario = scenario && scenario.description?.includes('test');
  // Base interval: 2000ms for normal events, but will use 400ms for last_line
  const baseIntervalMs = isTestScenario ? 1000 : 2000;

  // Generate realistic event types for active sessions
  const eventTypes = ['thinking', 'tool_execution', 'file_edit', 'status'];
  const thinkingMessages = [
    'Analyzing the codebase structure',
    'Identifying the best approach for this task',
    'Considering edge cases and error handling',
    'Planning the implementation strategy',
    'Reviewing related code sections',
  ];
  const toolExecutions = [
    {
      name: 'read_file',
      args: { path: 'src/auth.ts' },
      lastLines: [
        'Opening file...',
        'Reading contents...',
        'Parsing TypeScript...',
        'Analyzing imports...',
        'Done',
      ],
      output: 'File read successfully (142 lines)',
    },
    {
      name: 'search_codebase',
      args: { query: 'password validation' },
      lastLines: [
        'Searching files...',
        'Scanning src/...',
        'Scanning tests/...',
        'Found 3 matches in 2 files',
        'Analyzing results...',
      ],
      output: 'Found 3 matches',
    },
    {
      name: 'grep',
      args: { pattern: 'export.*function' },
      lastLines: [
        'Scanning repository...',
        'Processing src/...',
        'Processing lib/...',
        'Found 24 exported functions',
      ],
      output: 'Search complete',
    },
    {
      name: 'run_terminal_cmd',
      args: { command: 'npm test' },
      lastLines: [
        'Starting test suite...',
        '> Running tests with Jest',
        'PASS  src/auth.test.ts',
        'PASS  src/utils.test.ts',
        'PASS  src/db.test.ts',
        'Running 15 tests...',
        'Test Suites: 3 passed, 3 total',
        'Tests:       15 passed, 15 total',
        'All tests passed',
      ],
      output: '15 tests passed',
    },
  ];
  const fileEdits = [
    {
      path: 'src/auth.ts',
      linesAdded: 5,
      linesRemoved: 3,
      preview: '+  validatePassword(password);',
    },
    {
      path: 'src/utils/validation.ts',
      linesAdded: 12,
      linesRemoved: 0,
      preview: '+ export function validatePassword(pwd: string) {',
    },
    {
      path: 'tests/auth.test.ts',
      linesAdded: 8,
      linesRemoved: 2,
      preview: '+  it("validates password strength", () => {',
    },
  ];

  // Track tool execution state for multi-event sequences
  let currentToolExecution: { tool: any; lastLineIndex: number } | null = null;
  let timeoutId: ReturnType<typeof setTimeout>;

  const sendNextEvent = () => {
    if (eventCount >= maxEvents) {
      res.end();
      return;
    }

    eventCount++;

    // Track if we sent a last_line event (for rapid-fire interval)
    let sentLastLine = false;

    // Try to replay scenario event first
    const scenarioEvent = scenarioReplayer.replayScenario(session.id, eventCount - 1);

    if (scenarioEvent) {
      // Send the scenario event
      sendEvent(scenarioEvent.type, scenarioEvent);

      // Update session status if it's a status event
      if (scenarioEvent.type === 'status' && scenarioEvent.status) {
        session.status = scenarioEvent.status;
      }
    } else {
      // Generate realistic events for continuous streams

      // If we have a tool execution in progress, continue it
      if (currentToolExecution) {
        const { tool, lastLineIndex } = currentToolExecution;

        if (lastLineIndex < tool.lastLines.length) {
          // Send next last_line update (IN PLACE update) - RAPID FIRE
          sendEvent('tool_execution', {
            sessionId: session.id,
            tool_name: tool.name,
            last_line: tool.lastLines[lastLineIndex],
            ts: new Date().toISOString(),
          });
          currentToolExecution.lastLineIndex++;
          sentLastLine = true;
        } else {
          // Tool execution complete - send completion event
          sendEvent('tool_execution', {
            sessionId: session.id,
            tool_name: tool.name,
            tool_output: tool.output,
            tool_status: 'success',
            ts: new Date().toISOString(),
          });
          currentToolExecution = null;
        }
      } else {
        // No tool in progress - start a new event
        const eventType = eventTypes[Math.floor(Math.random() * eventTypes.length)];

        switch (eventType) {
          case 'thinking':
            sendEvent('thinking', {
              sessionId: session.id,
              thought: thinkingMessages[Math.floor(Math.random() * thinkingMessages.length)],
              ts: new Date().toISOString(),
            });
            break;

          case 'tool_execution': {
            // Start a new tool execution sequence
            const tool = toolExecutions[Math.floor(Math.random() * toolExecutions.length)];

            // Send tool start event (tool_name only, no last_line or output)
            sendEvent('tool_execution', {
              sessionId: session.id,
              tool_name: tool.name,
              tool_args: tool.args,
              ts: new Date().toISOString(),
            });

            // Start tracking this tool execution
            currentToolExecution = { tool, lastLineIndex: 0 };
            break;
          }

          case 'file_edit': {
            const edit = fileEdits[Math.floor(Math.random() * fileEdits.length)];
            sendEvent('file_edit', {
              sessionId: session.id,
              file_path: edit.path,
              lines_added: edit.linesAdded,
              lines_removed: edit.linesRemoved,
              diff_preview: edit.preview,
              ts: new Date().toISOString(),
            });
            break;
          }

          case 'status':
            sendEvent('progress', {
              sessionId: session.id,
              progress: Math.min(100, eventCount * 2),
              stage:
                eventCount < 25
                  ? 'analyzing'
                  : eventCount < 50
                    ? 'implementing'
                    : eventCount < 75
                      ? 'testing'
                      : 'finalizing',
              ts: new Date().toISOString(),
            });
            break;
        }
      }

      // Schedule next event - use rapid-fire interval for last_line events
      const nextInterval = sentLastLine ? 400 : baseIntervalMs;
      timeoutId = setTimeout(sendNextEvent, nextInterval);
    }
  };

  // Start the event stream
  timeoutId = setTimeout(sendNextEvent, baseIntervalMs);

  // Handle client disconnect
  req.on('close', () => {
    clearTimeout(timeoutId);
    res.end();
  });
});

// GET /api/v1/sessions/:id/logs - Get Logs
router.get('/:id/logs', (req, res) => {
  const sessionId = req.params.id;

  // First check regular mock sessions
  let session = mockSessions.find(s => s.id === sessionId);

  // If not found and scenario runner is active, check scenario sessions
  let isScenarioSession = false;
  if (!session && scenarioRunner) {
    const scenarioSession = scenarioRunner.getSessionById(sessionId);
    if (scenarioSession) {
      isScenarioSession = true;
      session = {
        id: scenarioSession.id,
        status: scenarioSession.status,
      } as any;
    }
  }

  if (!session) {
    return res.status(404).json({
      type: 'https://docs.example.com/errors/not-found',
      title: 'Session Not Found',
      status: 404,
      detail: `Session ${req.params.id} not found`,
    });
  }

  let logs: Array<{ level: string; message: string; ts: string }> = [];

  if (isScenarioSession && scenarioRunner) {
    // Generate logs from scenario events
    const scenarioSession = scenarioRunner.getSessionById(sessionId);
    if (scenarioSession) {
      const events = scenarioRunner.getSessionEvents(sessionId);
      let timestamp = scenarioSession.startTime.getTime();

      logs.push({
        level: 'info',
        message: `Scenario session started: ${scenarioSession.scenarioName}`,
        ts: scenarioSession.startTime.toISOString(),
      });

      for (const event of events) {
        timestamp += 1000; // Add 1 second per event for simplicity
        let message = '';

        switch (event.type) {
          case 'thinking':
            message = `Thinking: ${event.thought}`;
            break;
          case 'tool_execution':
            if (event.tool_name && event.tool_args) {
              message = `Tool: ${event.tool_name}(${JSON.stringify(event.tool_args)})`;
            } else if (event.last_line) {
              message = `Tool progress: ${event.last_line}`;
            } else if (event.tool_output) {
              message = `Tool result: ${event.tool_output}`;
            }
            break;
          case 'file_edit':
            message = `File edit: ${event.file_path} (+${event.lines_added} -${event.lines_removed})`;
            break;
          case 'log':
            message = event.message || 'Log message';
            break;
          case 'status':
            message = `Status: ${event.status}`;
            break;
          default:
            message = `${event.type}: ${JSON.stringify(event)}`;
        }

        if (message) {
          logs.push({
            level: 'info',
            message,
            ts: new Date(timestamp).toISOString(),
          });
        }
      }

      // Add completion message if session is done
      if (scenarioSession.status === 'completed') {
        logs.push({
          level: 'info',
          message: `Scenario completed: ${scenarioSession.scenarioName}`,
          ts: new Date().toISOString(),
        });
      } else if (scenarioSession.status === 'failed') {
        logs.push({
          level: 'error',
          message: `Scenario failed: ${scenarioSession.scenarioName}`,
          ts: new Date().toISOString(),
        });
      }
    }
  } else {
    // Generate logs based on the session's assigned scenario (existing logic)
    const scenario = scenarioReplayer.getScenarioForSession(session.id);

    if (scenario && scenario.turns) {
      // Generate logs from scenario turns
      let timestamp = Date.now() - 300000; // Start 5 minutes ago
      logs.push({
        level: 'info',
        message: 'Session started',
        ts: new Date(timestamp).toISOString(),
      });

      timestamp += 50000; // 50 seconds later
      logs.push({
        level: 'info',
        message: 'Agent initialized',
        ts: new Date(timestamp).toISOString(),
      });

      // Add logs for each scenario turn
      for (let i = 0; i < scenario.turns.length; i++) {
        const turn = scenario.turns[i];
        timestamp += 30000; // 30 seconds between turns

        if (turn.user) {
          logs.push({
            level: 'info',
            message: `User: ${turn.user}`,
            ts: new Date(timestamp).toISOString(),
          });
        }

        if (turn.think) {
          timestamp += 10000; // 10 seconds for thinking
          logs.push({
            level: 'info',
            message: `Thinking: ${turn.think}`,
            ts: new Date(timestamp).toISOString(),
          });
        }

        if (turn.tool) {
          timestamp += 5000; // 5 seconds for tool execution
          logs.push({
            level: 'info',
            message: `Tool: ${turn.tool.name}(${JSON.stringify(turn.tool.args)})`,
            ts: new Date(timestamp).toISOString(),
          });
        }

        if (turn.assistant) {
          timestamp += 15000; // 15 seconds for response
          logs.push({
            level: 'info',
            message: `Assistant: ${turn.assistant}`,
            ts: new Date(timestamp).toISOString(),
          });
        }
      }
    } else {
      // Fallback logs if no scenario available
      logs.push(
        {
          level: 'info',
          message: 'Session started',
          ts: new Date(Date.now() - 300000).toISOString(),
        },
        {
          level: 'info',
          message: 'Agent initialized',
          ts: new Date(Date.now() - 250000).toISOString(),
        },
        {
          level: 'info',
          message: 'Task execution in progress',
          ts: new Date(Date.now() - 200000).toISOString(),
        },
      );
    }
  }

  // Apply tail parameter if provided
  const tail = req.query.tail ? parseInt(req.query.tail as string) : null;
  const responseLogs = tail ? logs.slice(-tail) : logs;

  res.json({ logs: responseLogs });
});

export { router as sessionsRouter, scenarioReplayer };
