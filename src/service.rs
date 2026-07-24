//! High-level application services shared by the CLI and the TUI.
//!
//! The service owns the merged [`Config`] and the project [`Registry`], and
//! exposes intention-level operations (import a project, create an
//! environment, install requirements, run a task, ...). It deliberately
//! returns [`PlannedCommand`]s or [`RunHandle`]s for anything long-running so
//! the caller decides how output is presented.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::backend::{self, python_exe, Backend};
use crate::config::Config;
use crate::domain::{BackendKind, EnvInfo, EnvSpec, EnvStatus, Package, Task};
use crate::pyfinder;
use crate::registry::{self, ProjectEntry, Registry};
use crate::runner::{self, PlannedCommand, RunHandle};

/// The name of the per-project manifest file.
pub const MANIFEST_FILE: &str = ".molt.toml";

/// Portable, shareable per-project settings stored in `.molt.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectManifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendKind>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<EnvSpec>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<Task>,
}

impl ProjectManifest {
    /// Load a manifest from a project directory, or default if absent.
    pub fn load(project_root: &Path) -> Result<ProjectManifest> {
        let path = project_root.join(MANIFEST_FILE);
        if !path.exists() {
            return Ok(ProjectManifest::default());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading manifest {}", path.display()))?;
        let manifest = toml::from_str(&text)
            .with_context(|| format!("parsing manifest {}", path.display()))?;
        Ok(manifest)
    }

    /// Save the manifest into a project directory.
    pub fn save(&self, project_root: &Path) -> Result<()> {
        let path = project_root.join(MANIFEST_FILE);
        let text = toml::to_string_pretty(self).context("serializing manifest")?;
        std::fs::write(&path, text)
            .with_context(|| format!("writing manifest {}", path.display()))?;
        Ok(())
    }
}

/// A fully resolved view of a project for display and action.
#[derive(Debug, Clone)]
pub struct ProjectView {
    pub entry: ProjectEntry,
    pub manifest: ProjectManifest,
    pub environments: Vec<EnvInfo>,
    pub tasks: Vec<Task>,
    pub scripts: Vec<String>,
    pub requirements_files: Vec<String>,
}

impl ProjectView {
    /// The environment selected by name, or the project default, or the first.
    pub fn env(&self, name: Option<&str>) -> Option<&EnvInfo> {
        if let Some(name) = name {
            return self.environments.iter().find(|e| e.name == name);
        }
        if let Some(default) = &self.entry.default_env {
            if let Some(found) = self.environments.iter().find(|e| &e.name == default) {
                return Some(found);
            }
        }
        self.environments.first()
    }
}

/// The application service.
pub struct Service {
    pub config: Config,
    pub registry: Registry,
}

impl Service {
    /// Build a service from persisted configuration and registry.
    pub fn load() -> Result<Service> {
        Ok(Service {
            config: Config::load()?,
            registry: Registry::load()?,
        })
    }

    // ----- Projects ---------------------------------------------------------

    /// All registered projects.
    pub fn projects(&self) -> &[ProjectEntry] {
        &self.registry.projects
    }

    /// Resolve a project entry from an explicit name, path, or the current dir.
    pub fn resolve_entry(&self, selector: Option<&str>) -> Result<ProjectEntry> {
        if let Some(selector) = selector {
            if let Some(found) = self.registry.find_by_name(selector) {
                return Ok(found.clone());
            }
            let path = PathBuf::from(selector);
            if path.exists() {
                if let Some(found) = self.registry.find_by_path(&path) {
                    return Ok(found.clone());
                }
                return Ok(entry_from_path(&path));
            }
            return Err(anyhow!("no project named or located at '{selector}'"));
        }

        // Fall back to the current directory (walk up to find a manifest).
        let cwd = std::env::current_dir().context("reading the current directory")?;
        if let Some(root) = find_project_root(&cwd) {
            if let Some(found) = self.registry.find_by_path(&root) {
                return Ok(found.clone());
            }
            return Ok(entry_from_path(&root));
        }
        if let Some(found) = self.registry.find_by_path(&cwd) {
            return Ok(found.clone());
        }
        Err(anyhow!(
            "no project specified and none found from the current directory"
        ))
    }

    /// Import (register) a project directory, returning the stored entry.
    pub fn import_project(&mut self, path: &Path) -> Result<ProjectEntry> {
        if !path.exists() {
            return Err(anyhow!("path does not exist: {}", path.display()));
        }
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let mut entry = entry_from_path(&canonical);
        // Prefer a name/backend from an existing manifest.
        let manifest = ProjectManifest::load(&canonical)?;
        if let Some(name) = manifest.name.clone() {
            entry.name = name;
        }
        if let Some(backend) = manifest.backend {
            entry.backend = Some(backend);
        }
        self.registry.upsert(entry.clone());
        self.registry.save()?;
        Ok(entry)
    }

