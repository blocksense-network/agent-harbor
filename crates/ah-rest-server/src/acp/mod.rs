// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent Client Protocol (ACP) gateway skeleton.
//!
//! This module houses the translation layer that will expose Agent Harbor over
//! ACP as described in the upstream specification:
//! - `resources/acp-specs/docs/overview/architecture.mdx`
//! - `resources/acp-specs/docs/protocol/overview.mdx`
//! - `resources/acp-specs/docs/protocol/transports.mdx`
//!
//! Milestone 0 focuses on bootstrapping configuration, error types, and a
//! gateway wrapper that can be wired alongside the existing REST server.
//! Subsequent milestones fill in transport handling, authentication, and
//! request/response translation.

pub mod errors;
pub mod gateway;
pub mod recorder;
pub mod translator;
pub mod transport;

/// Raw+typed payload wrapper for SDK-driven dispatch.
#[derive(Clone, Debug)]
pub struct RawAndTyped<T> {
    pub typed: T,
    pub raw: serde_json::Value,
}

pub use errors::{AcpError, AcpResult};
pub use gateway::{AcpGateway, GatewayHandle};
pub use translator::JsonRpcTranslator;
