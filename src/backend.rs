//! Environment and package backends.
//!
//! A [`Backend`] abstracts the concrete tool used to create environments and
//! manage packages. Two implementations are provided: [`UvBackend`] (fast,
//! preferred) and [`VenvBackend`] (the standard library `venv` + `pip`).
//!
//! Backends build [`PlannedCommand`]s so callers control *how* commands run
//! (streamed into the TUI console, captured, or inherited by the CLI).

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::config::BackendPreference;
use crate::domain::{BackendKind, Package};
use crate::runner::{self, PlannedCommand};

/// The directory containing the environment's executables.
pub fn bin_dir(env: &Path) -> PathBuf {
    if cfg!(windows) {
        env.join("Scripts")
    } else {
        env.join("bin")
    }
}

/// The Python interpreter inside an environment.
pub fn python_exe(env: &Path) -> PathBuf {
    if cfg!(windows) {
        bin_dir(env).join("python.exe")
    } else {
        bin_dir(env).join("python")
    }
}

/// Locate an executable by scanning `PATH`.
pub fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
        if cfg!(windows) {
            let exe = dir.join(format!("{program}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

/// Common behaviour of an environment/package backend.
pub trait Backend {
    /// Which backend this is.
    fn kind(&self) -> BackendKind;

    /// Build the command(s) to create an environment at `path`.
    fn create_env(&self, path: &Path, python: Option<&str>) -> Result<Vec<PlannedCommand>>;

    /// Build the command to install from a requirements file.
    fn install_requirements(&self, env: &Path, file: &Path) -> PlannedCommand;

    /// Build the command to install the given packages.
    fn install_packages(&self, env: &Path, packages: &[String]) -> PlannedCommand;

    /// Build the command to uninstall the given packages.
    fn uninstall_packages(&self, env: &Path, packages: &[String]) -> PlannedCommand;

    /// Build the command to freeze the environment to a requirements file.
    fn freeze(&self, env: &Path) -> PlannedCommand;

    /// List installed packages by running the backend synchronously.
    fn list_packages(&self, env: &Path) -> Result<Vec<Package>> {
        let cmd = self.freeze(env);
        let output = runner::run_capture(&cmd)?;
        if !output.success() {
            return Err(anyhow!(
                "failed to list packages: {}",
                output.stderr.trim()
            ));
        }
        Ok(parse_freeze(&output.stdout))
    }
}

/// The `uv` backend.
pub struct UvBackend {
    program: String,
}

impl UvBackend {
    pub fn new() -> Self {
        UvBackend {
            program: "uv".to_string(),
        }
    }

    pub fn is_available() -> bool {
        which("uv").is_some()
    }
}

impl Backend for UvBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Uv
    }

    fn create_env(&self, path: &Path, python: Option<&str>) -> Result<Vec<PlannedCommand>> {
        let mut cmd = PlannedCommand::new(&self.program)
            .arg("venv")
            .arg(path.to_string_lossy().to_string());
        if let Some(python) = python {
            cmd = cmd.arg("--python").arg(python.to_string());
        }
        Ok(vec![cmd])
    }

    fn install_requirements(&self, env: &Path, file: &Path) -> PlannedCommand {
        PlannedCommand::new(&self.program)
            .arg("pip")
            .arg("install")
            .arg("--python")
            .arg(python_exe(env).to_string_lossy().to_string())
            .arg("-r")
            .arg(file.to_string_lossy().to_string())
    }

    fn install_packages(&self, env: &Path, packages: &[String]) -> PlannedCommand {
        PlannedCommand::new(&self.program)
            .arg("pip")
            .arg("install")
            .arg("--python")
            .arg(python_exe(env).to_string_lossy().to_string())
            .args(packages.to_vec())
    }

    fn uninstall_packages(&self, env: &Path, packages: &[String]) -> PlannedCommand {
        PlannedCommand::new(&self.program)
            .arg("pip")
            .arg("uninstall")
            .arg("--python")
            .arg(python_exe(env).to_string_lossy().to_string())
            .args(packages.to_vec())
    }

    fn freeze(&self, env: &Path) -> PlannedCommand {
        PlannedCommand::new(&self.program)
            .arg("pip")
            .arg("freeze")
            .arg("--python")
            .arg(python_exe(env).to_string_lossy().to_string())
    }
}

/// The standard `venv` + `pip` backend.
pub struct VenvBackend {
    /// The base interpreter used to *create* environments.
    base_python: String,
}

impl VenvBackend {
    pub fn new(base_python: impl Into<String>) -> Self {
        VenvBackend {
            base_python: base_python.into(),
        }
    }
}

impl Backend for VenvBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Venv
    }

    fn create_env(&self, path: &Path, python: Option<&str>) -> Result<Vec<PlannedCommand>> {
        let interpreter = python.unwrap_or(&self.base_python).to_string();
        let create = PlannedCommand::new(interpreter)
            .arg("-m")
            .arg("venv")
            .arg(path.to_string_lossy().to_string());
        // Make sure pip is present and up to date inside the fresh environment.
        let upgrade = PlannedCommand::new(python_exe(path).to_string_lossy().to_string())
            .arg("-m")
            .arg("pip")
            .arg("install")
            .arg("--upgrade")
            .arg("pip");
        Ok(vec![create, upgrade])
    }

    fn install_requirements(&self, env: &Path, file: &Path) -> PlannedCommand {
        PlannedCommand::new(python_exe(env).to_string_lossy().to_string())
            .arg("-m")
            .arg("pip")
            .arg("install")
            .arg("-r")
            .arg(file.to_string_lossy().to_string())
    }

    fn install_packages(&self, env: &Path, packages: &[String]) -> PlannedCommand {
        PlannedCommand::new(python_exe(env).to_string_lossy().to_string())
            .arg("-m")
            .arg("pip")
            .arg("install")
            .args(packages.to_vec())
    }

    fn uninstall_packages(&self, env: &Path, packages: &[String]) -> PlannedCommand {
        PlannedCommand::new(python_exe(env).to_string_lossy().to_string())
            .arg("-m")
            .arg("pip")
            .arg("uninstall")
            .arg("-y")
            .args(packages.to_vec())
    }

    fn freeze(&self, env: &Path) -> PlannedCommand {
        PlannedCommand::new(python_exe(env).to_string_lossy().to_string())
            .arg("-m")
            .arg("pip")
            .arg("freeze")
    }
}

