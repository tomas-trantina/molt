//! Core domain types shared across the whole application.
//!
//! These types are deliberately free of I/O and UI concerns so they can be
//! reused by the service layer, the CLI and the TUI, and unit-tested in
//! isolation.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Which tool backs environment and package operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    /// The `uv` tool from Astral (fast, recommended when available).
    Uv,
    /// The standard library `venv` module plus `pip`.
    Venv,
}

impl BackendKind {
    /// A short, stable label for display and serialization.
    pub fn label(self) -> &'static str {
        match self {
            BackendKind::Uv => "uv",
            BackendKind::Venv => "venv",
        }
    }

    /// Parse a backend name from user input.
    pub fn parse(value: &str) -> Option<BackendKind> {
        match value.trim().to_ascii_lowercase().as_str() {
            "uv" => Some(BackendKind::Uv),
            "venv" | "pip" | "std" | "standard" => Some(BackendKind::Venv),
            _ => None,
        }
    }
}

/// The health of a resolved environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvStatus {
    /// The environment exists and its interpreter is runnable.
    Ready,
    /// The environment directory exists but contains no interpreter.
    Empty,
    /// The environment is declared but the directory does not exist.
    Missing,
    /// The interpreter is present but could not be executed.
    Broken,
    /// Status could not be determined.
    Unknown,
}

impl EnvStatus {
    pub fn label(self) -> &'static str {
        match self {
            EnvStatus::Ready => "ready",
            EnvStatus::Empty => "empty",
            EnvStatus::Missing => "missing",
            EnvStatus::Broken => "broken",
            EnvStatus::Unknown => "unknown",
        }
    }
}

/// A declared environment in a project manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvSpec {
    /// Logical name (e.g. `default`, `test`).
    pub name: String,
    /// Path to the environment, relative to the project root. Defaults to
    /// `.venv` when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Requested Python version or interpreter path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python: Option<String>,
    /// Default requirements file used when installing into this environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirements: Option<String>,
}

impl EnvSpec {
    /// The environment directory relative path, defaulting to `.venv`.
    pub fn relative_path(&self) -> String {
        self.path.clone().unwrap_or_else(|| ".venv".to_string())
    }
}

/// How a task should be launched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    /// Run a Python script file with the environment interpreter.
    #[default]
    Script,
    /// Run a Python module (`python -m <module>`).
    Module,
    /// Run an arbitrary shell command line.
    Shell,
    /// Execute a program directly (found on the environment PATH).
    Exec,
}

/// A saved, runnable task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub name: String,
    #[serde(default)]
    pub kind: TaskKind,
    /// Script path, module name, program name, or shell command line.
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Working directory relative to the project root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Optional dotenv-style file to load before running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Task {
    /// A compact one-line preview of what the task runs.
    pub fn preview(&self) -> String {
        let args = if self.args.is_empty() {
            String::new()
        } else {
            format!(" {}", self.args.join(" "))
        };
        match self.kind {
            TaskKind::Script => format!("python {}{}", self.command, args),
            TaskKind::Module => format!("python -m {}{}", self.command, args),
            TaskKind::Shell => self.command.clone(),
            TaskKind::Exec => format!("{}{}", self.command, args),
        }
    }
}

/// An installed package, as reported by the backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub version: String,
}

/// A resolved, runtime view of an environment (declared or discovered).
#[derive(Debug, Clone)]
pub struct EnvInfo {
    pub name: String,
    pub path: PathBuf,
    pub python_version: Option<String>,
    pub status: EnvStatus,
    pub backend: BackendKind,
    pub package_count: Option<usize>,
}
