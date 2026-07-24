//! User configuration.
//!
//! Configuration is layered: built-in defaults are overridden by the on-disk
//! `config.toml`, which is in turn overridden by command-line flags at
//! runtime. Every field uses `#[serde(default)]` so a partial file remains
//! valid, and everything that could be a setting *is* a setting.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::domain::BackendKind;
use crate::keymap::KeyConfig;

/// Which backend the user prefers. `Auto` picks `uv` when available and falls
/// back to the standard `venv` + `pip`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BackendPreference {
    #[default]
    Auto,
    Uv,
    Venv,
}

impl BackendPreference {
    /// The concrete backend requested, if any (None means "auto").
    pub fn forced(self) -> Option<BackendKind> {
        match self {
            BackendPreference::Auto => None,
            BackendPreference::Uv => Some(BackendKind::Uv),
            BackendPreference::Venv => Some(BackendKind::Venv),
        }
    }

    pub fn parse(value: &str) -> Option<BackendPreference> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(BackendPreference::Auto),
            "uv" => Some(BackendPreference::Uv),
            "venv" | "pip" | "std" => Some(BackendPreference::Venv),
            _ => None,
        }
    }
}

/// How a run should be presented by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    /// Capture output into the in-app console.
    #[default]
    Captured,
    /// Hand the full terminal to the child process.
    Terminal,
}

/// Python-related preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PythonConfig {
    /// Preferred Python version (e.g. `3.12`) when creating environments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_version: Option<String>,
    /// Default directory name for in-project environments.
    pub default_venv_name: String,
    /// Optional central directory for environments created outside projects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub central_env_dir: Option<PathBuf>,
    /// Extra directories to search for interpreters, in addition to PATH.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub search_paths: Vec<PathBuf>,
}

impl Default for PythonConfig {
    fn default() -> Self {
        PythonConfig {
            preferred_version: None,
            default_venv_name: ".venv".to_string(),
            central_env_dir: None,
            search_paths: Vec::new(),
        }
    }
}

/// Presentation preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Name of the active theme.
    pub theme: String,
    /// Use Unicode box-drawing / glyphs (false = ASCII fallback).
    pub unicode: bool,
    /// Enable mouse capture in the TUI.
    pub mouse: bool,
    /// UI refresh tick in milliseconds (also the input poll interval).
    pub tick_rate_ms: u64,
    /// `chrono`-style format string for timestamps.
    pub date_format: String,
    /// Per-token colour overrides applied on top of the theme.
    ///
    /// Kept last so TOML serialization emits this sub-table after the scalar
    /// fields (TOML forbids values after a table within the same table).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub theme_overrides: BTreeMap<String, String>,
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig {
            theme: "moss".to_string(),
            theme_overrides: BTreeMap::new(),
            unicode: true,
            mouse: false,
            tick_rate_ms: 120,
            date_format: "%Y-%m-%d %H:%M".to_string(),
        }
    }
}

/// Behavioural preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorConfig {
    /// Require confirmation before destructive actions (remove / rebuild).
    pub confirm_destructive: bool,
    /// Record a local run history.
    pub save_history: bool,
    /// Shell to use for `shell` and shell tasks. Falls back to `$SHELL`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// Editor for editing longer values. Falls back to `$EDITOR`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
    /// Default presentation for runs.
    pub default_run_mode: RunMode,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        BehaviorConfig {
            confirm_destructive: true,
            save_history: true,
            shell: None,
            editor: None,
            default_run_mode: RunMode::Captured,
        }
    }
}

/// The full, merged configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub backend: BackendPreference,
    pub python: PythonConfig,
    pub ui: UiConfig,
    pub behavior: BehaviorConfig,
    pub keys: KeyConfig,
}

impl Config {
    /// Path to the configuration file (`$XDG_CONFIG_HOME/molt/config.toml`).
    pub fn config_path() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .context("could not determine the user configuration directory")?;
        Ok(dir.join("molt").join("config.toml"))
    }

    /// Load configuration from disk, or return defaults when no file exists.
    pub fn load() -> Result<Config> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading configuration from {}", path.display()))?;
        let config: Config = toml::from_str(&text)
            .with_context(|| format!("parsing configuration at {}", path.display()))?;
        Ok(config)
    }

    /// Persist the configuration to disk, creating parent directories.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating configuration directory {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("serializing configuration")?;
        std::fs::write(&path, text)
            .with_context(|| format!("writing configuration to {}", path.display()))?;
        Ok(())
    }
}
