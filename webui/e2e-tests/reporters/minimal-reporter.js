/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

const fs = require('fs');
const path = require('path');

class MinimalReporter {
  constructor() {
    this.logsDir = path.join(process.cwd(), 'test-results/logs');
    this.testCounter = 0;
    this.failedTests = [];
    this.passedTests = [];
    // Use shared timestamp from environment, or generate one
    this.runTimestamp =
      process.env.TEST_RUN_TIMESTAMP || new Date().toISOString().replace(/[:.]/g, '-');
    this.runDir = path.join(this.logsDir, `test-run-${this.runTimestamp}`);

    console.log('MinimalReporter constructor - CWD:', process.cwd());
    console.log('MinimalReporter constructor - logsDir:', this.logsDir);
    console.log('MinimalReporter constructor - runDir:', this.runDir);
    console.log('MinimalReporter constructor - using timestamp:', this.runTimestamp);

    // Ensure logs directory and run-specific directory exist
    try {
      if (!fs.existsSync(this.logsDir)) {
        fs.mkdirSync(this.logsDir, { recursive: true });
        console.log('Created logs directory:', this.logsDir);
      }
      if (!fs.existsSync(this.runDir)) {
        fs.mkdirSync(this.runDir, { recursive: true });
        console.log('Created run directory:', this.runDir);
      }
    } catch (error) {
      console.error('Failed to create directories:', error);
    }
  }

  onBegin() {
    console.log('🧪 Running tests...');
  }

  onTestBegin(test) {
    // Create unique log file for this test in the run-specific directory
    const testId = `${++this.testCounter}`;
    const sanitizedTitle = test.title
      .replace(/[^a-zA-Z0-9_-]/g, '_') // Replace special chars with underscores
      .replace(/_+/g, '_') // Replace multiple underscores with single
      .replace(/^_+|_+$/g, '') // Remove leading/trailing underscores
      .substring(0, 50); // Limit length

    // Ensure run directory exists before creating log file
    try {
      if (!fs.existsSync(this.runDir)) {
        fs.mkdirSync(this.runDir, { recursive: true });
      }
    } catch (error) {
      console.error('Failed to create run directory in onTestBegin:', error);
      return;
    }

    const logFile = path.join(this.runDir, `${testId}-${sanitizedTitle}.log`);

    // Store log file path in test object for later use
    test._logFile = logFile;
    test._testId = testId;

    // Start capturing output to file
    const logStream = fs.createWriteStream(logFile, { flags: 'a' });

    // Write test metadata to log file
    logStream.write(`[${new Date().toISOString()}] TEST_START: ${test.title}\n`);
    logStream.write(`[${new Date().toISOString()}] TEST_ID: ${testId}\n`);
    logStream.write(`[${new Date().toISOString()}] TEST_FILE: ${test.location.file}\n`);
    logStream.write(`[${new Date().toISOString()}] TEST_LINE: ${test.location.line}\n`);
    logStream.write(`[${new Date().toISOString()}] TEST_COLUMN: ${test.location.column}\n`);
    logStream.write(`[${new Date().toISOString()}] LOG_FILE: ${logFile}\n\n`);

    // Store log stream for later use
    test._logStream = logStream;

    // Store cleanup function
    test._cleanupConsole = () => {
      logStream.write(`\n[${new Date().toISOString()}] TEST_END\n`);
      logStream.end();
    };
  }

  onStdOut(chunk, test) {
    if (test && test._logStream) {
      test._logStream.write(`[${new Date().toISOString()}] STDOUT: ${chunk.toString()}\n`);
    }
  }

  onStdErr(chunk, test) {
    if (test && test._logStream) {
      test._logStream.write(`[${new Date().toISOString()}] STDERR: ${chunk.toString()}\n`);
    }
  }

