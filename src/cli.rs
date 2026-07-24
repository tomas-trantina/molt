//! Command-line interface.
//!
//! Running `molt` with no subcommand launches the interactive TUI. All other
//! subcommands are scriptable, non-interactive counterparts of the same
//! service-layer operations, so the tool is equally usable from scripts.

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand};

use crate::config::{BackendPreference, Config};
use crate::domain::{BackendKind, EnvStatus};
use crate::error::exit;
use crate::runner;
use crate::service::{ProjectView, Service};
use crate::tui;

/// Molt - manage Python environments and run code from your terminal.
#[derive(Debug, Parser)]
#[command(name = "molt", version, about, long_about = None)]
struct Cli {
    /// Select a project by name or path (defaults to the current directory).
    #[arg(short, long, global = true)]
    project: Option<String>,

    /// Select an environment by name (defaults to the project default).
    #[arg(short, long, global = true)]
    env: Option<String>,

    /// Override the backend for this invocation.
    #[arg(long, global = true, value_name = "auto|uv|venv")]
    backend: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Launch the interactive terminal UI (default when omitted).
    Tui,
    /// List all registered projects.
    List,
    /// Register an existing project directory.
    Import(ImportArgs),
    /// Create a new project (directory + manifest).
    Create(CreateArgs),
    /// Forget a project (files are left untouched).
    Forget(ForgetArgs),
    /// Environment management.
    #[command(subcommand)]
    Env(EnvCommand),
    /// Install packages from a requirements file.
    Install(InstallArgs),
    /// Add (install) one or more packages.
    Add(PackagesArgs),
    /// Remove (uninstall) one or more packages.
    Remove(PackagesArgs),
    /// List installed packages in an environment.
    Packages,
    /// Run a saved task or a Python script.
    Run(RunArgs),
    /// Open an interactive shell with the environment activated.
    Shell,
    /// Diagnose the environment and toolchain.
    Doctor,
    /// Show or locate configuration.
    #[command(subcommand)]
    Config(ConfigCommand),
}

#[derive(Debug, Args)]
struct ImportArgs {
    /// Path to the project directory.
    path: PathBuf,
}

#[derive(Debug, Args)]
struct CreateArgs {
    /// Project name.
    name: String,
    /// Directory to create (defaults to ./<name>).
    #[arg(long)]
    path: Option<PathBuf>,
    /// Preferred Python version (e.g. 3.12).
    #[arg(long)]
    python: Option<String>,
}

#[derive(Debug, Args)]
struct ForgetArgs {
    /// Project name or path.
    project: String,
}

#[derive(Debug, Subcommand)]
enum EnvCommand {
    /// List environments in the project.
    List,
    /// Create the selected environment.
    Create,
    /// Recreate the selected environment from scratch.
    Rebuild,
    /// Delete the selected environment.
    Remove,
}

#[derive(Debug, Args)]
struct InstallArgs {
    /// Requirements file (defaults to the environment or project default).
    file: Option<String>,
}

#[derive(Debug, Args)]
struct PackagesArgs {
    /// Package specifiers (e.g. `requests` or `django>=5`).
    #[arg(required = true)]
    packages: Vec<String>,
}

#[derive(Debug, Args)]
struct RunArgs {
    /// Task name or Python script path.
    target: String,
    /// Extra arguments passed to the script (after `--`).
    #[arg(last = true)]
    args: Vec<String>,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Print the resolved configuration.
    Show,
    /// Print the configuration file path.
    Path,
    /// Write a default configuration file if none exists.
    Init,
}

