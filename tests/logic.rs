//! Integration tests for the pure, I/O-free logic that Molt relies on.
//!
//! These do not spawn Python or touch the network, so they run anywhere
//! `cargo test` runs.

use std::path::Path;

use molt::backend::{self, parse_freeze, Backend};
use molt::config::Config;
use molt::domain::{BackendKind, Task, TaskKind};
use molt::pyfinder::parse_version;
use molt::service::parse_dotenv;

#[test]
fn parses_pip_freeze_output() {
    let text = "requests==2.32.3\n# a comment\n-e ./local\nrich @ file:///tmp/rich\nbare\n";
    let packages = parse_freeze(text);
    assert_eq!(packages.len(), 3);
    assert_eq!(packages[0].name, "requests");
    assert_eq!(packages[0].version, "2.32.3");
    assert_eq!(packages[1].name, "rich");
    assert_eq!(packages[2].name, "bare");
    assert_eq!(packages[2].version, "");
}

#[test]
fn parses_python_version_banner() {
    assert_eq!(parse_version("Python 3.13.5").as_deref(), Some("3.13.5"));
    assert_eq!(parse_version("Python 3.12").as_deref(), Some("3.12"));
    assert_eq!(parse_version("garbage"), None);
}

#[test]
fn backend_kind_round_trips() {
    assert_eq!(BackendKind::parse("uv"), Some(BackendKind::Uv));
    assert_eq!(BackendKind::parse("PIP"), Some(BackendKind::Venv));
    assert_eq!(BackendKind::parse("nope"), None);
    assert_eq!(BackendKind::Uv.label(), "uv");
}

#[test]
fn default_config_is_valid_and_serializable() {
    let config = Config::default();
    assert_eq!(config.python.default_venv_name, ".venv");
    assert!(config.behavior.confirm_destructive);
    let text = toml::to_string_pretty(&config).expect("serialize");
    let parsed: Config = toml::from_str(&text).expect("round-trip");
    assert_eq!(parsed.ui.theme, config.ui.theme);
}

#[test]
fn partial_config_uses_defaults() {
    // Only one field set; everything else must fall back to defaults.
    let parsed: Config = toml::from_str("[ui]\ntheme = \"ocean\"\n").expect("parse");
    assert_eq!(parsed.ui.theme, "ocean");
    assert_eq!(parsed.python.default_venv_name, ".venv");
}

#[test]
fn venv_backend_builds_requirements_command() {
    let backend = backend::VenvBackend::new("python3");
    let env = Path::new("/tmp/proj/.venv");
    let file = Path::new("/tmp/proj/requirements.txt");
    let cmd = backend.install_requirements(env, file);
    let rendered = cmd.display();
    assert!(rendered.contains("-m"));
    assert!(rendered.contains("pip"));
    assert!(rendered.contains("install"));
    assert!(rendered.contains("-r"));
    assert!(rendered.contains("requirements.txt"));
}

#[test]
fn uv_backend_targets_env_python() {
    let backend = backend::UvBackend::new();
    let env = Path::new("/tmp/proj/.venv");
    let file = Path::new("/tmp/proj/requirements.txt");
    let rendered = backend.install_requirements(env, file).display();
    assert!(rendered.starts_with("uv pip install"));
    assert!(rendered.contains("--python"));
}

#[test]
fn parses_dotenv_pairs() {
    let text = "# comment\nexport A=1\nB=\"two words\"\nC='quoted'\nBAD LINE\n";
    let vars = parse_dotenv(text);
    assert_eq!(vars.len(), 3);
    assert_eq!(vars[0], ("A".to_string(), "1".to_string()));
    assert_eq!(vars[1], ("B".to_string(), "two words".to_string()));
    assert_eq!(vars[2], ("C".to_string(), "quoted".to_string()));
}

#[test]
fn task_preview_is_readable() {
    let task = Task {
        name: "test".into(),
        kind: TaskKind::Module,
        command: "pytest".into(),
        args: vec!["-q".into()],
        cwd: None,
        env_file: None,
        description: None,
    };
    assert_eq!(task.preview(), "python -m pytest -q");
}
