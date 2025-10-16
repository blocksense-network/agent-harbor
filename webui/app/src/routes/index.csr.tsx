import { createAsync, type RouteDefinition } from '@solidjs/router';
import { Show } from 'solid-js';
import { Title, Meta } from '@solidjs/meta';
import { TaskFeed } from '../components/sessions/TaskFeed.js';
import { apiClient } from '~/lib/api.js';

// Simple logger that respects quiet mode for testing
const logger = {
  log: (...args: unknown[]) => {
    const isQuietMode = process.env['QUIET_MODE'] === 'true' || process.env['NODE_ENV'] === 'test';
    if (!isQuietMode) {
      console.log(...args);
    }
  },
};

// CSR version: No route preloading needed - data is fetched client-side
export const route = {
  load: async () => {
    logger.log('[Route CSR] No preload needed - client-side data fetching');
  },
} satisfies RouteDefinition;

export default function Dashboard() {
  // CSR mode: Fetch data directly from API on the client
  const sessionsData = createAsync(async () => {
    logger.log('[Dashboard CSR] Fetching sessions from API...');
    const data = await apiClient.listSessions({ perPage: 50 });
    logger.log(`[Dashboard CSR] Fetched ${data.items.length} sessions`);
    return data;
  });

  const draftsData = createAsync(async () => {
    logger.log('[Dashboard CSR] Fetching drafts from API...');
    const result = await apiClient.listDrafts();
    logger.log(`[Dashboard CSR] Fetched ${result.items.length} drafts`);
    return result.items;
  });

  return (
    <>
      <Title>Agent Harbor — Dashboard</Title>
      <Meta
        name="description"
        content="Create and manage AI agent coding sessions with real-time monitoring"
      />
      <Show when={sessionsData() && draftsData()}>
        <TaskFeed
          initialSessions={sessionsData()!}
          initialDrafts={draftsData()!}
          onDraftTaskCreated={(taskId) => {
            console.log(`Task created: ${taskId}`);
            // Could add announcement here if needed
          }}
        />
      </Show>
    </>
  );
}