/// Parse arguments and dispatch, returning a process exit code.
pub fn run() -> Result<i32> {
    let cli = Cli::parse();

    // Apply a per-invocation backend override before doing anything else.
    let backend_override = match &cli.backend {
        Some(value) => Some(
            BackendPreference::parse(value)
                .ok_or_else(|| anyhow!("invalid --backend value '{value}'"))?,
        ),
        None => None,
    };

    let command = cli.command.unwrap_or(Command::Tui);

    // Config-only commands do not need the full service.
    if let Command::Config(sub) = &command {
        return config_command(sub);
    }

    let mut service = Service::load()?;
    if let Some(pref) = backend_override {
        service.config.backend = pref;
    }

    match command {
        Command::Tui => {
            tui::run(service)?;
            Ok(exit::OK)
        }
        Command::List => cmd_list(&service),
        Command::Import(args) => cmd_import(&mut service, args),
        Command::Create(args) => {
            cmd_create(&mut service, args, backend_override.and_then(|p| p.forced()))
        }
        Command::Forget(args) => cmd_forget(&mut service, args),
        Command::Env(sub) => cmd_env(&service, sub, cli.project.as_deref(), cli.env.as_deref()),
        Command::Install(args) => {
            cmd_install(&service, args, cli.project.as_deref(), cli.env.as_deref())
        }
        Command::Add(args) => {
            cmd_packages(&service, args, true, cli.project.as_deref(), cli.env.as_deref())
        }
        Command::Remove(args) => {
            cmd_packages(&service, args, false, cli.project.as_deref(), cli.env.as_deref())
        }
        Command::Packages => cmd_packages_list(&service, cli.project.as_deref(), cli.env.as_deref()),
        Command::Run(args) => cmd_run(&service, args, cli.project.as_deref(), cli.env.as_deref()),
        Command::Shell => cmd_shell(&service, cli.project.as_deref(), cli.env.as_deref()),
        Command::Doctor => cmd_doctor(&service),
        Command::Config(_) => unreachable!("handled above"),
    }
}

fn cmd_list(service: &Service) -> Result<i32> {
    let projects = service.projects();
    if projects.is_empty() {
        println!("No projects registered yet. Use `molt import <path>` to add one.");
        return Ok(exit::OK);
    }
    for entry in projects {
        println!("{:<10} {:<24} {}", entry.id, entry.name, entry.path.display());
    }
    Ok(exit::OK)
}

fn cmd_import(service: &mut Service, args: ImportArgs) -> Result<i32> {
    let entry = service.import_project(&args.path)?;
    println!("Imported '{}' ({})", entry.name, entry.path.display());
    Ok(exit::OK)
}

fn cmd_create(
    service: &mut Service,
    args: CreateArgs,
    backend: Option<BackendKind>,
) -> Result<i32> {
    let path = args
        .path
        .unwrap_or_else(|| PathBuf::from("./").join(&args.name));
    let entry = service.create_project(&args.name, &path, args.python.as_deref(), backend)?;
    println!("Created project '{}' at {}", entry.name, entry.path.display());
    Ok(exit::OK)
}

fn cmd_forget(service: &mut Service, args: ForgetArgs) -> Result<i32> {
    let entry = service.resolve_entry(Some(&args.project))?;
    match service.remove_project(&entry.id)? {
        Some(removed) => {
            println!("Forgot project '{}'", removed.name);
            Ok(exit::OK)
        }
        None => {
            eprintln!("Project was not registered.");
            Ok(exit::NOT_FOUND)
        }
    }
}

fn resolve_view(
    service: &Service,
    project: Option<&str>,
) -> Result<ProjectView> {
    let entry = service.resolve_entry(project)?;
    service.load_view(&entry)
}

fn env_name<'a>(view: &'a ProjectView, requested: Option<&'a str>) -> String {
    if let Some(name) = requested {
        return name.to_string();
    }
    view.env(None)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| "default".to_string())
}

fn cmd_env(
    service: &Service,
    sub: EnvCommand,
    project: Option<&str>,
    env: Option<&str>,
) -> Result<i32> {
    let view = resolve_view(service, project)?;
    let name = env_name(&view, env);
    match sub {
        EnvCommand::List => {
            if view.environments.is_empty() {
                println!("No environments declared.");
            }
            for info in &view.environments {
                let version = info.python_version.as_deref().unwrap_or("-");
                println!(
                    "{:<12} {:<8} py {:<10} {}",
                    info.name,
                    info.status.label(),
                    version,
                    info.path.display()
                );
            }
            Ok(exit::OK)
        }
        EnvCommand::Create => {
            let commands = service.plan_create_env(&view, &name)?;
            run_sequence(&commands)
        }
        EnvCommand::Rebuild => {
            service.remove_env(&view, &name)?;
            let commands = service.plan_create_env(&view, &name)?;
            run_sequence(&commands)
        }
        EnvCommand::Remove => {
            service.remove_env(&view, &name)?;
            println!("Removed environment '{name}'.");
            Ok(exit::OK)
        }
    }
}

fn cmd_install(
    service: &Service,
    args: InstallArgs,
    project: Option<&str>,
    env: Option<&str>,
) -> Result<i32> {
    let view = resolve_view(service, project)?;
    let name = env_name(&view, env);
    let command = service.plan_install_requirements(&view, &name, args.file.as_deref())?;
    println!("Installing: {}", command.display());
    runner::run_blocking(&command)
}