    /// Create a brand-new project directory with a manifest.
    pub fn create_project(
        &mut self,
        name: &str,
        path: &Path,
        python: Option<&str>,
        backend: Option<BackendKind>,
    ) -> Result<ProjectEntry> {
        std::fs::create_dir_all(path)
            .with_context(|| format!("creating project directory {}", path.display()))?;
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

        let manifest = ProjectManifest {
            name: Some(name.to_string()),
            python: python.map(|s| s.to_string()),
            backend,
            environments: vec![EnvSpec {
                name: "default".to_string(),
                path: Some(self.config.python.default_venv_name.clone()),
                python: python.map(|s| s.to_string()),
                requirements: Some("requirements.txt".to_string()),
            }],
            tasks: Vec::new(),
        };
        manifest.save(&canonical)?;

        let mut entry = entry_from_path(&canonical);
        entry.name = name.to_string();
        entry.backend = backend;
        entry.default_env = Some("default".to_string());
        self.registry.upsert(entry.clone());
        self.registry.save()?;
        Ok(entry)
    }

    /// Forget a project (does not touch files on disk).
    pub fn remove_project(&mut self, id: &str) -> Result<Option<ProjectEntry>> {
        let removed = self.registry.remove(id);
        if removed.is_some() {
            self.registry.save()?;
        }
        Ok(removed)
    }

    /// Build a resolved view of a project.
    pub fn load_view(&self, entry: &ProjectEntry) -> Result<ProjectView> {
        let manifest = ProjectManifest::load(&entry.path)?;
        let backend_kind = self.effective_backend_kind(entry, &manifest);
        let environments = self.resolve_environments(entry, &manifest, backend_kind);
        let scripts = discover_scripts(&entry.path);
        let requirements_files = discover_requirements(&entry.path);
        Ok(ProjectView {
            entry: entry.clone(),
            tasks: manifest.tasks.clone(),
            manifest,
            environments,
            scripts,
            requirements_files,
        })
    }

    // ----- Backend ----------------------------------------------------------

    /// The backend kind that would be used for a project (for display).
    pub fn effective_backend_kind(
        &self,
        entry: &ProjectEntry,
        manifest: &ProjectManifest,
    ) -> BackendKind {
        let requested = entry
            .backend
            .or(manifest.backend)
            .or_else(|| self.config.backend.forced());
        match requested {
            Some(kind) => kind,
            None => {
                if backend::UvBackend::is_available() {
                    BackendKind::Uv
                } else {
                    BackendKind::Venv
                }
            }
        }
    }

    /// Resolve a concrete backend implementation for a project.
    pub fn backend_for(
        &self,
        entry: &ProjectEntry,
        manifest: &ProjectManifest,
    ) -> Result<Box<dyn Backend>> {
        let base_python = self.base_python(manifest);
        backend::resolve_backend(
            self.config.backend,
            entry.backend.or(manifest.backend),
            &base_python,
        )
    }

    /// Determine the base interpreter used to bootstrap venvs.
    fn base_python(&self, manifest: &ProjectManifest) -> String {
        let requested = manifest
            .python
            .clone()
            .or_else(|| self.config.python.preferred_version.clone());
        if let Some(found) =
            pyfinder::best_match(requested.as_deref(), &self.config.python.search_paths)
        {
            return found.path.to_string_lossy().to_string();
        }
        // Fall back to a name on PATH.
        "python3".to_string()
    }

    // ----- Environments -----------------------------------------------------

    fn resolve_environments(
        &self,
        entry: &ProjectEntry,
        manifest: &ProjectManifest,
        backend_kind: BackendKind,
    ) -> Vec<EnvInfo> {
        let mut specs = manifest.environments.clone();
        if specs.is_empty() {
            // Fall back to the conventional in-project environment.
            specs.push(EnvSpec {
                name: "default".to_string(),
                path: Some(self.config.python.default_venv_name.clone()),
                python: manifest.python.clone(),
                requirements: None,
            });
        }
        specs
            .into_iter()
            .map(|spec| {
                let path = entry.path.join(spec.relative_path());
                let (status, version) = probe_env(&path);
                EnvInfo {
                    name: spec.name,
                    path,
                    python_version: version,
                    status,
                    backend: backend_kind,
                    package_count: None,
                }
            })
            .collect()
    }

    /// Plan environment creation. Returns the commands to run in order.
    pub fn plan_create_env(
        &self,
        view: &ProjectView,
        env_name: &str,
    ) -> Result<Vec<PlannedCommand>> {
        let spec = self.env_spec(view, env_name)?;
        let env_path = view.entry.path.join(spec.relative_path());
        let backend = self.backend_for(&view.entry, &view.manifest)?;
        let python = spec
            .python
            .clone()
            .or_else(|| view.manifest.python.clone())
            .or_else(|| self.config.python.preferred_version.clone());
        backend.create_env(&env_path, python.as_deref())
    }

