/**
 * Electron main process entry point
 * 
 * This file initializes the Electron application and manages the main window lifecycle.
 * It serves as the Node.js backend for the GUI application.
 */

import { app, BrowserWindow } from 'electron';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Determine if running in development mode
const isDev = process.env.VITE_DEV_SERVER_URL !== undefined;

let mainWindow: BrowserWindow | null = null;

/**
 * Creates the main application window
 */
function createWindow(): void {
  mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    minWidth: 800,
    minHeight: 600,
    title: 'Agent Harbor',
    webPreferences: {
      preload: path.join(__dirname, '../renderer/preload.mjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
    show: false, // Don't show until ready-to-show event
  });

  // Show window when ready (prevents white flash)
  mainWindow.once('ready-to-show', () => {
    mainWindow?.show();
  });

  // Load the app
  if (isDev && process.env.VITE_DEV_SERVER_URL) {
    // Development: Load from Vite dev server
    mainWindow.loadURL(process.env.VITE_DEV_SERVER_URL);
    mainWindow.webContents.openDevTools();
  } else {
    // Production: Load from built files
    // For now, show a simple HTML page until WebUI integration is complete
    mainWindow.loadURL('data:text/html;charset=utf-8,' + encodeURIComponent(`
      <!DOCTYPE html>
      <html>
        <head>
          <meta charset="UTF-8">
          <title>Agent Harbor</title>
          <style>
            body {
              margin: 0;
              padding: 0;
              font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
              display: flex;
              justify-content: center;
              align-items: center;
              height: 100vh;
              background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
              color: white;
            }
            .container {
              text-align: center;
              padding: 2rem;
            }
            h1 {
              font-size: 3rem;
              margin: 0 0 1rem 0;
            }
            p {
              font-size: 1.25rem;
              opacity: 0.9;
            }
            .version {
              margin-top: 2rem;
              font-size: 0.875rem;
              opacity: 0.7;
            }
          </style>
        </head>
        <body>
          <div class="container">
            <h1>Agent Harbor GUI</h1>
            <p>Electron application initialized successfully!</p>
            <p class="version">Version ${app.getVersion()}</p>
          </div>
        </body>
      </html>
    `));
  }

  // Handle window closed event
  mainWindow.on('closed', () => {
    mainWindow = null;
  });
}

/**
 * Application lifecycle: ready
 * Create the main window when Electron has finished initialization
 */
app.whenReady().then(() => {
  createWindow();

  // macOS: Re-create window when dock icon is clicked and no windows are open
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

/**
 * Application lifecycle: all windows closed
 * Quit when all windows are closed, except on macOS
 */
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

/**
 * Security: Handle navigation events to prevent unauthorized redirects
 */
app.on('web-contents-created', (_, contents) => {
  contents.on('will-navigate', (event, navigationUrl) => {
    const parsedUrl = new URL(navigationUrl);
    
    // Allow navigation to dev server in development
    if (isDev && parsedUrl.origin === process.env.VITE_DEV_SERVER_URL) {
      return;
    }
    
    // Allow navigation to localhost (WebUI)
    if (parsedUrl.hostname === 'localhost' || parsedUrl.hostname === '127.0.0.1') {
      return;
    }
    
    // Block all other navigation attempts
    event.preventDefault();
    console.warn('Blocked navigation to:', navigationUrl);
  });
});
