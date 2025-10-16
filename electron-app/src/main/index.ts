import { app, BrowserWindow, ipcMain } from 'electron';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import Store from 'electron-store';
import guiCore from '@agent-harbor/gui-core';
const { helloFromRust, getPlatform } = guiCore;

// ES module equivalent of __dirname
const __dirname = dirname(fileURLToPath(import.meta.url));

// Initialize electron-store for persistent configuration
const store = new Store();

// Keep a global reference of the window object
let mainWindow: BrowserWindow | null = null;

// Application constants
const APP_NAME = 'Agent Harbor';
const WINDOW_WIDTH = 1200;
const WINDOW_HEIGHT = 800;

// Window state management
interface WindowState {
  width: number;
  height: number;
  x?: number;
  y?: number;
  isMaximized: boolean;
}

function saveWindowState(window: BrowserWindow): void {
  const bounds = window.getBounds();
  const isMaximized = window.isMaximized();

  const state: WindowState = {
    width: bounds.width,
    height: bounds.height,
    x: bounds.x,
    y: bounds.y,
    isMaximized,
  };

  store.set('windowState', state);
}

function restoreWindowState(): WindowState | null {
  return store.get('windowState') as WindowState | null;
}

function createMainWindow(): void {
  // Restore previous window state
  const windowState = restoreWindowState();

  // Create the browser window
  mainWindow = new BrowserWindow({
    width: windowState?.width || WINDOW_WIDTH,
    height: windowState?.height || WINDOW_HEIGHT,
    x: windowState?.x,
    y: windowState?.y,
    title: APP_NAME,
    titleBarStyle: process.platform === 'darwin' ? 'hiddenInset' : 'default',
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      preload: join(__dirname, 'renderer/preload.mjs'),
    },
    show: false, // Don't show until ready-to-show
  });

  // Restore maximized state
  if (windowState?.isMaximized) {
    mainWindow.maximize();
  }

  // Load the WebUI from localhost
  // In development: loads from mock server (localhost:3001)
  // In production: will load from `ah webui` subprocess (M0.4)
  const webuiUrl = process.env.WEBUI_URL || 'http://localhost:3001';

  mainWindow.loadURL(webuiUrl).catch((err) => {
    console.error('Failed to load WebUI:', err);
    // Show error page if WebUI fails to load
    if (mainWindow) {
      mainWindow.loadURL(`data:text/html,
        <html>
          <head><title>${APP_NAME} - Error</title></head>
          <body style="font-family: system-ui; padding: 40px; text-align: center;">
            <h1>🚢 ${APP_NAME}</h1>
            <p style="color: red;">Failed to load WebUI from ${webuiUrl}</p>
            <p><em>Make sure the WebUI server is running</em></p>
            <pre style="text-align: left; background: #f5f5f5; padding: 10px;">${err}</pre>
          </body>
        </html>
      `);
    }
  });

  // Show window when ready to prevent visual flash
  mainWindow.once('ready-to-show', () => {
    if (mainWindow) {
      mainWindow.show();
    }
  });

  // Handle window state changes
  mainWindow.on('resize', () => {
    if (mainWindow && !mainWindow.isMaximized()) {
      saveWindowState(mainWindow);
    }
  });

  mainWindow.on('move', () => {
    if (mainWindow && !mainWindow.isMaximized()) {
      saveWindowState(mainWindow);
    }
  });

  mainWindow.on('maximize', () => {
    if (mainWindow) {
      saveWindowState(mainWindow);
    }
  });

  mainWindow.on('unmaximize', () => {
    if (mainWindow) {
      saveWindowState(mainWindow);
    }
  });

  // Handle window closed
  mainWindow.on('closed', () => {
    mainWindow = null;
  });

  // Open DevTools in development
  if (process.env.NODE_ENV === 'development') {
    mainWindow.webContents.openDevTools();
  }
}

// Test native addon functionality on startup
console.log('Testing native addon...');
try {
  const helloMessage = helloFromRust();
  const rustPlatform = getPlatform();
  console.log('Native addon test:', { helloMessage, rustPlatform });
} catch (error) {
  console.error('Native addon test failed:', error);
}

// IPC handlers for renderer process communication
ipcMain.handle('get-app-version', () => {
  return app.getVersion();
});

ipcMain.handle('get-platform', () => {
  return process.platform;
});

ipcMain.handle('test-native-addon', () => {
  try {
    return {
      success: true,
      helloMessage: helloFromRust(),
      rustPlatform: getPlatform(),
    };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
});

ipcMain.handle('minimize-window', () => {
  if (mainWindow) {
    mainWindow.minimize();
  }
});

ipcMain.handle('maximize-window', () => {
  if (mainWindow) {
    if (mainWindow.isMaximized()) {
      mainWindow.unmaximize();
    } else {
      mainWindow.maximize();
    }
  }
});

ipcMain.handle('close-window', () => {
  if (mainWindow) {
    mainWindow.close();
  }
});

// App event handlers
app.whenReady().then(() => {
  createMainWindow();

  app.on('activate', () => {
    // On macOS it's common to re-create a window in the app when the
    // dock icon is clicked and there are no other windows open
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow();
    }
  });
});

// Quit when all windows are closed, except on macOS
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

// Security: Prevent new window creation
app.on('web-contents-created', (_event, contents) => {
  contents.setWindowOpenHandler(() => {
    // Prevent new window creation
    return { action: 'deny' };
  });
});