    /// Plan installation from a requirements file into an environment.
    ///
    /// This is the primary "install requirements.txt" entry point.
    pub fn plan_install_requirements(
        &self,
        view: &ProjectView,
        env_name: &str,
        requirements: Option<&str>,
    ) -> Result<PlannedCommand> {
        let spec = self.env_spec(view, env_name)?;
        let env_path = view.entry.path.join(spec.relative_path());
        let file_name = requirements
            .map(|s| s.to_string())
            .or_else(|| spec.requirements.clone())
            .or_else(|| view.requirements_files.first().cloned())
            .unwrap_or_else(|| "requirements.txt".to_string());
        let file_path = resolve_relative(&view.entry.path, &file_name);
        if !file_path.exists() {
            return Err(anyhow!(
                "requirements file not found: {}",
                file_path.display()
            ));
        }
        let backend = self.backend_for(&view.entry, &view.manifest)?;
        Ok(backend.install_requirements(&env_path, &file_path))
    }

    /// Plan installation of explicit packages.
    pub fn plan_add_packages(
        &self,
        view: &ProjectView,
        env_name: &str,
        packages: &[String],
    ) -> Result<PlannedCommand> {
        let env_path = self.env_path(view, env_name)?;
        let backend = self.backend_for(&view.entry, &view.manifest)?;
        Ok(backend.install_packages(&env_path, packages))
    }

    /// Plan removal of explicit packages.
    pub fn plan_remove_packages(
        &self,
        view: &ProjectView,
        env_name: &str,
        packages: &[String],
    ) -> Result<PlannedCommand> {
        let env_path = self.env_path(view, env_name)?;
        let backend = self.backend_for(&view.entry, &view.manifest)?;
        Ok(backend.uninstall_packages(&env_path, packages))
    }

    /// List installed packages for an environment.
    pub fn list_packages(&self, view: &ProjectView, env_name: &str) -> Result<Vec<Package>> {
        let env_path = self.env_path(view, env_name)?;
        let backend = self.backend_for(&view.entry, &view.manifest)?;
        backend.list_packages(&env_path)
    }

    /// Delete an environment directory from disk.
    pub fn remove_env(&self, view: &ProjectView, env_name: &str) -> Result<()> {
        let env_path = self.env_path(view, env_name)?;
        if env_path.exists() {
            std::fs::remove_dir_all(&env_path)
                .with_context(|| format!("removing environment {}", env_path.display()))?;
        }
        Ok(())
    }

    // ----- Running ----------------------------------------------------------

    /// Build the command to run a task inside an environment.
    pub fn plan_task(&self, view: &ProjectView, task: &Task, env_name: &str) -> Result<PlannedCommand> {
        let env_path = self.env_path(view, env_name)?;
        let env_vars = self.activation_env(&env_path, task.env_file.as_deref(), &view.entry.path)?;
        let cwd = match &task.cwd {
            Some(dir) => resolve_relative(&view.entry.path, dir),
            None => view.entry.path.clone(),
        };
        let python = python_exe(&env_path);
        let command = match task.kind {
            crate::domain::TaskKind::Script => PlannedCommand::new(python.to_string_lossy().to_string())
                .arg(task.command.clone())
                .args(task.args.clone()),
            crate::domain::TaskKind::Module => PlannedCommand::new(python.to_string_lossy().to_string())
                .arg("-m")
                .arg(task.command.clone())
                .args(task.args.clone()),
            crate::domain::TaskKind::Exec => {
                PlannedCommand::new(task.command.clone()).args(task.args.clone())
            }
            crate::domain::TaskKind::Shell => {
                let shell = self.shell();
                PlannedCommand::new(shell).arg("-c").arg(task.command.clone())
            }
        };
        Ok(command.cwd(cwd).envs(env_vars))
    }

    /// Build the command to run a Python script file directly.
    pub fn plan_script(
        &self,
        view: &ProjectView,
        script: &str,
        args: &[String],
        env_name: &str,
    ) -> Result<PlannedCommand> {
        let env_path = self.env_path(view, env_name)?;
        let env_vars = self.activation_env(&env_path, None, &view.entry.path)?;
        let python = python_exe(&env_path);
        Ok(PlannedCommand::new(python.to_string_lossy().to_string())
            .arg(script.to_string())
            .args(args.to_vec())
            .cwd(view.entry.path.clone())
            .envs(env_vars))
    }

    /// Build a command that opens an interactive shell with the env activated.
    pub fn plan_shell(&self, view: &ProjectView, env_name: &str) -> Result<PlannedCommand> {
        let env_path = self.env_path(view, env_name)?;
        let env_vars = self.activation_env(&env_path, None, &view.entry.path)?;
        Ok(PlannedCommand::new(self.shell())
            .cwd(view.entry.path.clone())
            .envs(env_vars))
    }

