/**
 * Preload script for secure IPC communication
 * 
 * This script runs in a privileged context and exposes a limited API
 * to the renderer process via contextBridge. This maintains security
 * by preventing direct Node.js access from the renderer.
 * 
 * See: https://www.electronjs.org/docs/latest/tutorial/context-isolation
 */

import { contextBridge, ipcRenderer } from 'electron';

/**
 * API exposed to the renderer process
 * Available in renderer as: window.electronAPI
 */
const electronAPI = {
  /**
   * Get application version
   */
  getVersion: (): Promise<string> => {
    return ipcRenderer.invoke('get-version');
  },

  /**
   * Placeholder for future IPC methods
   * These will be implemented as part of later milestones:
   * - WebUI process status queries
   * - Notification triggers
   * - Browser automation controls
   */
};

// Expose the API to the renderer process
contextBridge.exposeInMainWorld('electronAPI', electronAPI);

/**
 * Type definitions for the exposed API
 * This should be copied to a .d.ts file for TypeScript support in renderer
 */
declare global {
  interface Window {
    electronAPI: typeof electronAPI;
  }
}
