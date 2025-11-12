/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { createEffect, onCleanup, onMount } from 'solid-js';
import { useNavigate } from '@solidjs/router';

import { useFocus } from '../../contexts/FocusContext';
import { useSession } from '../../contexts/SessionContext';
import { useDrafts } from '../../contexts/DraftContext';
import { type DraftTask } from '../../lib/api.js';
import { type SessionsResponse } from '../../lib/server-data.js';
import { TaskFeedHeader } from './TaskFeedHeader';
import { TaskFeedDraftsSection } from './draft/TaskFeedDraftsSection';
import { TaskFeedSessionsSection } from './session/TaskFeedSessionsSection';

type TaskFeedProps = {
  draftTasks?: DraftTask[];
  onDraftTaskCreated?: (taskId: string) => void;
  onDraftTaskRemoved?: (draftId: string) => void;
  initialSessions?: SessionsResponse;
  initialDrafts?: DraftTask[];
};

export const TaskFeed = (props: TaskFeedProps) => {
  const navigate = useNavigate();
  const {
    clearFocus,
    setSessionFocus,
    keyboardSelectedIndex,
    setKeyboardSelectedIndex,
    handleKeyboardNavigation,
    getActiveDescendantId,
    registerCollections,
  } = useFocus();
  const { drafts, setInitialDrafts, registerDraftCallbacks, requestDraftsRefresh } = useDrafts();
  const {
    filteredSessions,
    statusFilter,
    requestSessionsRefresh,
    setInitialSessions,
    setSelectedSessionId,
  } = useSession();

  onMount(() => {
    if (props.initialDrafts || props.draftTasks) {
      setInitialDrafts(props.initialDrafts ?? props.draftTasks);
    }
    setInitialSessions(props.initialSessions);
    requestSessionsRefresh();
    requestDraftsRefresh();

    registerDraftCallbacks({
      onTaskCreated: taskId => {
        props.onDraftTaskCreated?.(taskId);
        requestSessionsRefresh();
      },
      onDraftRemoved: draftId => {
        props.onDraftTaskRemoved?.(draftId);
      },
    });

    onCleanup(() => {
      registerDraftCallbacks({});
    });
  });

  createEffect(() => {
    statusFilter();
    setKeyboardSelectedIndex(-1);
    setSelectedSessionId(undefined);
    clearFocus();
  });

  createEffect(() => {
    registerCollections({
      drafts,
      sessions: filteredSessions,
      onDraftFocus: () => {
        setSelectedSessionId(undefined);
      },
      onSessionFocus: sessionId => {
        setSelectedSessionId(sessionId);
      },
    });
    onCleanup(() => {
      registerCollections({ drafts: () => [], sessions: () => [] });
    });
  });

  return (
    <section
      data-testid="task-feed"
      class="flex h-full flex-col"
      role="region"
      aria-label="Task feed"
    >
      <TaskFeedHeader />

      <section
        class="flex-1 overflow-y-auto"
        role="region"
        tabindex="0"
        aria-activedescendant={getActiveDescendantId()}
        aria-label="Task list navigation"
        onKeyDown={event => {
          handleKeyboardNavigation(event);
          if (event.key === 'Enter') {
            const idx = keyboardSelectedIndex();
            const draftList = drafts();
            if (idx >= draftList.length) {
              const sessionIndex = idx - draftList.length;
              const session = filteredSessions()[sessionIndex];
              if (session) {
                setSessionFocus(session.id);
                setSelectedSessionId(session.id);
                navigate(`/tasks/${session.id}`);
              }
            }
          }
        }}
      >
        <div class="p-4">
          <TaskFeedDraftsSection />

          <TaskFeedSessionsSection />
        </div>
      </section>
    </section>
  );
};
