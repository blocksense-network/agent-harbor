import { cache, createAsync, type RouteDefinition } from "@solidjs/router";
import { Show } from "solid-js";
import { Title, Meta } from "@solidjs/meta";
import { TaskFeed } from "../components/sessions/TaskFeed.js";
import { getSessions, getDrafts } from "../lib/server-data.js";

// Simple logger that respects quiet mode for testing
const logger = {
  log: (...args: any[]) => {
    const isQuietMode = process.env.QUIET_MODE === 'true' || process.env.NODE_ENV === 'test';
    if (!isQuietMode) {
      console.log(...args);
    }
  }
};

// Cache server functions for SSR and client reuse
const getSessionsData = cache(async () => {
  "use server";
  logger.log("[Dashboard] Fetching sessions...");
  const data = await getSessions({ perPage: 50 });
  logger.log(`[Dashboard] Fetched ${data.items.length} sessions`);
  return data;
}, "sessions");

const getDraftsData = cache(async () => {
  "use server";
  logger.log("[Dashboard] Fetching drafts...");
  const data = await getDrafts();
  logger.log(`[Dashboard] Fetched ${data.length} drafts`);
  return data;
}, "drafts");

// Route definition - preload data during SSR
export const route = {
  load: async () => {
    // Preload both to ensure data is cached before component renders
    logger.log("[Route] Preloading data...");
    const [sessions, drafts] = await Promise.all([
      getSessionsData(),
      getDraftsData()
    ]);
    logger.log(`[Route] Preloaded ${sessions.items.length} sessions, ${drafts.length} drafts`);
  },
} satisfies RouteDefinition;

export default function Dashboard() {
  // Use cached data - deferStream: false blocks SSR until data is ready
  const sessionsData = createAsync(() => getSessionsData(), { deferStream: false });
  const draftsData = createAsync(() => getDraftsData(), { deferStream: false });

  // DraftProvider is now global (in app.tsx)
  // deferStream: false blocks SSR rendering until async data is ready
  return (
    <>
      <Title>Agent Harbor — Dashboard</Title>
      <Meta name="description" content="Create and manage AI agent coding sessions with real-time monitoring" />
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