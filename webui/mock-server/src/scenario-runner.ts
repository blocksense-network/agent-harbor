/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import * as fs from 'fs';
import * as path from 'path';
import * as YAML from 'yaml';
import { setTimeout } from 'timers';
import { logger } from './index.js';

// Scenario format types
export interface Scenario {
  name: string;
  tags?: string[];
  terminalRef?: string;
  compat?: {
    allowInlineTerminal?: boolean;
    allowTypeSteps?: boolean;
  };
  repo?: {
    init?: boolean;
    branch?: string;
    dir?: string;
    files?: Array<{
      path: string;
      contents: string | { base64: string };
    }>;
  };
  ah?: {
    cmd: string;
    flags: string[];
    env?: Record<string, string>;
  };
  server?: {
    mode?: 'none' | 'mock' | 'real';
  };
  timeline: TimelineEvent[];
  expect?: {
    exitCode?: number;
    artifacts?: Array<{
      type: string;
      pattern: string;
    }>;
  };
}

export type TimelineEvent =
  | { think: Array<[number, string]> }
  | {
      agentToolUse: {
        toolName: string;
        args: Record<string, unknown>;
        progress?: Array<[number, string]>;
        result: string;
        status: 'ok' | 'error';
      };
    }
  | {
      agentEdits: {
        path: string;
        linesAdded: number;
        linesRemoved: number;
      };
    }
  | { assistant: Array<[number, string]> }
  | { baseTimeDelta: number }
  | { screenshot: string }
  | { assert: Assertion }
  | { userInputs: Array<[number, string]> & { target?: 'tui' | 'webui' | 'cli' } }
  | {
      userEdits: {
        patch: string;
      };
    }
  | {
      userCommand: {
        cmd: string;
        cwd?: string;
      };
    }
  | { merge: true }
  | { complete: true };

export interface Assertion {
  fs?: {
    exists?: string[];
    notExists?: string[];
  };
  text?: {
    contains?: string[];
  };
  json?: {
    file: string;
    pointer: string;
    equals: unknown;
  };
  git?: {
    commit?: {
      messageContains?: string;
    };
  };
}

export interface ScenarioSession {
  id: string;
  scenarioName: string;
  scenario: Scenario;
  status: 'running' | 'completed' | 'failed';
  startTime: Date;
  currentEventIndex: number;
  events: ScenarioEvent[];
  merged: boolean;
  completed: boolean;
}

export interface ScenarioEvent {
  type: 'status' | 'log' | 'progress' | 'thinking' | 'tool_execution' | 'file_edit';
  sessionId: string;
  status?: string;
  level?: string;
  message?: string;
  progress?: number;
  stage?: string;
  thought?: string;
  tool_name?: string;
  tool_args?: Record<string, any>;
  tool_output?: string;
  tool_status?: 'success' | 'error';
  file_path?: string;
  lines_added?: number;
  lines_removed?: number;
  diff_preview?: string;
  last_line?: string;
  ts: string;
}

export class ScenarioRunner {
  private scenarios: Map<string, Scenario> = new Map();
  private sessions: Map<string, ScenarioSession> = new Map();
  private mergeCompleted: boolean;

  constructor(scenarioFiles: string[], mergeCompleted: boolean = false) {
    this.mergeCompleted = mergeCompleted;
    this.loadScenarios(scenarioFiles);
    this.startScenarios();
  }

  private loadScenarios(scenarioFiles: string[]): void {
    for (const scenarioFile of scenarioFiles) {
      try {
        let fullPath = scenarioFile;

        // If relative path, try to resolve from common locations
        if (!path.isAbsolute(scenarioFile)) {
          const possiblePaths = [
            path.join(process.cwd(), scenarioFile),
            path.join(process.cwd(), 'specs', 'Public', scenarioFile),
            path.join(process.cwd(), 'test_scenarios', scenarioFile),
            path.join(process.cwd(), 'tests', 'tools', 'mock-agent', 'scenarios', scenarioFile),
            path.join(process.cwd(), 'tests', 'tools', 'mock-agent', 'examples', scenarioFile),
          ];

          for (const possiblePath of possiblePaths) {
            if (fs.existsSync(possiblePath)) {
              fullPath = possiblePath;
              break;
            }
          }
        }

        if (!fs.existsSync(fullPath)) {
          logger.error(`Scenario file not found: ${scenarioFile}`);
          continue;
        }

        const content = fs.readFileSync(fullPath, 'utf-8');
        const scenario: Scenario = YAML.parse(content);

        if (!scenario.name) {
          logger.error(`Scenario missing name: ${scenarioFile}`);
          continue;
        }

        this.scenarios.set(scenario.name, scenario);
        logger.log(`Loaded scenario: ${scenario.name} from ${fullPath}`);
      } catch (error) {
        logger.error(`Failed to load scenario ${scenarioFile}:`, error);
      }
    }
  }

