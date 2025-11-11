/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { State } from '@livestore/livestore';
import { events, tables } from './schema';

// Materializers: map events to SQLite mutations
export const materializers = State.SQLite.materializers(events, {
  'v1.DraftCreated': ({
    id,
    prompt,
    repoMode,
    repoUrl,
    repoBranch,
    runtimeType,
    deliveryMode,
    agentsJson,
    createdAt,
    updatedAt,
  }) =>
    tables.drafts.insert({
      id,
      prompt,
      repoMode,
      repoUrl: repoUrl ?? null,
      repoBranch: repoBranch ?? null,
      runtimeType,
      deliveryMode,
      agentsJson,
      createdAt,
      updatedAt,
      deletedAt: null,
    }),

  'v1.DraftUpdated': ({
    id,
    prompt,
    repoMode,
    repoUrl,
    repoBranch,
    runtimeType,
    deliveryMode,
    agentsJson,
    updatedAt,
  }) =>
    tables.drafts
      .update({
        ...(prompt !== undefined && { prompt }),
        ...(repoMode !== undefined && { repoMode }),
        ...(repoUrl !== undefined && { repoUrl }),
        ...(repoBranch !== undefined && { repoBranch }),
        ...(runtimeType !== undefined && { runtimeType }),
        ...(deliveryMode !== undefined && { deliveryMode }),
        ...(agentsJson !== undefined && { agentsJson }),
        updatedAt,
      })
      .where({ id }),

  'v1.DraftDeleted': ({ id, deletedAt }) => tables.drafts.update({ deletedAt }).where({ id }),

  'v1.SessionUpserted': ({
    id,
    status,
    createdAt,
    prompt,
    repoUrl,
    repoBranch,
    agentType,
    agentVersion,
  }) =>
    tables.sessions
      .insert({
        id,
        status,
        createdAt,
        prompt,
        repoUrl: repoUrl ?? null,
        repoBranch: repoBranch ?? null,
        agentType,
        agentVersion,
        updatedAt: createdAt,
      })
      .onConflict('id')
      .doUpdate({
        status,
        prompt,
        repoUrl: repoUrl ?? null,
        repoBranch: repoBranch ?? null,
        agentType,
        agentVersion,
        updatedAt: createdAt,
      }),

  'v1.SessionStatusUpdated': ({ id, status, ts }) =>
    tables.sessions.update({ status, updatedAt: ts }).where({ id }),

  'v1.SessionEventReceived': payload =>
    tables.sessionEvents.insert({
      id: `${payload.sessionId}:${payload.ts.getTime()}:${payload.type}:${payload.tool_name ?? ''}`,
      sessionId: payload.sessionId,
      type: payload.type,
      payloadJson: JSON.stringify(payload),
      ts: payload.ts,
    }),
});
