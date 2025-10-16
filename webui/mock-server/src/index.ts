import express from 'express';
import cors from 'cors';
import helmet from 'helmet';
import morgan from 'morgan';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { sessionsRouter } from './routes/sessions.js';
import { agentsRouter } from './routes/agents.js';
import { runtimesRouter } from './routes/runtimes.js';
import { executorsRouter } from './routes/executors.js';
import { tasksRouter } from './routes/tasks.js';
import repositoriesRouter from './routes/repositories.js';
import draftsRouter from './routes/drafts.js';
import { ScenarioRunner } from './scenario-runner.js';

// ES module equivalent of __dirname
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Redirect output to file if SERVER_LOG_FILE is set
if (process.env.SERVER_LOG_FILE) {
  const logStream = fs.createWriteStream(process.env.SERVER_LOG_FILE, { flags: 'a' });
  // Store original stdout/stderr
  const originalStdoutWrite = process.stdout.write;
  const originalStderrWrite = process.stderr.write;

  // Redirect stdout and stderr to both original and file
  process.stdout.write = function(chunk: any, encoding?: any, callback?: any) {
    logStream.write(chunk, encoding, callback);
    return originalStdoutWrite.call(process.stdout, chunk, encoding, callback);
  };

  process.stderr.write = function(chunk: any, encoding?: any, callback?: any) {
    logStream.write(chunk, encoding, callback);
    return originalStderrWrite.call(process.stderr, chunk, encoding, callback);
  };
}

// Parse command line arguments
const args = process.argv.slice(2);
const scenarioFiles: string[] = [];
let mergeCompleted = false;

for (let i = 0; i < args.length; i++) {
  const arg = args[i];
  if (arg === '--scenario' || arg === '-s') {
    if (i + 1 < args.length) {
      scenarioFiles.push(args[i + 1]);
      i++; // Skip next arg
    }
  } else if (arg === '--merge-completed') {
    mergeCompleted = true;
  } else if (arg.startsWith('--scenario=')) {
    scenarioFiles.push(arg.split('=')[1]);
  }
}

// Simple logger that respects quiet mode
export const logger = {
  log: (...args: any[]) => {
    const isQuietMode = process.env.QUIET_MODE === 'true' || process.env.NODE_ENV === 'test';
    if (!isQuietMode) {
      console.log(...args);
    }
  },
  error: (...args: any[]) => {
    console.error(...args); // Always log errors
  }
};

const app = express();
const PORT = process.env.PORT || 3001;

// Determine if we should be quiet (for testing)
const isQuietMode = process.env.QUIET_MODE === 'true' || process.env.NODE_ENV === 'test';

// Middleware
app.use(helmet());
app.use(
  cors({
    origin: process.env.NODE_ENV === 'production' ? false : true,
    credentials: true,
  })
);
// Only use verbose logging when not in quiet mode
if (!isQuietMode) {
  app.use(morgan('combined'));
}
app.use(express.json());

// Health check
app.get('/health', (req, res) => {
  res.json({ status: 'ok', timestamp: new Date().toISOString() });
});

// Initialize scenario runner if scenarios are provided
let scenarioRunner: ScenarioRunner | null = null;
if (scenarioFiles.length > 0) {
  scenarioRunner = new ScenarioRunner(scenarioFiles, mergeCompleted);
  logger.log(`Loaded ${scenarioFiles.length} scenario(s): ${scenarioFiles.join(', ')}`);
}

// Static file serving for CSR build (for Electron integration)
// Serve static files from ../app/dist/client/ if it exists
const staticDir = path.join(__dirname, '../../app/dist/client');
if (fs.existsSync(staticDir)) {
  logger.log(`Serving static files from ${staticDir}`);

  // Serve static assets
  app.use(express.static(staticDir));

  // SPA fallback: serve index.html for all non-API routes
  // This must come AFTER API routes to avoid intercepting them
} else {
  logger.log('Static files directory not found - API-only mode');
}

// API routes
app.use('/api/v1/sessions', sessionsRouter);
app.use('/api/v1/agents', agentsRouter);
app.use('/api/v1/runtimes', runtimesRouter);
app.use('/api/v1/executors', executorsRouter);
app.use('/api/v1/tasks', tasksRouter);
app.use('/api/v1/repositories', repositoriesRouter);
app.use('/api/v1/drafts', draftsRouter);

// SPA fallback: serve index.html for all non-API routes
// This enables client-side routing to work properly
if (fs.existsSync(staticDir)) {
  // Use middleware instead of route for Express 5 compatibility
  app.use((req, res, next) => {
    // Only handle GET requests
    if (req.method !== 'GET') {
      return next();
    }

    // Don't serve index.html for API routes
    if (req.path.startsWith('/api/')) {
      return next();
    }

    // Don't serve index.html for static assets
    if (req.path.includes('.')) {
      return next();
    }

    // Serve index.html for all other GET requests (SPA routing)
    res.sendFile(path.join(staticDir, 'index.html'));
  });

  // 404 handler for API routes and missing assets
  app.use((req, res) => {
    res.status(404).json({
      type: 'https://docs.example.com/errors/not-found',
      title: 'Not Found',
      status: 404,
      detail: `Route ${req.originalUrl} not found`,
    });
  });
} else {
  // 404 handler for API-only mode
  app.use((req, res) => {
    res.status(404).json({
      type: 'https://docs.example.com/errors/not-found',
      title: 'Not Found',
      status: 404,
      detail: `Route ${req.originalUrl} not found`,
    });
  });
}

// Error handler
app.use((err: any, req: express.Request, res: express.Response, _next: express.NextFunction) => {
  // Handle JSON parsing errors specially in quiet mode
  if (err instanceof SyntaxError && 'body' in err) {
    // This is a JSON parsing error from body-parser
    if (isQuietMode) {
      // In quiet mode, return a simple 400 without logging
      return res.status(400).json({
        type: 'https://docs.example.com/errors/bad-request',
        title: 'Bad Request',
        status: 400,
        detail: 'Invalid JSON in request body',
      });
    }
  }

  // For other errors, or when not in quiet mode, log and return 500
  if (!isQuietMode) {
    console.error(err.stack);
  }
  res.status(500).json({
    type: 'https://docs.example.com/errors/internal-server-error',
    title: 'Internal Server Error',
    status: 500,
    detail: 'An unexpected error occurred',
  });
});

// Export scenario runner for use in routes
export { scenarioRunner };

app
  .listen(PORT, () => {
    if (!isQuietMode) {
      console.log(`Mock API server running on http://localhost:${PORT}`);
      console.log(`Health check: http://localhost:${PORT}/health`);
      if (scenarioRunner) {
        console.log(`Running ${scenarioFiles.length} scenario(s) in background`);
      }
    }
  })
  .on('error', (err) => {
    console.error('Server error:', err);
  });
