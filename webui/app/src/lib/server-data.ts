/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

// Server-side data fetching for SSR
// This file contains server functions that fetch data from the API server during SSR

import { cache } from '@solidjs/router';
import { DraftTask, Session } from './api.js';

// Simple logger that respects quiet mode for testing
const logger = {
  log: (...args: unknown[]) => {
    const isQuietMode = process.env['QUIET_MODE'] === 'true' || process.env['NODE_ENV'] === 'test';
    if (!isQuietMode) {
      console.log(...args);
    }
  },
};

// API base URL for SSR - uses SSR server's proxy
// The SSR server proxies /api/v1/* to the access point daemon
// SSR code calls back to itself (localhost:3002) to leverage the proxy
const API_BASE_URL = process.env['SSR_SERVER_URL'] || 'http://localhost:3002';

export interface SessionsResponse {
  items: Session[];
  pagination: {
    page: number;
    perPage: number;
    total: number;
    totalPages: number;
  };
}

/**
 * Fetch sessions from the API server during SSR
 * This is a cached server function that will be called during server-side rendering
 */
export const getSessions = cache(
  async (params?: {
    status?: string;
    projectId?: string;
    page?: number;
    perPage?: number;
  }): Promise<SessionsResponse> => {
    'use server';

    try {
      const queryParams = new URLSearchParams();
      if (params?.status) queryParams.append('status', params.status);
      if (params?.projectId) queryParams.append('projectId', params.projectId);
      if (params?.page) queryParams.append('page', params.page.toString());
      if (params?.perPage) queryParams.append('perPage', params.perPage.toString());

      const url = `${API_BASE_URL}/api/v1/sessions${queryParams.toString() ? `?${queryParams.toString()}` : ''}`;

      logger.log(`[SSR] Fetching sessions from: ${url}`);

      const response = await fetch(url, {
        headers: {
          Accept: 'application/json',
        },
      });

      if (!response.ok) {
        console.error(`[SSR] Failed to fetch sessions: ${response.status} ${response.statusText}`);
        throw new Error(`Failed to fetch sessions: ${response.statusText}`);
      }

      const data = await response.json();
      logger.log(`[SSR] Fetched ${data.items?.length || 0} sessions`);

      return data;
    } catch (error) {
      console.error('[SSR] Error fetching sessions:', error);
      // Return empty result on error to prevent SSR failure
      return {
        items: [],
        pagination: {
          page: 1,
          perPage: 20,
          total: 0,
          totalPages: 0,
        },
      };
    }
  },
  'sessions',
);

/**
 * Fetch agents list from the API server during SSR
 */
export const getAgents = cache(async (): Promise<Array<{ type: string; version: string }>> => {
  'use server';

  try {
    const url = `${API_BASE_URL}/api/v1/agents`;

    logger.log(`[SSR] Fetching agents from: ${url}`);

    const response = await fetch(url, {
      headers: {
        Accept: 'application/json',
      },
    });

    if (!response.ok) {
      console.error(`[SSR] Failed to fetch agents: ${response.status} ${response.statusText}`);
      // Return default agents on error
      return [
        { type: 'claude-code', version: 'latest' },
        { type: 'openhands', version: 'latest' },
      ];
    }

    const data = await response.json();
    return data.agents || [];
  } catch (error) {
    console.error('[SSR] Error fetching agents:', error);
    // Return default agents on error
    return [
      { type: 'claude-code', version: 'latest' },
      { type: 'openhands', version: 'latest' },
    ];
  }
}, 'agents');

/**
 * Fetch drafts from the API server during SSR
 */
export const getDrafts = cache(async (): Promise<DraftTask[]> => {
  'use server';

  try {
    const url = `${API_BASE_URL}/api/v1/drafts`;

    logger.log(`[SSR] Fetching drafts from: ${url}`);

    const response = await fetch(url, {
      headers: {
        Accept: 'application/json',
      },
    });

    if (!response.ok) {
      console.error(`[SSR] Failed to fetch drafts: ${response.status} ${response.statusText}`);
      return [];
    }

    const data = await response.json();
    logger.log(`[SSR] Fetched ${data.items?.length || 0} drafts`);
    return data.items || [];
  } catch (error) {
    console.error('[SSR] Error fetching drafts:', error);
    return [];
  }
}, 'drafts');
