import { contextBridge, ipcRenderer } from 'electron';

// Expose protected methods that allow the renderer process to use
// the ipcRenderer without exposing the entire object
// Expose Agent Harbor configuration
contextBridge.exposeInMainWorld('agentHarborConfig', {
  isElectron: true,
  apiBaseUrl: process.env.WEBUI_URL || 'http://localhost:3001',
  platform: process.platform,
});

contextBridge.exposeInMainWorld('electronAPI', {
  // App information
  getAppVersion: () => ipcRenderer.invoke('get-app-version'),
  getPlatform: () => ipcRenderer.invoke('get-platform'),

  // Window controls
  minimizeWindow: () => ipcRenderer.invoke('minimize-window'),
  maximizeWindow: () => ipcRenderer.invoke('maximize-window'),
  closeWindow: () => ipcRenderer.invoke('close-window'),

  // IPC communication channel (for future WebUI integration)
  send: (channel: string, data: any) => {
    // Whitelist of allowed channels
    const validChannels = [
      'webui-status-request',
      'webui-health-check',
      'notification-trigger',
      'browser-automation-request',
    ];

    if (validChannels.includes(channel)) {
      ipcRenderer.send(channel, data);
    }
  },

  receive: (channel: string, func: (...args: any[]) => void) => {
    // Whitelist of allowed channels
    const validChannels = [
      'webui-status-update',
      'webui-health-update',
      'notification-response',
      'browser-automation-response',
    ];

    if (validChannels.includes(channel)) {
      // Deliberately strip event as it includes `sender`
      ipcRenderer.on(channel, (_event, ...args) => func(...args));
    }
  },

  // Remove all listeners for a channel
  removeAllListeners: (channel: string) => {
    const validChannels = [
      'webui-status-update',
      'webui-health-update',
      'notification-response',
      'browser-automation-response',
    ];

    if (validChannels.includes(channel)) {
      ipcRenderer.removeAllListeners(channel);
    }
  },
});

// Type definitions for the exposed API (for TypeScript consumers)
declare global {
  interface Window {
    electronAPI: {
      getAppVersion: () => Promise<string>;
      getPlatform: () => Promise<string>;
      minimizeWindow: () => Promise<void>;
      maximizeWindow: () => Promise<void>;
      closeWindow: () => Promise<void>;
      send: (channel: string, data: any) => void;
      receive: (channel: string, func: (...args: any[]) => void) => void;
      removeAllListeners: (channel: string) => void;
    };
  }
}
