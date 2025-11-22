/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import * as YAML from 'yaml';
import * as fs from 'fs';

export interface RecordedEvent {
  type: string;
  timestamp: number;
  sessionId: string;
  data: any;
}

export interface ScenarioTimingResult {
  scenarioName: string;
  totalDuration: number;
  events: RecordedEvent[];
  timingErrors: TimingError[];
  isValid: boolean;
}

export interface TimingError {
  eventIndex: number;
  expectedDelay: number;
  actualDelay: number;
  eventType: string;
  description: string;
}

/**
 * Records SSE events from a scenario execution and validates timing
 */
export class ScenarioTimingValidator {
  private events: RecordedEvent[] = [];
  private startTime: number = 0;
  private scenario: any = null;

  constructor(private baseUrl: string = 'http://localhost:3001') {}

  /**
   * Load and parse a scenario file
   */
  loadScenario(scenarioPath: string): void {
    if (!fs.existsSync(scenarioPath)) {
      throw new Error(`Scenario file not found: ${scenarioPath}`);
    }

    const content = fs.readFileSync(scenarioPath, 'utf-8');
    this.scenario = YAML.parse(content);

    if (!this.scenario.name || !this.scenario.timeline) {
      throw new Error('Invalid scenario file: missing name or timeline');
    }
  }

  /**
   * Record an event with timestamp
   */
  recordEvent(type: string, sessionId: string, data: any): void {
    if (this.startTime === 0) {
      this.startTime = Date.now();
    }

    this.events.push({
      type,
      timestamp: Date.now(),
      sessionId,
      data,
    });
  }

  /**
   * Validate timing against the loaded scenario
   */
  validateTiming(): ScenarioTimingResult {
    if (!this.scenario) {
      throw new Error('No scenario loaded');
    }

    const timingErrors: TimingError[] = [];
    const expectedEvents = this.extractExpectedEvents();

    // Calculate expected cumulative delays
    let expectedCumulativeDelay = 0;
    for (let i = 0; i < expectedEvents.length; i++) {
      const expectedEvent = expectedEvents[i];
      expectedCumulativeDelay += expectedEvent.expectedDelay;

      // Find corresponding recorded event
      const recordedEvent = this.events.find(
        e => e.type === expectedEvent.type && this.matchesEventData(e.data, expectedEvent.data),
      );

      if (recordedEvent) {
        const actualDelay = recordedEvent.timestamp - this.startTime;

        // Allow 100ms tolerance for timing variations
        const tolerance = 100;
        if (Math.abs(actualDelay - expectedCumulativeDelay) > tolerance) {
          timingErrors.push({
            eventIndex: i,
            expectedDelay: expectedCumulativeDelay,
            actualDelay,
            eventType: expectedEvent.type,
            description: `Timing deviation: expected ${expectedCumulativeDelay}ms, got ${actualDelay}ms`,
          });
        }
      } else {
        timingErrors.push({
          eventIndex: i,
          expectedDelay: expectedCumulativeDelay,
          actualDelay: -1,
          eventType: expectedEvent.type,
          description: `Expected event not found: ${expectedEvent.type}`,
        });
      }
    }

    const totalDuration =
      this.events.length > 0 ? this.events[this.events.length - 1].timestamp - this.startTime : 0;

    return {
      scenarioName: this.scenario.name,
      totalDuration,
      events: this.events,
      timingErrors,
      isValid: timingErrors.length === 0,
    };
  }

  /**
   * Extract expected events with timing from scenario timeline
   */
  private extractExpectedEvents(): Array<{ type: string; expectedDelay: number; data: any }> {
    const events: Array<{ type: string; expectedDelay: number; data: any }> = [];
    let cumulativeDelay = 0;

    for (const timelineEvent of this.scenario.timeline) {
      if ('think' in timelineEvent) {
        for (const [delay, thought] of timelineEvent.think) {
          cumulativeDelay += delay;
          events.push({
            type: 'thinking',
            expectedDelay: cumulativeDelay,
            data: { thought },
          });
        }
      } else if ('agentToolUse' in timelineEvent) {
        const { toolName } = timelineEvent.agentToolUse;
        cumulativeDelay += 100; // Assume some base delay
        events.push({
          type: 'tool_execution',
          expectedDelay: cumulativeDelay,
          data: { tool_name: toolName },
        });
      } else if ('agentEdits' in timelineEvent) {
        const { path: filePath, linesAdded, linesRemoved } = timelineEvent.agentEdits;
        cumulativeDelay += 50; // File edit delay
        events.push({
          type: 'file_edit',
          expectedDelay: cumulativeDelay,
          data: { file_path: filePath, lines_added: linesAdded, lines_removed: linesRemoved },
        });
      } else if ('complete' in timelineEvent && timelineEvent.complete) {
        cumulativeDelay += 10; // Small delay for completion event
        events.push({
          type: 'complete',
          expectedDelay: cumulativeDelay,
          data: {},
        });
      } else if ('merge' in timelineEvent && timelineEvent.merge) {
        cumulativeDelay += 10; // Small delay for merge event
        events.push({
          type: 'merge',
          expectedDelay: cumulativeDelay,
          data: {},
        });
      }
    }

    return events;
  }

  /**
   * Check if recorded event data matches expected event data
   */
  private matchesEventData(recordedData: any, expectedData: any): boolean {
    // Simple matching logic - can be enhanced based on event types
    if (expectedData.thought && recordedData.thought) {
      return (
        recordedData.thought.includes(expectedData.thought) ||
        expectedData.thought.includes(recordedData.thought)
      );
    }

    if (expectedData.tool_name && recordedData.tool_name) {
      return recordedData.tool_name === expectedData.tool_name;
    }

    if (expectedData.file_path && recordedData.file_path) {
      return recordedData.file_path === expectedData.file_path;
    }

    return false;
  }

  /**
   * Reset the validator for a new test
   */
  reset(): void {
    this.events = [];
    this.startTime = 0;
    this.scenario = null;
  }

  /**
   * Get current events for debugging
   */
  getEvents(): RecordedEvent[] {
    return this.events;
  }
}

/**
 * Connect to SSE endpoint and record events
 * This would typically be used in a test environment
 */
export async function recordScenarioEvents(
  sessionId: string,
  _baseUrl: string = 'http://localhost:3001',
  duration: number = 10000,
): Promise<RecordedEvent[]> {
  const events: RecordedEvent[] = [];
  const startTime = Date.now();

  try {
    // In a real implementation, this would use EventSource or fetch with SSE support
    // For now, we'll simulate by polling the events endpoint
    const endTime = startTime + duration;

    while (Date.now() < endTime) {
      // Poll for new events (this is a simplified approach)
      await new Promise(resolve => setTimeout(resolve, 500));

      // In a real test, you'd connect to the SSE endpoint and collect events
      // For this implementation, we'll return an empty array as a placeholder
    }
  } catch (error) {
    console.error('Error recording scenario events:', error);
  }

  return events;
}
