/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

// LiveStore schema and events for Agent Harbor WebUI data layer
// Reference: https://livestore.dev/

/* eslint-disable @typescript-eslint/no-explicit-any */
import * as LS from '@livestore/livestore';

// Use loose typings to insulate from upstream API changes while keeping this module stable
const Schema: any = (LS as any).Schema;
const State: any = (LS as any).State;
const Events: any = (LS as any).Events;

// Event definitions (event-sourced model)
export const events = {
  draftCreated: Events.synced({
    name: 'v1.DraftCreated',
    schema: Schema.Struct({
      id: Schema.String,
      prompt: Schema.String,
      repoMode: Schema.String,
      repoUrl: Schema.String.pipe(Schema.optional),
      repoBranch: Schema.String.pipe(Schema.optional),
      runtimeType: Schema.String,
      deliveryMode: Schema.String,
      agentsJson: Schema.String, // JSON stringified array of { type, version, instances }
      createdAt: Schema.Date,
      updatedAt: Schema.Date,
    }),
  }),

  draftUpdated: Events.synced({
    name: 'v1.DraftUpdated',
    schema: Schema.Struct({
      id: Schema.String,
      prompt: Schema.String.pipe(Schema.optional),
      repoMode: Schema.String.pipe(Schema.optional),
      repoUrl: Schema.String.pipe(Schema.optional),
      repoBranch: Schema.String.pipe(Schema.optional),
      runtimeType: Schema.String.pipe(Schema.optional),
      deliveryMode: Schema.String.pipe(Schema.optional),
      agentsJson: Schema.String.pipe(Schema.optional),
      updatedAt: Schema.Date,
    }),
  }),

  draftDeleted: Events.synced({
    name: 'v1.DraftDeleted',
    schema: Schema.Struct({ id: Schema.String, deletedAt: Schema.Date }),
  }),

  sessionUpserted: Events.synced({
    name: 'v1.SessionUpserted',
    schema: Schema.Struct({
      id: Schema.String,
      status: Schema.String,
      createdAt: Schema.Date,
      prompt: Schema.String,
      repoUrl: Schema.String.pipe(Schema.optional),
      repoBranch: Schema.String.pipe(Schema.optional),
      agentType: Schema.String,
      agentVersion: Schema.String,
    }),
  }),

  sessionStatusUpdated: Events.synced({
    name: 'v1.SessionStatusUpdated',
    schema: Schema.Struct({ id: Schema.String, status: Schema.String, ts: Schema.Date }),
  }),

  sessionEventReceived: Events.synced({
    name: 'v1.SessionEventReceived',
    schema: Schema.Struct({
      sessionId: Schema.String,
      type: Schema.String, // status | log | progress | thinking | tool_execution | file_edit
      level: Schema.String.pipe(Schema.optional),
      message: Schema.String.pipe(Schema.optional),
      thought: Schema.String.pipe(Schema.optional),
      progress: Schema.Number.pipe(Schema.optional),
      stage: Schema.String.pipe(Schema.optional),
      tool_name: Schema.String.pipe(Schema.optional),
      tool_args: Schema.String.pipe(Schema.optional),
      tool_status: Schema.String.pipe(Schema.optional),
      tool_output: Schema.String.pipe(Schema.optional),
      last_line: Schema.String.pipe(Schema.optional),
      file_path: Schema.String.pipe(Schema.optional),
      lines_added: Schema.Number.pipe(Schema.optional),
      lines_removed: Schema.Number.pipe(Schema.optional),
      ts: Schema.Date,
    }),
  }),
};

// SQLite table definitions used by materializers and queries
export const tables = {
  drafts: State.SQLite.table({
    name: 'drafts',
    columns: {
      id: State.SQLite.text({ primaryKey: true }),
      prompt: State.SQLite.text({ default: '' }),
      repoMode: State.SQLite.text({ default: 'git' }),
      repoUrl: State.SQLite.text({ nullable: true }),
      repoBranch: State.SQLite.text({ nullable: true }),
      runtimeType: State.SQLite.text({ default: 'devcontainer' }),
      deliveryMode: State.SQLite.text({ default: 'pr' }),
      agentsJson: State.SQLite.text({ default: '[]' }),
      createdAt: State.SQLite.integer({ schema: Schema.DateFromNumber }),
      updatedAt: State.SQLite.integer({ schema: Schema.DateFromNumber }),
      deletedAt: State.SQLite.integer({ nullable: true, schema: Schema.DateFromNumber }),
    },
  }),

  sessions: State.SQLite.table({
    name: 'sessions',
    columns: {
      id: State.SQLite.text({ primaryKey: true }),
      status: State.SQLite.text({ default: 'queued' }),
      createdAt: State.SQLite.integer({ schema: Schema.DateFromNumber }),
      prompt: State.SQLite.text({ default: '' }),
      repoUrl: State.SQLite.text({ nullable: true }),
      repoBranch: State.SQLite.text({ nullable: true }),
      agentType: State.SQLite.text({ default: '' }),
      agentVersion: State.SQLite.text({ default: '' }),
      updatedAt: State.SQLite.integer({ schema: Schema.DateFromNumber, nullable: true }),
    },
  }),

  sessionEvents: State.SQLite.table({
    name: 'session_events',
    columns: {
      id: State.SQLite.text({ primaryKey: true }), // `${sessionId}:${ts.getTime()}:${type}:${tool_name ?? ''}`
      sessionId: State.SQLite.text({}),
      type: State.SQLite.text({}),
      payloadJson: State.SQLite.text({}),
      ts: State.SQLite.integer({ schema: Schema.DateFromNumber }),
    },
  }),
};

export type DraftRow = typeof tables.drafts.Type;
export type SessionRow = typeof tables.sessions.Type;
export type SessionEventRow = typeof tables.sessionEvents.Type;
