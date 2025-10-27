/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { For, Show, createMemo } from 'solid-js';
import { A } from '@solidjs/router';

import { Session } from '../../../lib/api.js';
import { useSessionLiveActivity } from './useSessionLiveActivity';

const getStatusIcon = (status: Session['status']) => {
  switch (status) {
    case 'running':
      return { icon: '●', color: 'text-green-600', bg: 'bg-green-50' };
    case 'queued':
      return { icon: '●', color: 'text-yellow-600', bg: 'bg-yellow-50' };
    case 'provisioning':
      return { icon: '●', color: 'text-blue-600', bg: 'bg-blue-50' };
    case 'pausing':
    case 'paused':
      return { icon: '⏸', color: 'text-orange-600', bg: 'bg-orange-50' };
    case 'resuming':
      return { icon: '●', color: 'text-blue-600', bg: 'bg-blue-50' };
    case 'stopping':
      return { icon: '⏹', color: 'text-red-600', bg: 'bg-red-50' };
    case 'stopped':
    case 'completed':
      return { icon: '✓', color: 'text-gray-600', bg: 'bg-gray-50' };
    case 'failed':
    case 'cancelled':
      return { icon: '✗', color: 'text-red-600', bg: 'bg-red-50' };
    default:
      return { icon: '?', color: 'text-gray-600', bg: 'bg-gray-50' };
  }
};

const formatSessionDate = (dateString: string) => {
  try {
    const date = new Date(dateString);
    const month = String(date.getMonth() + 1).padStart(2, '0');
    const day = String(date.getDate()).padStart(2, '0');
    const year = date.getFullYear();
    const hours = String(date.getHours()).padStart(2, '0');
    const minutes = String(date.getMinutes()).padStart(2, '0');

    return `${month}/${day}/${year} ${hours}:${minutes}`;
  } catch {
    return dateString;
  }
};

const getRepoName = (url?: string) => {
  if (!url) return 'Unknown';
  try {
    const match = url.match(/\/([^/]+)\.git$/);
    return match ? match[1] : url.split('/').pop() || 'Unknown';
  } catch {
    return 'Unknown';
  }
};

type SessionCardProps = {
  session: Session;
  isSelected?: boolean;
  onClick?: () => void;
  onStop?: () => void;
  onCancel?: () => void;
};

export const SessionCard = (props: SessionCardProps) => {
  const session = () => props.session;
  const { sessionStatus, liveActivityLines, canStop, canCancel } = useSessionLiveActivity(session);

  const statusInfo = createMemo(() => getStatusIcon(sessionStatus()));

  return (
    <article
      data-testid="task-card"
      id={`session-${session().id}`}
      data-task-id={session().id}
      aria-labelledby={`session-heading-${session().id}`}
      aria-selected={props.isSelected}
      class="rounded-lg border bg-white p-4 shadow-sm transition-all"
      classList={{
        'ring-2 ring-blue-500 border-blue-500 bg-blue-50 selected': props.isSelected,
        'border-gray-200': !props.isSelected,
      }}
      tabindex={props.isSelected ? '0' : '-1'}
    >
      <div class="mb-2 flex items-center justify-between">
        <div class="flex min-w-0 flex-1 items-center space-x-2">
          <span
            class={`
              text-sm
              ${statusInfo().color}
            `}
            aria-label={`Status: ${sessionStatus()}`}
          >
            <span aria-hidden="true">{statusInfo().icon}</span>
          </span>
          <h3
            id={`session-heading-${session().id}`}
            class={`
            min-w-0 flex-1 text-sm font-semibold
          `}
          >
            <A
              href={`/tasks/${session().id}`}
              data-testid="task-title-link"
              class={`
                cursor-pointer truncate text-gray-900
                hover:text-blue-600 hover:underline
                focus-visible:ring-2 focus-visible:ring-blue-500
                focus-visible:ring-offset-2
              `}
              title={session().prompt}
              onClick={e => {
                e.stopPropagation();
              }}
            >
              {session().prompt.length > 60
                ? `${session().prompt.slice(0, 60)}...`
                : session().prompt}
            </A>
          </h3>
        </div>

        <div class="flex space-x-1">
          <Show when={canStop()}>
            <button
              onClick={e => {
                e.stopPropagation();
                props.onStop?.();
              }}
              class={`
                rounded p-1 text-xs text-gray-400
                hover:bg-red-50 hover:text-red-600
                focus-visible:ring-2 focus-visible:ring-blue-500
                focus-visible:ring-offset-2
              `}
              title="Stop"
              aria-label="Stop session"
            >
              ⏹
            </button>
          </Show>
          <Show when={canCancel()}>
            <button
              onClick={e => {
                e.stopPropagation();
                props.onCancel?.();
              }}
              class={`
                rounded p-1 text-xs text-gray-400
                hover:bg-red-50 hover:text-red-600
                focus-visible:ring-2 focus-visible:ring-blue-500
                focus-visible:ring-offset-2
              `}
              title="Cancel"
              aria-label="Cancel session"
            >
              ✕
            </button>
          </Show>
        </div>
      </div>

      <div class="mb-2 flex items-center space-x-1.5 text-xs text-gray-600">
        <span aria-hidden="true">📁</span>
        <span class="max-w-[120px] truncate">{getRepoName(session().repo.url)}</span>
        <Show when={session().repo.branch}>
          <>
            <span class="text-gray-400">•</span>
            <span
              class={`
                max-w-[100px] truncate rounded bg-gray-100 px-1 py-0.5
                text-gray-700
              `}
            >
              {session().repo.branch}
            </span>
          </>
        </Show>
        <span class="text-gray-400">•</span>
        <span aria-hidden="true">🤖</span>
        <span class="truncate">
          {session().agent.type} v{session().agent.version}
        </span>
        <span class="text-gray-400">•</span>
        <span aria-hidden="true">🕒</span>
        <time datetime={session().createdAt} class="truncate">
          {formatSessionDate(session().createdAt)}
        </time>
      </div>

      <Show
        when={['running', 'queued', 'provisioning', 'paused', 'resuming', 'stopping'].includes(
          sessionStatus(),
        )}
      >
        <div class="space-y-0.5">
          <For each={liveActivityLines()}>
            {(activity, index) => (
              <div
                class="h-4 truncate overflow-hidden text-xs"
                classList={{
                  'text-blue-600': Boolean(activity && index() === 2),
                  'text-gray-600': Boolean(activity && index() !== 2),
                  'text-transparent': Boolean(!activity),
                }}
                title={activity || ''}
              >
                {activity || '\u00A0'}
              </div>
            )}
          </For>
        </div>
      </Show>
    </article>
  );
};
