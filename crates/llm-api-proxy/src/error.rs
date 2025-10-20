// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Error types for the LLM API proxy

/// Result type alias for operations that can fail
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for the LLM API proxy
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Configuration error: {message}")]
    Config { message: String },

    #[error("HTTP client error: {source}")]
    HttpClient {
        #[from]
        source: reqwest::Error,
    },

    #[error("API conversion error: {message}")]
    Conversion { message: String },

    #[error("Provider routing error: {message}")]
    Routing { message: String },

    #[error("Metrics collection error: {message}")]
    Metrics { message: String },

    #[error("Scenario playback error: {message}")]
    Scenario { message: String },

    #[error("Authentication error: {message}")]
    Authentication { message: String },

    #[error("Rate limit exceeded: {message}")]
    RateLimit { message: String },

    #[error("Serialization error: {source}")]
    Serialization {
        #[from]
        source: serde_json::Error,
    },

    #[error("YAML serialization error: {source}")]
    YamlSerialization {
        #[from]
        source: serde_yaml::Error,
    },

    #[error("IO error: {source}")]
    Io {
        #[from]
        source: std::io::Error,
    },

    #[error("Provider error: {provider} returned {status}: {message}")]
    Provider {
        provider: String,
        status: u16,
        message: String,
    },

    #[error("Timeout error: {message}")]
    Timeout { message: String },

    #[error("Unknown error: {message}")]
    Unknown { message: String },
}

// TODO: Uncomment when telemetry crate is integrated
// impl From<telemetry::Error> for Error {
//     fn from(err: telemetry::Error) -> Self {
//         Error::Metrics {
//             message: err.to_string(),
//         }
//     }
// }

// TODO: Uncomment when dynamic-router crate is integrated
// impl From<dynamic_router::Error> for Error {
//     fn from(err: dynamic_router::Error) -> Self {
//         Error::Routing {
//             message: err.to_string(),
//         }
//     }
// }

// TODO: Uncomment when weighted-balance crate is integrated
// impl From<weighted_balance::Error> for Error {
//     fn from(err: weighted_balance::Error) -> Self {
//         Error::Routing {
//             message: err.to_string(),
//         }
//     }
// }
