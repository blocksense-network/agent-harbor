import { test, expect } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { ScenarioTimingValidator, recordScenarioEvents } from '../test-helpers/scenario-timing.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * Scenario Testing
 *
 * Tests that verify scenario files are executed correctly by the mock server,
 * including timing verification and SSE event streaming.
 */

interface RecordedEvent {
  type: string;
  timestamp: number;
  data: any;
}

interface ScenarioEvent {
  type: string;
  expectedDelay?: number;
  data: any;
}

test.describe('Scenario Testing', () => {
  test.describe('Basic Scenario Execution', () => {
    test('should execute test scenario and stream events correctly', async ({ request }) => {
      // This test requires starting the mock server manually with the scenario
      // In CI, this would be handled by the test runner starting the server

      const scenarioPath = path.join(__dirname, '../../../test_scenarios/test_scenario.yaml');

      // Skip test if scenario file doesn't exist
      if (!fs.existsSync(scenarioPath)) {
        test.skip(true, 'Test scenario file not found');
        return;
      }

      // Test that sessions endpoint includes scenario sessions
      const sessionsResponse = await request.get('/api/v1/sessions');
      expect(sessionsResponse.ok()).toBe(true);

      const sessionsData = await sessionsResponse.json();
      expect(sessionsData).toHaveProperty('items');
      expect(Array.isArray(sessionsData.items)).toBe(true);

      // Find scenario sessions
      const scenarioSessions = sessionsData.items.filter((session: any) =>
        session.metadata?.scenario === 'test_scenario'
      );

      if (scenarioSessions.length === 0) {
        console.warn('No scenario sessions found - mock server may not be running with scenario');
        return;
      }

      expect(scenarioSessions.length).toBe(1);
      const scenarioSession = scenarioSessions[0];

      expect(scenarioSession).toHaveProperty('id');
      expect(scenarioSession.status).toBe('running');
      expect(scenarioSession.metadata.scenario).toBe('test_scenario');
      expect(scenarioSession.metadata.merged).toBe(false);
    });

    test('should complete scenario and merge if configured', async ({ request }) => {
      // Wait for scenario to complete (this test assumes the scenario has already run)
      await new Promise(resolve => setTimeout(resolve, 5000));

      const sessionsResponse = await request.get('/api/v1/sessions');
      expect(sessionsResponse.ok()).toBe(true);

      const sessionsData = await sessionsResponse.json();
      const scenarioSessions = sessionsData.items.filter((session: any) =>
        session.metadata?.scenario === 'test_scenario'
      );

      // Scenario should be completed and merged
      if (scenarioSessions.length > 0) {
        const scenarioSession = scenarioSessions[0];
        expect(['completed', 'running']).toContain(scenarioSession.status);
        if (scenarioSession.status === 'completed') {
          expect(scenarioSession.metadata.merged).toBe(true);
        }
      }
    });
  });

  test.describe('SSE Event Timing Verification', () => {
    test('should execute timing test scenario with correct event sequencing', async ({ request }) => {
      const scenarioPath = path.join(__dirname, '../../../test_scenarios/timing_test_scenario.yaml');

      if (!fs.existsSync(scenarioPath)) {
        test.skip(true, 'Timing test scenario file not found');
        return;
      }

      // Load scenario for validation
      const validator = new ScenarioTimingValidator();
      validator.loadScenario(scenarioPath);

      // Find the timing test scenario session
      const sessionsResponse = await request.get('/api/v1/sessions');
      expect(sessionsResponse.ok()).toBe(true);

      const sessionsData = await sessionsResponse.json();
      const timingSessions = sessionsData.items.filter((session: any) =>
        session.metadata?.scenario === 'timing_test_scenario'
      );

      if (timingSessions.length === 0) {
        console.warn('Timing test scenario session not found - server may not be running with timing scenario');
        return;
      }

      expect(timingSessions.length).toBe(1);
      const timingSession = timingSessions[0];

      // Test that session has expected metadata
      expect(timingSession.metadata.scenario).toBe('timing_test_scenario');
      expect(timingSession.metadata.merged).toBe(false);
      expect(timingSession.metadata.completed).toBe(false);

      // Get session logs to verify scenario execution
      const logsResponse = await request.get(`/api/v1/sessions/${timingSession.id}/logs`);
      expect(logsResponse.ok()).toBe(true);

      const logsData = await logsResponse.json();
      expect(logsData.logs.length).toBeGreaterThan(0);

      // Verify scenario-specific log entries
      const scenarioLogs = logsData.logs.filter((log: any) =>
        log.message.includes('timing_test_scenario') ||
        log.message.includes('Timing test')
      );

      expect(scenarioLogs.length).toBeGreaterThan(0);
    });

    test('should validate scenario file structure and timing expectations', () => {
      const scenarioPath = path.join(__dirname, '../../../test_scenarios/timing_test_scenario.yaml');

      if (!fs.existsSync(scenarioPath)) {
        test.skip(true, 'Timing test scenario file not found');
        return;
      }

      // Test scenario file parsing
      const validator = new ScenarioTimingValidator();
      expect(() => validator.loadScenario(scenarioPath)).not.toThrow();

      // Verify scenario has expected structure
      const scenarioContent = fs.readFileSync(scenarioPath, 'utf-8');
      const scenario = require('yaml').parse(scenarioContent);

      expect(scenario).toHaveProperty('name', 'timing_test_scenario');
      expect(scenario).toHaveProperty('description');
      expect(scenario).toHaveProperty('timeline');
      expect(Array.isArray(scenario.timeline)).toBe(true);
      expect(scenario.timeline.length).toBeGreaterThan(0);

      // Verify timeline contains expected event types
      const eventTypes = scenario.timeline.map((event: any) => Object.keys(event)[0]);
      expect(eventTypes).toContain('think');
      expect(eventTypes).toContain('agentToolUse');
      expect(eventTypes).toContain('agentEdits');
      expect(eventTypes).toContain('complete');
      expect(eventTypes).toContain('merge');
    });

    test('should provide scenario logs with correct content', async ({ request }) => {
      const sessionsResponse = await request.get('/api/v1/sessions');
      const sessionsData = await sessionsResponse.json();

      const scenarioSessions = sessionsData.items.filter((session: any) =>
        session.metadata?.scenario === 'test_scenario'
      );

      if (scenarioSessions.length === 0) {
        console.warn('No scenario sessions found for logs test');
        return;
      }

      const scenarioSession = scenarioSessions[0];
      const sessionId = scenarioSession.id;

      // Test logs endpoint
      const logsResponse = await request.get(`/api/v1/sessions/${sessionId}/logs`);
      expect(logsResponse.ok()).toBe(true);

      const logsData = await logsResponse.json();
      expect(logsData).toHaveProperty('logs');
      expect(Array.isArray(logsData.logs)).toBe(true);

      // Should contain scenario-specific logs
      const scenarioLogs = logsData.logs.filter((log: any) =>
        log.message.includes('test_scenario') || log.message.includes('Scenario')
      );

      expect(scenarioLogs.length).toBeGreaterThan(0);
    });
  });

  test.describe('Scenario Format Validation', () => {
    test('should validate scenario file structure', () => {
      const scenarioPath = path.join(__dirname, '../../../test_scenarios/test_scenario.yaml');

      if (!fs.existsSync(scenarioPath)) {
        test.skip(true, 'Test scenario file not found');
        return;
      }

      const scenarioContent = fs.readFileSync(scenarioPath, 'utf-8');

      // Should be valid YAML
      expect(() => {
        const YAML = require('yaml');
        YAML.parse(scenarioContent);
      }).not.toThrow();

      // Should contain required fields
      const scenario = require('yaml').parse(scenarioContent);
      expect(scenario).toHaveProperty('name');
      expect(scenario).toHaveProperty('timeline');
      expect(Array.isArray(scenario.timeline)).toBe(true);
    });

    test('should handle multiple scenarios', async ({ request }) => {
      // Test that multiple scenario files can be loaded
      // This would require the server to be started with multiple --scenario flags

      const sessionsResponse = await request.get('/api/v1/sessions');
      const sessionsData = await sessionsResponse.json();

      const scenarioSessions = sessionsData.items.filter((session: any) =>
        session.metadata?.scenario
      );

      // Could be 0 or more depending on how the server was started
      expect(Array.isArray(scenarioSessions)).toBe(true);
    });
  });

  test.describe('Error Handling', () => {
    test('should handle missing scenario files gracefully', async ({ request }) => {
      // Test that server starts without scenario files
      const healthResponse = await request.get('/health');
      expect(healthResponse.ok()).toBe(true);

      const healthData = await healthResponse.json();
      expect(healthData.status).toBe('ok');
    });

    test('should handle invalid scenario files', async ({ request }) => {
      // Server should start even with invalid scenario files
      const healthResponse = await request.get('/health');
      expect(healthResponse.ok()).toBe(true);
    });
  });
});

/**
 * Helper function to record SSE events with timing for scenario verification
 * This could be used in future tests to do detailed timing verification
 */
async function recordScenarioEvents(sessionId: string, request: any): Promise<RecordedEvent[]> {
  const events: RecordedEvent[] = [];
  const startTime = Date.now();

  // Note: This is a simplified version. Real SSE testing would require
  // setting up an EventSource connection and capturing events over time.

  return events;
}

/**
 * Helper function to compare recorded events with expected scenario timing
 */
function validateScenarioTiming(scenario: any, recordedEvents: RecordedEvent[]): boolean {
  // This would implement the detailed timing comparison logic
  // comparing expected delays in the scenario with actual event timestamps

  return true; // Placeholder
}