fn cmd_packages(
    service: &Service,
    args: PackagesArgs,
    add: bool,
    project: Option<&str>,
    env: Option<&str>,
) -> Result<i32> {
    let view = resolve_view(service, project)?;
    let name = env_name(&view, env);
    let command = if add {
        service.plan_add_packages(&view, &name, &args.packages)?
    } else {
        service.plan_remove_packages(&view, &name, &args.packages)?
    };
    println!("{}", command.display());
    runner::run_blocking(&command)
}

fn cmd_packages_list(
    service: &Service,
    project: Option<&str>,
    env: Option<&str>,
) -> Result<i32> {
    let view = resolve_view(service, project)?;
    let name = env_name(&view, env);
    let packages = service.list_packages(&view, &name)?;
    if packages.is_empty() {
        println!("No packages installed.");
    }
    for package in packages {
        if package.version.is_empty() {
            println!("{}", package.name);
        } else {
            println!("{}=={}", package.name, package.version);
        }
    }
    Ok(exit::OK)
}

fn cmd_run(
    service: &Service,
    args: RunArgs,
    project: Option<&str>,
    env: Option<&str>,
) -> Result<i32> {
    let view = resolve_view(service, project)?;
    let name = env_name(&view, env);
    // Prefer a saved task with this name; otherwise treat it as a script path.
    let command = if let Some(task) = view.tasks.iter().find(|t| t.name == args.target) {
        service.plan_task(&view, task, &name)?
    } else {
        service.plan_script(&view, &args.target, &args.args, &name)?
    };
    println!("Running: {}", command.display());
    runner::run_blocking(&command)
}

fn cmd_shell(service: &Service, project: Option<&str>, env: Option<&str>) -> Result<i32> {
    let view = resolve_view(service, project)?;
    let name = env_name(&view, env);
    let command = service.plan_shell(&view, &name)?;
    println!("Entering shell with environment '{name}' activated. Type `exit` to leave.");
    runner::run_blocking(&command)
}

fn cmd_doctor(service: &Service) -> Result<i32> {
    use crate::backend::{which, UvBackend};
    use crate::pyfinder;

    println!("Molt doctor");
    println!("-----------");
    println!(
        "uv backend:      {}",
        if UvBackend::is_available() {
            "available"
        } else {
            "not found"
        }
    );
    println!(
        "pip (module):    checked per-environment"
    );
    match which("git") {
        Some(path) => println!("git:             {}", path.display()),
        None => println!("git:             not found"),
    }
    let interpreters = pyfinder::discover(&service.config.python.search_paths);
    if interpreters.is_empty() {
        println!("python:          none found");
    } else {
        println!("python interpreters:");
        for interpreter in interpreters {
            println!("  {:<10} {}", interpreter.version, interpreter.path.display());
        }
    }
    println!("projects:        {}", service.projects().len());
    Ok(exit::OK)
}

fn config_command(sub: &ConfigCommand) -> Result<i32> {
    match sub {
        ConfigCommand::Show => {
            let config = Config::load()?;
            let text = toml::to_string_pretty(&config).context("serializing configuration")?;
            println!("{text}");
            Ok(exit::OK)
        }
        ConfigCommand::Path => {
            println!("{}", Config::config_path()?.display());
            Ok(exit::OK)
        }
        ConfigCommand::Init => {
            let path = Config::config_path()?;
            if path.exists() {
                println!("Configuration already exists at {}", path.display());
            } else {
                Config::default().save()?;
                println!("Wrote default configuration to {}", path.display());
            }
            Ok(exit::OK)
        }
    }
}

/// Run a sequence of commands, stopping at the first non-zero exit code.
fn run_sequence(commands: &[runner::PlannedCommand]) -> Result<i32> {
    for command in commands {
        println!("$ {}", command.display());
        let code = runner::run_blocking(command)?;
        if code != 0 {
            return Ok(code);
        }
    }
    Ok(exit::OK)
}

/// Kept for symmetry with the TUI status labels; used by `env list`.
#[allow(dead_code)]
fn status_symbol(status: EnvStatus) -> &'static str {
    match status {
        EnvStatus::Ready => "ok",
        EnvStatus::Empty => "empty",
        EnvStatus::Missing => "missing",
        EnvStatus::Broken => "broken",
        EnvStatus::Unknown => "?",
    }
}
