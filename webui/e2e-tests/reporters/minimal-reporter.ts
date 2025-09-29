import { Reporter, TestResult, TestCase } from '@playwright/test/reporter';
import * as fs from 'fs';
import * as path from 'path';

class MinimalReporter implements Reporter {
  private logsDir = 'test-results/logs';
  private testCounter = 0;

  constructor() {
    // Ensure logs directory exists
    if (!fs.existsSync(this.logsDir)) {
      fs.mkdirSync(this.logsDir, { recursive: true });
    }
  }

  onBegin() {
    console.log('🧪 Running tests...');
  }

  onTestBegin(test: TestCase) {
    // Create unique log file for this test
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const testId = `${timestamp}-${++this.testCounter}`;
    const logFile = path.join(this.logsDir, `${testId}.log`);

    // Store log file path in test object for later use
    (test as any)._logFile = logFile;

    // Start capturing output to file
    const logStream = fs.createWriteStream(logFile, { flags: 'a' });

    // Write test metadata to log file
    logStream.write(`[${new Date().toISOString()}] TEST_START: ${test.title}\n`);
    logStream.write(`[${new Date().toISOString()}] TEST_FILE: ${test.location.file}\n`);
    logStream.write(`[${new Date().toISOString()}] TEST_LINE: ${test.location.line}\n`);
    logStream.write(`[${new Date().toISOString()}] TEST_COLUMN: ${test.location.column}\n`);
    logStream.write(`[${new Date().toISOString()}] LOG_FILE: ${logFile}\n\n`);

    // Store log stream for later use
    (test as any)._logStream = logStream;

    // Store cleanup function
    (test as any)._cleanupConsole = () => {
      logStream.write(`\n[${new Date().toISOString()}] TEST_END\n`);
      logStream.end();
    };
  }

  onStdOut(chunk: string | Buffer, test?: TestCase) {
    if (test && (test as any)._logToFile) {
      (test as any)._logToFile('STDOUT', chunk.toString());
    }
  }

  onStdErr(chunk: string | Buffer, test?: TestCase) {
    if (test && (test as any)._logToFile) {
      (test as any)._logToFile('STDERR', chunk.toString());
    }
  }

  onTestEnd(test: TestCase, result: TestResult) {
    const logFile = (test as any)._logFile;
    const logToFile = (test as any)._logToFile;
    const cleanupConsole = (test as any)._cleanupConsole;

    // Log test result to file
    if (logToFile) {
      logToFile('RESULT', `Test ${result.status}`);
      if (result.duration) {
        logToFile('DURATION', `${result.duration}ms`);
      }
      if (result.error) {
        logToFile('ERROR', result.error.message);
        logToFile('ERROR_STACK', result.error.stack || 'No stack trace');
      }
    }

    // Restore console
    if (cleanupConsole) {
      cleanupConsole();
    }

    // Get log file stats
    let fileSize = 0;
    try {
      const stats = fs.statSync(logFile);
      fileSize = stats.size;
    } catch (e) {
      // File might not exist or be accessible
    }

    if (result.status === 'passed') {
      // On success: minimal output (just a checkmark)
      process.stdout.write('✅');
    } else if (result.status === 'failed') {
      // On failure: show minimal info with log file path and size
      const relativePath = path.relative(process.cwd(), logFile);
      const sizeKB = (fileSize / 1024).toFixed(1);
      console.log(`\n❌ ${test.title}`);
      console.log(`   📄 Log: ${relativePath} (${sizeKB} KB)`);

      // Show error details briefly
      if (result.error) {
        console.log(`   💥 ${result.error.message.split('\n')[0]}`);
      }
    } else if (result.status === 'skipped') {
      process.stdout.write('⏭️');
    }
  }

  onEnd() {
    console.log('\n\n📊 Test Summary:');
    console.log(`   📁 All logs saved to: ${this.logsDir}`);
    console.log('   🔍 Failed test logs contain full error details and stack traces');
  }
}

export default MinimalReporter;