/// Choose a backend given the user preference and an optional project override.
///
/// `base_python` is the interpreter used by the `venv` backend to bootstrap
/// new environments.
pub fn resolve_backend(
    preference: BackendPreference,
    project_override: Option<BackendKind>,
    base_python: &str,
) -> Result<Box<dyn Backend>> {
    let requested = project_override.or_else(|| preference.forced());
    match requested {
        Some(BackendKind::Uv) => {
            if UvBackend::is_available() {
                Ok(Box::new(UvBackend::new()))
            } else {
                Err(anyhow!("the 'uv' backend was requested but `uv` is not on PATH"))
            }
        }
        Some(BackendKind::Venv) => Ok(Box::new(VenvBackend::new(base_python))),
        None => {
            if UvBackend::is_available() {
                Ok(Box::new(UvBackend::new()))
            } else {
                Ok(Box::new(VenvBackend::new(base_python)))
            }
        }
    }
}

/// Parse `pip freeze` / `uv pip freeze` output into packages.
pub fn parse_freeze(text: &str) -> Vec<Package> {
    let mut packages = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("-e ") {
            continue;
        }
        if let Some((name, version)) = line.split_once("==") {
            packages.push(Package {
                name: name.trim().to_string(),
                version: version.trim().to_string(),
            });
        } else if let Some((name, rest)) = line.split_once(" @ ") {
            packages.push(Package {
                name: name.trim().to_string(),
                version: rest.trim().to_string(),
            });
        } else {
            packages.push(Package {
                name: line.to_string(),
                version: String::new(),
            });
        }
    }
    packages
}