  onTestEnd(test, result) {
    const logFile = test._logFile;
    const logStream = test._logStream;
    const cleanupConsole = test._cleanupConsole;

    // Log test result to file
    if (logStream) {
      logStream.write(`[${new Date().toISOString()}] RESULT: Test ${result.status}\n`);
      if (result.duration) {
        logStream.write(`[${new Date().toISOString()}] DURATION: ${result.duration}ms\n`);
      }
      if (result.error) {
        logStream.write(`[${new Date().toISOString()}] ERROR: ${result.error.message}\n`);
        logStream.write(
          `[${new Date().toISOString()}] ERROR_STACK: ${result.error.stack || 'No stack trace'}\n`,
        );
      }
    }

    // Restore console
    if (cleanupConsole) {
      cleanupConsole();
    }

    // Track test results
    const testInfo = {
      id: test._testId,
      title: test.title,
      file: test.location.file,
      line: test.location.line,
      status: result.status,
      duration: result.duration || 0,
      logFile: logFile,
      error: result.error ? result.error.message : null,
    };

    if (result.status === 'passed') {
      this.passedTests.push(testInfo);
      // On success: minimal output (just a dot)
      process.stdout.write('.');
    } else if (result.status === 'failed') {
      this.failedTests.push(testInfo);
      // On failure: concise output (just F)
      process.stdout.write('F');
    } else if (result.status === 'skipped') {
      process.stdout.write('S');
    }
  }

  onEnd() {
    // Create summary files
    this.createSummaryFiles();

    console.log('\n\n📊 Test Summary:');
    console.log(`   📁 Test logs saved to: ${this.runDir}`);

    if (this.failedTests.length > 0) {
      console.log(`   ❌ ${this.failedTests.length} failed tests:`);
      this.failedTests.forEach(test => {
        const relativePath = path.relative(process.cwd(), test.logFile);
        console.log(`      • ${test.title}`);
        console.log(`        📄 ${relativePath}`);
      });
      console.log(
        `\n   🔍 View detailed results: cat ${path.join(this.runDir, 'failed-tests.json')}`,
      );
      console.log(`   📊 View all results: cat ${path.join(this.runDir, 'test-summary.json')}`);
    } else {
      console.log(`   ✅ All ${this.passedTests.length} tests passed!`);
    }

    console.log('   📋 Following AGENTS.md guidelines for minimal console output');
    console.log('   🔇 Server logs suppressed during testing (QUIET_MODE=true)');

    // Show commands for accessing logs
    console.log('\n💡 Commands to view logs:');
    console.log(`   • View failed tests: cat ${path.join(this.runDir, 'failed-tests.txt')}`);
    console.log(`   • View test summary: cat ${path.join(this.runDir, 'test-summary.json')}`);
    console.log(`   • List all log files: ls -la ${this.runDir}/*.log`);
    console.log(`   • Open Playwright report: just webui-test-report`);
  }

  createSummaryFiles() {
    // Ensure run directory still exists before writing summary artifacts
    if (!fs.existsSync(this.runDir)) {
      fs.mkdirSync(this.runDir, { recursive: true });
    }

    // Create failed tests summary (human readable)
    const failedSummaryPath = path.join(this.runDir, 'failed-tests.txt');
    const failedContent = this.failedTests
      .map(test => {
        const relativePath = path.relative(process.cwd(), test.logFile);
        return `FAILED: ${test.title}
   File: ${test.file}:${test.line}
   Log: ${relativePath}
   Error: ${test.error || 'No error message'}
   ---
`;
      })
      .join('\n');

    fs.writeFileSync(
      failedSummaryPath,
      `Test Run: ${this.runTimestamp}\nFailed Tests: ${this.failedTests.length}\n\n${failedContent}`,
    );

    // Create comprehensive test summary (JSON)
    const summaryPath = path.join(this.runDir, 'test-summary.json');
    const summary = {
      runTimestamp: this.runTimestamp,
      totalTests: this.passedTests.length + this.failedTests.length,
      passed: this.passedTests.length,
      failed: this.failedTests.length,
      passedTests: this.passedTests,
      failedTests: this.failedTests,
      runDirectory: this.runDir,
    };

    fs.writeFileSync(summaryPath, JSON.stringify(summary, null, 2));

    // Create a quick failed tests list for easy access
    const failedListPath = path.join(this.runDir, 'failed-tests.json');
    fs.writeFileSync(failedListPath, JSON.stringify(this.failedTests, null, 2));
  }
}

module.exports = MinimalReporter;
