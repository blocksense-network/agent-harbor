/**
 * SSE Manager - Centralized management of Server-Sent Events connections
 *
 * Ensures only ONE EventSource per session, even if multiple components
 * are interested in the same session's events.
 */

import { apiClient, SessionEvent } from './api.js';

type EventCallback = (event: SessionEvent) => void;

interface SubscriptionInfo {
  eventSource: EventSource;
  callbacks: Set<EventCallback>;
}

// Global registry of active SSE connections
const activeConnections = new Map<string, SubscriptionInfo>();

/**
 * Subscribe to SSE events for a session
 *
 * If a connection already exists for this session, reuses it.
 * If not, creates a new EventSource.
 *
 * @param sessionId - The session to subscribe to
 * @param callback - Function to call when events arrive
 * @returns Unsubscribe function
 */
export function subscribeToSession(sessionId: string, callback: EventCallback): () => void {
  console.log(`[SSEManager] Subscribe request for session ${sessionId}`);

  let subscription = activeConnections.get(sessionId);

  if (!subscription) {
    // No existing connection - create new one
    console.log(`[SSEManager] Creating NEW EventSource for session ${sessionId}`);
    const eventSource = apiClient.subscribeToSessionEvents(sessionId, event => {
      // Notify all callbacks interested in this session
      const sub = activeConnections.get(sessionId);
      if (sub) {
        sub.callbacks.forEach(cb => cb(event));
      }
    });

    subscription = {
      eventSource,
      callbacks: new Set(),
    };

    activeConnections.set(sessionId, subscription);
  } else {
    console.log(`[SSEManager] Reusing existing EventSource for session ${sessionId}`);
  }

  // Add this callback to the set
  subscription.callbacks.add(callback);
  console.log(
    `[SSEManager] Session ${sessionId} now has ${subscription.callbacks.size} subscribers`,
  );

  // Return unsubscribe function
  return () => {
    console.log(`[SSEManager] Unsubscribe request for session ${sessionId}`);
    const sub = activeConnections.get(sessionId);
    if (sub) {
      sub.callbacks.delete(callback);
      console.log(`[SSEManager] Session ${sessionId} now has ${sub.callbacks.size} subscribers`);

      // If no more callbacks, close the EventSource
      if (sub.callbacks.size === 0) {
        console.log(
          `[SSEManager] No more subscribers for session ${sessionId} - closing EventSource`,
        );
        sub.eventSource.close();
        activeConnections.delete(sessionId);
      }
    }
  };
}

/**
 * Get stats about active connections (for debugging)
 */
export function getConnectionStats() {
  return {
    activeSessions: activeConnections.size,
    sessions: Array.from(activeConnections.keys()).map(sessionId => ({
      sessionId,
      subscriberCount: activeConnections.get(sessionId)?.callbacks.size || 0,
    })),
  };
}
