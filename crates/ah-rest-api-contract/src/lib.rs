//! agent-harbor REST API contract types and validation
//!
//! This crate defines the schema types and validation for the REST API
//! as specified in REST-Service.md. These types are shared between
//! the mock server, production server, and REST client implementations.

pub mod error;
pub mod types;
pub mod validation;

pub use error::*;
pub use types::*;
