//! Error types for sandbox operations.

/// Error type for sandbox operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Sandbox execution failed: {message}")]
    ExecutionFailed { message: String },

    #[error("Sandbox setup failed: {message}")]
    SetupFailed { message: String },

    #[error("No sandbox provider available")]
    NoAvailableProvider,

    #[error("Platform not supported: {platform}")]
    UnsupportedPlatform { platform: String },

    #[error("Sandbox provider error: {message}")]
    Provider { message: String },
}

impl Error {
    /// Create a new execution error.
    pub fn execution<S: Into<String>>(message: S) -> Self {
        Self::ExecutionFailed {
            message: message.into(),
        }
    }

    /// Create a new setup error.
    pub fn setup<S: Into<String>>(message: S) -> Self {
        Self::SetupFailed {
            message: message.into(),
        }
    }

    /// Create a new provider error.
    pub fn provider<S: Into<String>>(message: S) -> Self {
        Self::Provider {
            message: message.into(),
        }
    }
}
