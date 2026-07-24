//! Error types and process exit codes for Molt.
//!
//! The application uses [`anyhow`] for most error propagation. This module
//! defines a small domain error enum for well-known failure modes and the
//! stable exit codes that the CLI returns so shell scripts can react to them.

use thiserror::Error;

/// Stable process exit codes. Keeping these fixed makes `molt` scriptable.
pub mod exit {
    /// Everything went fine.
    pub const OK: i32 = 0;
    /// A generic, unexpected error occurred.
    pub const GENERIC: i32 = 1;
    /// The user passed invalid arguments.
    pub const USAGE: i32 = 2;
    /// No project / environment could be resolved for the request.
    pub const NOT_FOUND: i32 = 3;
    /// A required backend (uv / venv) was unavailable.
    pub const BACKEND: i32 = 4;
}

/// Well-known domain errors.
#[derive(Debug, Error)]
pub enum MoltError {
    /// No project could be resolved from the given location or name.
    #[error("no Molt project found for '{0}'")]
    ProjectNotFound(String),

    /// The requested environment does not exist in the project.
    #[error("environment '{0}' was not found")]
    EnvNotFound(String),

    /// No usable Python interpreter is available on the system.
    #[error("no Python interpreter could be found on this system")]
    NoPython,

    /// The selected backend is not installed / not on PATH.
    #[error("backend '{0}' is not available on this system")]
    BackendUnavailable(String),

    /// Configuration could not be parsed or was invalid.
    #[error("configuration error: {0}")]
    Config(String),
}

impl MoltError {
    /// Map a domain error to a stable process exit code.
    pub fn exit_code(&self) -> i32 {
        match self {
            MoltError::ProjectNotFound(_) | MoltError::EnvNotFound(_) => exit::NOT_FOUND,
            MoltError::NoPython | MoltError::BackendUnavailable(_) => exit::BACKEND,
            MoltError::Config(_) => exit::USAGE,
        }
    }
}