    /// Convenience: spawn a planned command as a streamed background run.
    pub fn spawn(&self, cmd: &PlannedCommand, title: impl Into<String>) -> Result<RunHandle> {
        runner::spawn_streaming(cmd, title)
    }

    // ----- Helpers ----------------------------------------------------------

    fn shell(&self) -> String {
        self.config
            .behavior
            .shell
            .clone()
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/sh".to_string())
    }

    fn env_spec(&self, view: &ProjectView, env_name: &str) -> Result<EnvSpec> {
        if let Some(spec) = view
            .manifest
            .environments
            .iter()
            .find(|s| s.name == env_name)
        {
            return Ok(spec.clone());
        }
        // Synthesize a default spec for the conventional environment.
        if view.environments.iter().any(|e| e.name == env_name) || env_name == "default" {
            return Ok(EnvSpec {
                name: env_name.to_string(),
                path: Some(self.config.python.default_venv_name.clone()),
                python: view.manifest.python.clone(),
                requirements: None,
            });
        }
        Err(anyhow!("environment '{env_name}' is not defined"))
    }

    fn env_path(&self, view: &ProjectView, env_name: &str) -> Result<PathBuf> {
        let spec = self.env_spec(view, env_name)?;
        Ok(view.entry.path.join(spec.relative_path()))
    }

    /// Compute environment variables that "activate" a venv for a child process.
    fn activation_env(
        &self,
        env_path: &Path,
        env_file: Option<&str>,
        project_root: &Path,
    ) -> Result<Vec<(String, String)>> {
        let mut vars: Vec<(String, String)> = Vec::new();
        vars.push((
            "VIRTUAL_ENV".to_string(),
            env_path.to_string_lossy().to_string(),
        ));
        let bin = backend::bin_dir(env_path);
        let existing = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", bin.to_string_lossy(), existing);
        vars.push(("PATH".to_string(), new_path));
        // Avoid a stale interpreter shadowing the venv.
        vars.push(("PYTHONNOUSERSITE".to_string(), "1".to_string()));

        if let Some(file) = env_file {
            let path = resolve_relative(project_root, file);
            if let Ok(text) = std::fs::read_to_string(&path) {
                vars.extend(parse_dotenv(&text));
            }
        }
        Ok(vars)
    }
}

/// Probe an environment directory for status and interpreter version.
fn probe_env(env_path: &Path) -> (EnvStatus, Option<String>) {
    if !env_path.exists() {
        return (EnvStatus::Missing, None);
    }
    let python = python_exe(env_path);
    if !python.exists() {
        return (EnvStatus::Empty, None);
    }
    match pyfinder::version_of(&python) {
        Some(version) => (EnvStatus::Ready, Some(version)),
        None => (EnvStatus::Broken, None),
    }
}

/// Build a fresh registry entry for a path.
fn entry_from_path(path: &Path) -> ProjectEntry {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let name = canonical
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    ProjectEntry {
        id: registry::project_id(&canonical),
        name,
        path: canonical,
        backend: None,
        default_env: None,
        added: chrono::Local::now().to_rfc3339(),
    }
}

/// Walk up from `start` looking for a directory containing a manifest.
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(MANIFEST_FILE).exists() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

/// Resolve a possibly-relative path against a project root.
fn resolve_relative(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

/// Discover candidate Python entry-point scripts near the project root.
fn discover_scripts(root: &Path) -> Vec<String> {
    let mut scripts = Vec::new();
    for candidate in ["main.py", "app.py", "run.py", "manage.py", "__main__.py"] {
        if root.join(candidate).exists() {
            scripts.push(candidate.to_string());
        }
    }
    // Also include top-level .py files (non-recursive) to keep it fast.
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".py") && !scripts.contains(&name) {
                scripts.push(name);
            }
        }
    }
    scripts.sort();
    scripts
}

/// Discover requirements files at the project root.
fn discover_requirements(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    for candidate in [
        "requirements.txt",
        "requirements-dev.txt",
        "dev-requirements.txt",
        "requirements/base.txt",
    ] {
        if root.join(candidate).exists() {
            files.push(candidate.to_string());
        }
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("requirements") && name.ends_with(".txt") && !files.contains(&name) {
                files.push(name);
            }
        }
    }
    files
}

/// Parse a minimal dotenv file into key/value pairs.
pub fn parse_dotenv(text: &str) -> Vec<(String, String)> {
    let mut vars = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let mut value = value.trim().to_string();
            if (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
                || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
            {
                value = value[1..value.len() - 1].to_string();
            }
            if !key.is_empty() {
                vars.push((key, value));
            }
        }
    }
    vars
}
