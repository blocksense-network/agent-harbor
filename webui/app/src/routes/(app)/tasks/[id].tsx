/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import {
  Show,
  For,
  createSignal,
  createMemo,
  createResource,
  createEffect,
  onCleanup,
  onMount,
} from 'solid-js';
import { useNavigate, useParams } from '@solidjs/router';

import { useBreadcrumbs } from '../../../contexts/BreadcrumbContext';
import { apiClient } from '../../../lib/api.js';
import {
  mockModifiedFiles,
  getDiffContentForFile,
} from '../../../components/task-details/mock-data';
import { ModifiedFiles } from '../../../components/task-details/ModifiedFiles';
import { AgentActivity } from '../../../components/task-details/AgentActivity';
import { ChatBox } from '../../../components/task-details/ChatBox';
import { FileDiff } from '../../../components/task-details/FileDiff';

export default function TaskDetailsPage() {
  const params = useParams();
  const navigate = useNavigate();
  const { setBreadcrumbs } = useBreadcrumbs();

  const taskId = createMemo(() => params['id']);
  const [searchQuery, setSearchQuery] = createSignal('');
  const [statusFilter, setStatusFilter] = createSignal('all');

  const [taskData] = createResource(taskId, async id => {
    if (!id) return null;
    try {
      return await apiClient.getSession(id);
    } catch (error) {
      console.error('Failed to load task details:', error);
      return null;
    }
  });
  const task = () => taskData();

  createEffect(() => {
    const currentTask = task();
    const currentTaskId = taskId();

    if (currentTaskId && currentTask) {
      setBreadcrumbs([
        {
          label: 'workspace',
          onClick: () => navigate('/'),
        },
        {
          label: `session-${currentTaskId}`,
        },
        {
          label: `Task ${currentTaskId}`,
        },
      ]);
    } else {
      setBreadcrumbs([]);
    }
  });

  onCleanup(() => {
    setBreadcrumbs([]);
  });

  const filteredModifiedFiles = createMemo(() => {
    const query = searchQuery().toLowerCase();
    const status = statusFilter();

    return mockModifiedFiles.filter(file => {
      const matchesSearch = query === '' || file.path.toLowerCase().includes(query);
      const matchesStatus = status === 'all' || file.status === status;
      return matchesSearch && matchesStatus;
    });
  });

  const handleFileClick = (filePath: string) => {
    const anchorId = filePath.replace(/[^a-zA-Z0-9]/g, '-').toLowerCase();
    const element = document.getElementById(anchorId);

    if (element) {
      element.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
  };

  const handleNavigateToFile = (direction: 'prev' | 'next', currentIndex: number) => {
    const files = filteredModifiedFiles();
    const targetIndex = direction === 'prev' ? currentIndex - 1 : currentIndex + 1;

    if (targetIndex >= 0 && targetIndex < files.length) {
      const targetFile = files[targetIndex];
      if (targetFile) {
        handleFileClick(targetFile.path);
      }
    }
  };

  onMount(() => {
    if (typeof window === 'undefined') return;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        navigate('/');
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    onCleanup(() => window.removeEventListener('keydown', handleKeyDown));
  });

  return (
    <Show
      when={task()}
      fallback={
        <div class="flex min-h-screen items-center justify-center bg-gray-50">
          <div class="text-center">
            <h2 class="mb-2 text-xl font-semibold text-gray-900">Task not found</h2>
            <p class="text-gray-600">The requested task could not be loaded.</p>
          </div>
        </div>
      }
    >
      <div class="min-h-screen bg-gray-50" data-testid="task-details">
        <div class="flex h-[calc(100vh-80px)]">
          <div class="flex w-3/10 flex-col border-r border-gray-200 bg-white">
            <ModifiedFiles
              files={filteredModifiedFiles()}
              searchQuery={searchQuery()}
              onSearchChange={setSearchQuery}
              statusFilter={statusFilter()}
              onStatusFilterChange={setStatusFilter}
              onFileSelect={file => handleFileClick(file.path)}
            />
            <AgentActivity />
            <ChatBox />
          </div>
          <div class="w-7/10 bg-white">
            <div class="h-full overflow-y-auto">
              <For each={filteredModifiedFiles()}>
                {(file, index) => (
                  <FileDiff
                    file={file}
                    index={index()}
                    totalFiles={filteredModifiedFiles().length}
                    onNavigate={handleNavigateToFile}
                    diffContent={getDiffContentForFile(file.path)}
                  />
                )}
              </For>
            </div>
          </div>
        </div>
      </div>
    </Show>
  );
}
