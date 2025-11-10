/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

// Lightweight factory to create a LiveStore instance for the WebUI.
// We keep this module minimal so it can be mocked in unit tests.

import type { Store } from '@livestore/livestore';
import { events, tables } from './schema';
import { materializers } from './materializers';

export type AgentHarborStore = Store & {
  // Narrow API surface we rely on
  commit: (event: unknown) => Promise<void> | void;
};

type LiveStoreModule = {
  createStore?: (args: unknown) => Promise<AgentHarborStore> | AgentHarborStore;
  State?: {
    SQLite?: {
      createStore?: (args: unknown) => Promise<AgentHarborStore> | AgentHarborStore;
    };
  };
};

export async function createAgentHarborStore(): Promise<AgentHarborStore> {
  const live = (await import('@livestore/livestore')) as unknown as LiveStoreModule;
  if (!live || (!live.createStore && !live.State)) {
    throw new Error('LiveStore not available at runtime');
  }

  const creator = live.createStore ?? live.State?.SQLite?.createStore;
  if (typeof creator !== 'function') {
    throw new Error('Unsupported LiveStore runtime: missing createStore');
  }

  const store = await creator({ events, materializers, tables });
  return store as AgentHarborStore;
}

export { events, tables, materializers };