  private startScenarios(): void {
    for (const [scenarioName, scenario] of this.scenarios) {
      const sessionId = `scenario-${scenarioName}-${Date.now()}`;
      const session: ScenarioSession = {
        id: sessionId,
        scenarioName,
        scenario,
        status: 'running',
        startTime: new Date(),
        currentEventIndex: 0,
        events: [],
        merged: false,
        completed: false,
      };

      this.sessions.set(sessionId, session);
      logger.log(`Started scenario session: ${sessionId}`);

      // Start processing the scenario asynchronously
      this.processScenario(session);
    }
  }

  private async processScenario(session: ScenarioSession): Promise<void> {
    const { scenario, id: sessionId } = session;

    try {
      for (let i = 0; i < scenario.timeline.length; i++) {
        const event = scenario.timeline[i];
        session.currentEventIndex = i;

        const scenarioEvents = this.convertTimelineEventToScenarioEvents(event, sessionId);
        session.events.push(...scenarioEvents);

        // Process each event with appropriate timing
        for (const _scenarioEvent of scenarioEvents) {
          // Add some delay between events to simulate real-time processing
          await this.delay(100);
        }

        // Check for merge event
        if ('merge' in event && event.merge) {
          session.merged = true;
          logger.log(`Scenario ${session.scenarioName} marked for merge`);
        }

        // Check for complete event
        if ('complete' in event && event.complete) {
          session.completed = true;
          session.status = 'completed';
          logger.log(`Scenario ${session.scenarioName} marked as completed`);
        }
      }

      // If scenario wasn't explicitly completed, mark it as completed now
      if (!session.completed) {
        session.status = 'completed';
        logger.log(`Scenario ${session.scenarioName} completed (implicit)`);
      }
    } catch (error) {
      session.status = 'failed';
      logger.error(`Scenario ${session.scenarioName} failed:`, error);
    }
  }

  private convertTimelineEventToScenarioEvents(
    event: TimelineEvent,
    sessionId: string,
  ): ScenarioEvent[] {
    const events: ScenarioEvent[] = [];
    const ts = new Date().toISOString();

    if ('think' in event) {
      for (const [_delay, thought] of event.think) {
        events.push({
          type: 'thinking',
          sessionId,
          thought,
          ts,
        });
      }
    } else if ('agentToolUse' in event) {
      const { toolName, args, progress, result, status } = event.agentToolUse;

      // Tool start event
      events.push({
        type: 'tool_execution',
        sessionId,
        tool_name: toolName,
        tool_args: args,
        ts,
      });

      // Progress events
      if (progress) {
        for (const [_delay, message] of progress) {
          events.push({
            type: 'tool_execution',
            sessionId,
            tool_name: toolName,
            last_line: message,
            ts,
          });
        }
      }

      // Tool completion event
      events.push({
        type: 'tool_execution',
        sessionId,
        tool_name: toolName,
        tool_output: result,
        tool_status: status === 'ok' ? 'success' : 'error',
        ts,
      });
    } else if ('agentEdits' in event) {
      const { path: filePath, linesAdded, linesRemoved } = event.agentEdits;
      events.push({
        type: 'file_edit',
        sessionId,
        file_path: filePath,
        lines_added: linesAdded,
        lines_removed: linesRemoved,
        ts,
      });
    } else if ('assistant' in event) {
      for (const [_delay, message] of event.assistant) {
        events.push({
          type: 'log',
          sessionId,
          level: 'info',
          message: `Assistant: ${message}`,
          ts,
        });
      }
    } else if ('userInputs' in event) {
      // User inputs are handled separately, not converted to scenario events
    } else if ('baseTimeDelta' in event) {
      // Time advancement is handled in the timeline processing
    } else if ('screenshot' in event) {
      // Screenshots are handled separately
    } else if ('assert' in event) {
      // Assertions are handled separately
    } else if ('userEdits' in event) {
      // User edits are handled separately
    } else if ('userCommand' in event) {
      // User commands are handled separately
    } else if ('merge' in event) {
      // Merge events are handled in the main processing loop
    } else if ('complete' in event) {
      // Complete events are handled in the main processing loop
    }

    return events;
  }

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  // Public API methods
  getActiveSessions(): ScenarioSession[] {
    return Array.from(this.sessions.values()).filter(session => session.status === 'running');
  }

  getCompletedSessions(): ScenarioSession[] {
    return Array.from(this.sessions.values()).filter(
      session => session.status === 'completed' || session.status === 'failed',
    );
  }

  getSessionById(sessionId: string): ScenarioSession | null {
    return this.sessions.get(sessionId) || null;
  }

  getSessionEvents(sessionId: string, afterIndex?: number): ScenarioEvent[] {
    const session = this.sessions.get(sessionId);
    if (!session) return [];

    const events = session.events;
    if (afterIndex !== undefined) {
      return events.slice(afterIndex);
    }
    return events;
  }

  getAvailableScenarios(): string[] {
    return Array.from(this.scenarios.keys());
  }
}
