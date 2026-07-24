//! The machine-local registry of known projects.
//!
//! The registry lives outside any project (in the user data directory) and
//! maps stable project IDs to on-disk locations. Portable, shareable project
//! settings live in each project's `.molt.toml` manifest instead.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::domain::BackendKind;

/// One registered project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    /// Stable identifier derived from the canonical path.
    pub id: String,
    /// Human-friendly name.
    pub name: String,
    /// Absolute path to the project root.
    pub path: PathBuf,
    /// Backend override for this project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendKind>,
    /// Name of the default environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_env: Option<String>,
    /// When the project was added (RFC3339 string).
    #[serde(default)]
    pub added: String,
}

/// The persisted registry document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
}

impl Registry {
    /// Path to the registry file (`$XDG_DATA_HOME/molt/registry.toml`).
    pub fn registry_path() -> Result<PathBuf> {
        let dir = dirs::data_dir().context("could not determine the user data directory")?;
        Ok(dir.join("molt").join("registry.toml"))
    }

    /// Load the registry, or return an empty one if none exists yet.
    pub fn load() -> Result<Registry> {
        let path = Self::registry_path()?;
        if !path.exists() {
            return Ok(Registry::default());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading registry from {}", path.display()))?;
        let registry: Registry = toml::from_str(&text)
            .with_context(|| format!("parsing registry at {}", path.display()))?;
        Ok(registry)
    }

    /// Persist the registry to disk, creating parent directories.
    pub fn save(&self) -> Result<()> {
        let path = Self::registry_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating data directory {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("serializing registry")?;
        std::fs::write(&path, text)
            .with_context(|| format!("writing registry to {}", path.display()))?;
        Ok(())
    }

    /// Find a project by its stable id.
    pub fn find_by_id(&self, id: &str) -> Option<&ProjectEntry> {
        self.projects.iter().find(|p| p.id == id)
    }

    /// Find a project whose canonical path matches `path`.
    pub fn find_by_path(&self, path: &Path) -> Option<&ProjectEntry> {
        let target = canonical(path);
        self.projects
            .iter()
            .find(|p| canonical(&p.path) == target)
    }

    /// Find a project by (case-insensitive) name.
    pub fn find_by_name(&self, name: &str) -> Option<&ProjectEntry> {
        self.projects
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Insert a new entry or update the existing one with the same path.
    pub fn upsert(&mut self, entry: ProjectEntry) {
        if let Some(existing) = self
            .projects
            .iter_mut()
            .find(|p| p.id == entry.id)
        {
            *existing = entry;
        } else {
            self.projects.push(entry);
        }
    }

    /// Remove a project by id, returning it if present.
    pub fn remove(&mut self, id: &str) -> Option<ProjectEntry> {
        if let Some(pos) = self.projects.iter().position(|p| p.id == id) {
            Some(self.projects.remove(pos))
        } else {
            None
        }
    }
}

/// Best-effort canonicalization that never fails.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Derive a short, stable id from a path using an FNV-1a hash.
///
/// Avoids pulling in a hashing dependency for such a small need.
pub fn project_id(path: &Path) -> String {
    let canonical = canonical(path);
    let bytes = canonical.to_string_lossy();
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x00000100000001B3);
    }
    format!("{hash:012x}")[..8].to_string()
}
