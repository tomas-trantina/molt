//! Process execution.
//!
//! Three execution styles are supported:
//! * [`run_blocking`] - inherit stdio and wait (used by the CLI).
//! * [`run_capture`] - capture output for programmatic queries.
//! * [`spawn_streaming`] - run in the background and stream output for the TUI
//!   console, with the ability to stop the child.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

/// A fully specified command to execute.
#[derive(Debug, Clone)]
pub struct PlannedCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Extra environment variables to set (added to the inherited environment).
    pub env: Vec<(String, String)>,
}

impl PlannedCommand {
    pub fn new(program: impl Into<String>) -> Self {
        PlannedCommand {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn envs(mut self, vars: Vec<(String, String)>) -> Self {
        self.env.extend(vars);
        self
    }

    /// A shell-like preview of the command, for display.
    pub fn display(&self) -> String {
        let mut out = self.program.clone();
        for arg in &self.args {
            out.push(' ');
            if arg.contains(' ') {
                out.push('"');
                out.push_str(arg);
                out.push('"');
            } else {
                out.push_str(arg);
            }
        }
        out
    }

    fn build(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        if let Some(dir) = &self.cwd {
            cmd.current_dir(dir);
        }
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        cmd
    }
}

/// Result of a captured command.
#[derive(Debug, Clone)]
pub struct CapturedOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CapturedOutput {
    pub fn success(&self) -> bool {
        self.code == 0
    }
}

/// Run a command, inheriting the parent's stdio, and return its exit code.
pub fn run_blocking(cmd: &PlannedCommand) -> Result<i32> {
    let status = cmd
        .build()
        .status()
        .with_context(|| format!("failed to launch `{}`", cmd.program))?;
    Ok(status.code().unwrap_or(1))
}

/// Run a command and capture its stdout/stderr.
pub fn run_capture(cmd: &PlannedCommand) -> Result<CapturedOutput> {
    let output = cmd
        .build()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to launch `{}`", cmd.program))?;
    Ok(CapturedOutput {
        code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// The lifecycle state of a streamed run.
#[derive(Debug, Clone)]
pub enum RunState {
    Running,
    Done(i32),
    Failed(String),
}

impl RunState {
    pub fn is_running(&self) -> bool {
        matches!(self, RunState::Running)
    }
}

/// Which stream a captured line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Stdout,
    Stderr,
    Meta,
}

/// A single captured output line.
#[derive(Debug, Clone)]
pub struct OutputLine {
    pub text: String,
    pub stream: StreamKind,
}

/// A handle to a background command whose output is streamed.
#[derive(Clone)]
pub struct RunHandle {
    pub title: String,
    pub command: String,
    pub started: Instant,
    lines: Arc<Mutex<Vec<OutputLine>>>,
    state: Arc<Mutex<RunState>>,
    child: Arc<Mutex<Option<std::process::Child>>>,
}

impl RunHandle {
    /// Snapshot the current output lines.
    pub fn lines(&self) -> Vec<OutputLine> {
        self.lines.lock().map(|l| l.clone()).unwrap_or_default()
    }

    /// Number of captured lines without cloning them all.
    pub fn line_count(&self) -> usize {
        self.lines.lock().map(|l| l.len()).unwrap_or(0)
    }

    /// Snapshot the current run state.
    pub fn state(&self) -> RunState {
        self.state
            .lock()
            .map(|s| s.clone())
            .unwrap_or(RunState::Running)
    }

    /// Ask the child process to stop.
    pub fn stop(&self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(child) = guard.as_mut() {
                let _ = child.kill();
            }
        }
    }
}

/// Spawn a command in the background and stream its combined output.
pub fn spawn_streaming(cmd: &PlannedCommand, title: impl Into<String>) -> Result<RunHandle> {
    let lines: Arc<Mutex<Vec<OutputLine>>> = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(Mutex::new(RunState::Running));
    let child_slot: Arc<Mutex<Option<std::process::Child>>> = Arc::new(Mutex::new(None));

    let mut command = cmd.build();
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to launch `{}`", cmd.program))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    if let Some(stdout) = stdout {
        let lines = Arc::clone(&lines);
        thread::spawn(move || pump(stdout, lines, StreamKind::Stdout));
    }
    if let Some(stderr) = stderr {
        let lines = Arc::clone(&lines);
        thread::spawn(move || pump(stderr, lines, StreamKind::Stderr));
    }

    {
        let mut guard = child_slot.lock().expect("child mutex poisoned");
        *guard = Some(child);
    }

    // Monitor thread: poll for completion without holding the lock while idle.
    {
        let child_slot = Arc::clone(&child_slot);
        let state = Arc::clone(&state);
        thread::spawn(move || loop {
            {
                let mut guard = match child_slot.lock() {
                    Ok(g) => g,
                    Err(_) => break,
                };
                match guard.as_mut() {
                    Some(child) => match child.try_wait() {
                        Ok(Some(status)) => {
                            set_state(&state, RunState::Done(status.code().unwrap_or(1)));
                            break;
                        }
                        Ok(None) => {}
                        Err(err) => {
                            set_state(&state, RunState::Failed(err.to_string()));
                            break;
                        }
                    },
                    None => break,
                }
            }
            thread::sleep(Duration::from_millis(80));
        });
    }

    Ok(RunHandle {
        title: title.into(),
        command: cmd.display(),
        started: Instant::now(),
        lines,
        state,
        child: child_slot,
    })
}

fn pump<R: std::io::Read + Send + 'static>(
    reader: R,
    lines: Arc<Mutex<Vec<OutputLine>>>,
    stream: StreamKind,
) {
    let buffered = BufReader::new(reader);
    for line in buffered.lines() {
        let text = match line {
            Ok(text) => text,
            Err(_) => break,
        };
        if let Ok(mut guard) = lines.lock() {
            guard.push(OutputLine { text, stream });
        }
    }
}

fn set_state(slot: &Arc<Mutex<RunState>>, value: RunState) {
    if let Ok(mut guard) = slot.lock() {
        *guard = value;
    }
}
