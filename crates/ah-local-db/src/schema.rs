// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Database schema definitions and constants.

// Current schema version
pub const SCHEMA_VERSION: u32 = 1;

// Table names
pub const TABLE_SCHEMA_MIGRATIONS: &str = "schema_migrations";
pub const TABLE_REPOS: &str = "repos";
pub const TABLE_WORKSPACES: &str = "workspaces";
pub const TABLE_AGENTS: &str = "agents";
pub const TABLE_RUNTIMES: &str = "runtimes";
pub const TABLE_SESSIONS: &str = "sessions";
pub const TABLE_TASKS: &str = "tasks";
pub const TABLE_EVENTS: &str = "events";
pub const TABLE_FS_SNAPSHOTS: &str = "fs_snapshots";
pub const TABLE_KV: &str = "kv";

// Column names for repos table
pub mod repos {
    pub const ID: &str = "id";
    pub const VCS: &str = "vcs";
    pub const ROOT_PATH: &str = "root_path";
    pub const REMOTE_URL: &str = "remote_url";
    pub const DEFAULT_BRANCH: &str = "default_branch";
    pub const CREATED_AT: &str = "created_at";
}

// Column names for workspaces table
pub mod workspaces {
    pub const ID: &str = "id";
    pub const NAME: &str = "name";
    pub const EXTERNAL_ID: &str = "external_id";
    pub const CREATED_AT: &str = "created_at";
}

// Column names for agents table
pub mod agents {
    pub const ID: &str = "id";
    pub const NAME: &str = "name";
    pub const VERSION: &str = "version";
    pub const METADATA: &str = "metadata";
}

// Column names for runtimes table
pub mod runtimes {
    pub const ID: &str = "id";
    pub const TYPE: &str = "type";
    pub const DEVCONTAINER_PATH: &str = "devcontainer_path";
    pub const METADATA: &str = "metadata";
}

// Column names for sessions table
pub mod sessions {
    pub const ID: &str = "id";
    pub const REPO_ID: &str = "repo_id";
    pub const WORKSPACE_ID: &str = "workspace_id";
    pub const AGENT_ID: &str = "agent_id";
    pub const RUNTIME_ID: &str = "runtime_id";
    pub const MULTIPLEXER_KIND: &str = "multiplexer_kind";
    pub const MUX_SESSION: &str = "mux_session";
    pub const MUX_WINDOW: &str = "mux_window";
    pub const PANE_LEFT: &str = "pane_left";
    pub const PANE_RIGHT: &str = "pane_right";
    pub const PID_AGENT: &str = "pid_agent";
    pub const STATUS: &str = "status";
    pub const LOG_PATH: &str = "log_path";
    pub const WORKSPACE_PATH: &str = "workspace_path";
    pub const STARTED_AT: &str = "started_at";
    pub const ENDED_AT: &str = "ended_at";
}

// Column names for tasks table
pub mod tasks {
    pub const ID: &str = "id";
    pub const SESSION_ID: &str = "session_id";
    pub const PROMPT: &str = "prompt";
    pub const BRANCH: &str = "branch";
    pub const DELIVERY: &str = "delivery";
    pub const INSTANCES: &str = "instances";
    pub const LABELS: &str = "labels";
    pub const BROWSER_AUTOMATION: &str = "browser_automation";
    pub const BROWSER_PROFILE: &str = "browser_profile";
    pub const CHATGPT_USERNAME: &str = "chatgpt_username";
    pub const CODEX_WORKSPACE: &str = "codex_workspace";
}

// Column names for events table
pub mod events {
    pub const ID: &str = "id";
    pub const SESSION_ID: &str = "session_id";
    pub const TS: &str = "ts";
    pub const TYPE: &str = "type";
    pub const DATA: &str = "data";
}

// Column names for fs_snapshots table
pub mod fs_snapshots {
    pub const ID: &str = "id";
    pub const SESSION_ID: &str = "session_id";
    pub const TS: &str = "ts";
    pub const PROVIDER: &str = "provider";
    pub const REF: &str = "ref";
    pub const PATH: &str = "path";
    pub const PARENT_ID: &str = "parent_id";
    pub const METADATA: &str = "metadata";
}

// Column names for kv table
pub mod kv {
    pub const SCOPE: &str = "scope";
    pub const K: &str = "k";
    pub const V: &str = "v";
}
