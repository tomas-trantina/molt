//! Discovery of Python interpreters on the system.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A discovered Python interpreter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonInterpreter {
    pub path: PathBuf,
    pub version: String,
}

/// Run `<exe> --version` and return the parsed version string (e.g. `3.13.5`).
pub fn version_of(exe: &Path) -> Option<String> {
    let output = Command::new(exe).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    // Python prints to stdout on 3.4+, but older versions used stderr.
    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        text = String::from_utf8_lossy(&output.stderr).trim().to_string();
    }
    parse_version(&text)
}

/// Extract the `X.Y.Z` portion from a `Python X.Y.Z` banner.
pub fn parse_version(banner: &str) -> Option<String> {
    let token = banner.split_whitespace().find(|t| {
        t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
    })?;
    Some(token.to_string())
}

/// Discover interpreters on PATH plus any extra search directories.
///
/// Results are de-duplicated by resolved version + path and sorted with the
/// newest version first.
pub fn discover(extra_dirs: &[PathBuf]) -> Vec<PythonInterpreter> {
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut found: Vec<PythonInterpreter> = Vec::new();

    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.extend(extra_dirs.iter().cloned());

    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !is_python_binary_name(&name) {
                continue;
            }
            let path = entry.path();
            let resolved = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if !seen.insert(resolved) {
                continue;
            }
            if let Some(version) = version_of(&path) {
                found.push(PythonInterpreter { path, version });
            }
        }
    }

    found.sort_by(|a, b| version_key(&b.version).cmp(&version_key(&a.version)));
    found
}

/// Return the first interpreter matching a requested version prefix, or the
/// newest interpreter when `requested` is `None`.
pub fn best_match(requested: Option<&str>, extra_dirs: &[PathBuf]) -> Option<PythonInterpreter> {
    let all = discover(extra_dirs);
    match requested {
        Some(req) => all
            .iter()
            .find(|i| i.version.starts_with(req))
            .cloned()
            .or_else(|| all.into_iter().next()),
        None => all.into_iter().next(),
    }
}

fn is_python_binary_name(name: &str) -> bool {
    // python, python3, python3.12, python3.12m (no .exe on Linux targets).
    if !name.starts_with("python") {
        return false;
    }
    let rest = &name["python".len()..];
    rest.chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == 'm')
}

/// Turn a version string into a comparable tuple for sorting.
fn version_key(version: &str) -> (u32, u32, u32) {
    let mut parts = version.split('.').map(|p| {
        p.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or(0)
    });
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}
