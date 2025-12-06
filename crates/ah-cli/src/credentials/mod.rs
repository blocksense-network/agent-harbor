// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Credentials-related helpers for the CLI glue layer.

pub mod acquisition_service;
pub mod commands;

pub use acquisition_service::{AcquisitionService, StoredAcquisition};
pub use commands::{CredentialCommands, CredentialsArgs};
