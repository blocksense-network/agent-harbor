/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { DraftTask, DraftUpdate } from '../../../lib/api';
import type { AgentSelection } from './ModelMultiSelect';

export type DraftRepository = {
  id: string;
  name: string;
  url?: string;
  branch?: string;
  keywords?: string[];
};

export const agentToAgentSelection = (agent: DraftTask['agents'][number]): AgentSelection => ({
  model: `${agent.type.charAt(0).toUpperCase() + agent.type.slice(1)} ${agent.version.replace(/-/g, ' ')}`,
  instances: (agent as { instances?: number }).instances || 1,
});

export const selectionsToAgents = (selections: AgentSelection[]): DraftTask['agents'] =>
  selections.map(selection => {
    const [type, ...versionParts] = selection.model.toLowerCase().split(' ');
    return {
      type: type || 'unknown',
      version: versionParts.join('-') || 'latest',
      instances: selection.instances,
    };
  });

export const toRepository = (draft: DraftTask): DraftRepository | null =>
  draft.repo
    ? {
        id: draft.repo.url || 'unknown',
        name:
          draft.repo.url?.split('/').pop()?.replace('.git', '') || draft.repo.branch || 'unknown',
        ...(draft.repo.url !== undefined && { url: draft.repo.url }),
        ...(draft.repo.branch !== undefined && { branch: draft.repo.branch }),
      }
    : null;

export const buildRepositoryUpdate = (
  draft: DraftTask,
  repository: DraftRepository,
): NonNullable<DraftUpdate['repo']> => ({
  mode: (draft.repo?.mode ?? 'git') as DraftTask['repo']['mode'],
  ...(repository.url !== undefined && { url: repository.url }),
  ...(repository.branch !== undefined && { branch: repository.branch }),
});

export const buildBranchUpdate = (
  draft: DraftTask,
  branch: string | null,
): NonNullable<DraftUpdate['repo']> => ({
  mode: (draft.repo?.mode ?? 'git') as DraftTask['repo']['mode'],
  branch: branch || '',
  ...(draft.repo?.url !== undefined && { url: draft.repo.url }),
});

export const countAgents = (selections: AgentSelection[]): number =>
  selections.reduce((total, selection) => total + selection.instances, 0);
