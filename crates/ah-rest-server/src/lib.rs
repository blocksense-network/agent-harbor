// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent Harbor REST API server
//!
//! This crate implements the REST API server for agent-harbor as specified in
//! the REST-Service specification. It provides endpoints for task creation,
//! session management, real-time event streaming, and capability discovery.

pub mod auth;
pub mod config;
pub mod dependencies;
pub mod error;
pub mod executor;
pub mod handlers;
pub mod middleware;
pub mod mock_dependencies;
pub mod models;
pub mod server;
pub mod services;
pub mod state;

pub use config::ServerConfig;
pub use error::{ServerError, ServerResult};
pub use server::Server;
