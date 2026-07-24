//! Interactive terminal user interface.
//!
//! Layout: a left projects pane and a right detail pane with three tabs
//! (Environment / Tasks / Packages). A command palette, modal input and
//! confirmation dialogs, an output console, and a help overlay are rendered on
//! top as needed. All key bindings come from the user configuration.

use std::collections::VecDeque;
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap,
};
use ratatui::{DefaultTerminal, Frame};

use crate::domain::{EnvStatus, Package};
use crate::keymap::key_matches;
use crate::registry::ProjectEntry;
use crate::runner::{PlannedCommand, RunHandle, RunState, StreamKind};
use crate::service::{ProjectView, Service};
use crate::theme::ThemePalette;

/// Entry point: run the interactive UI until the user quits.
pub fn run(service: Service) -> Result<()> {
    let mut app = App::new(service);
    let mut terminal = ratatui::init();
    let result = app.main_loop(&mut terminal);
    ratatui::restore();
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Projects,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Environment,
    Tasks,
    Packages,
}

impl DetailTab {
    fn index(self) -> usize {
        match self {
            DetailTab::Environment => 0,
            DetailTab::Tasks => 1,
            DetailTab::Packages => 2,
        }
    }
    fn next(self) -> DetailTab {
        match self {
            DetailTab::Environment => DetailTab::Tasks,
            DetailTab::Tasks => DetailTab::Packages,
            DetailTab::Packages => DetailTab::Environment,
        }
    }
    fn prev(self) -> DetailTab {
        match self {
            DetailTab::Environment => DetailTab::Packages,
            DetailTab::Tasks => DetailTab::Environment,
            DetailTab::Packages => DetailTab::Tasks,
        }
    }
}

/// Actions offered by the command palette and bound keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionId {
    CreateEnv,
    RebuildEnv,
    RemoveEnv,
    InstallRequirements,
    AddPackage,
    RemovePackage,
    RefreshPackages,
    RunTask,
    RunScript,
    OpenShell,
    ImportProject,
    ForgetProject,
    Refresh,
}

struct PaletteAction {
    id: ActionId,
    label: &'static str,
    hint: &'static str,
}

fn all_actions() -> Vec<PaletteAction> {
    vec![
        PaletteAction { id: ActionId::InstallRequirements, label: "Install requirements", hint: "pip install -r" },
        PaletteAction { id: ActionId::RunTask, label: "Run selected task", hint: "tasks tab" },
        PaletteAction { id: ActionId::RunScript, label: "Run selected script", hint: "environment tab" },
        PaletteAction { id: ActionId::AddPackage, label: "Add package", hint: "install a package" },
        PaletteAction { id: ActionId::RemovePackage, label: "Remove selected package", hint: "packages tab" },
        PaletteAction { id: ActionId::RefreshPackages, label: "Refresh packages", hint: "reload list" },
        PaletteAction { id: ActionId::CreateEnv, label: "Create environment", hint: "venv / uv venv" },
        PaletteAction { id: ActionId::RebuildEnv, label: "Rebuild environment", hint: "recreate clean" },
        PaletteAction { id: ActionId::RemoveEnv, label: "Remove environment", hint: "delete .venv" },
        PaletteAction { id: ActionId::OpenShell, label: "Open shell", hint: "activated subshell" },
        PaletteAction { id: ActionId::ImportProject, label: "Import project", hint: "register a directory" },
        PaletteAction { id: ActionId::ForgetProject, label: "Forget project", hint: "unregister" },
        PaletteAction { id: ActionId::Refresh, label: "Reload project", hint: "re-read manifest" },
    ]
}

struct PaletteState {
    query: String,
    actions: Vec<PaletteAction>,
    filtered: Vec<usize>,
    selected: usize,
}

impl PaletteState {
    fn new() -> Self {
        let actions = all_actions();
        let filtered = (0..actions.len()).collect();
        PaletteState { query: String::new(), actions, filtered, selected: 0 }
    }
    fn refilter(&mut self) {
        let query = self.query.to_ascii_lowercase();
        self.filtered = self
            .actions
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                query.is_empty()
                    || a.label.to_ascii_lowercase().contains(&query)
                    || a.hint.to_ascii_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect();
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }
    fn current(&self) -> Option<ActionId> {
        self.filtered.get(self.selected).map(|&i| self.actions[i].id)
    }
}

#[derive(Debug, Clone, Copy)]
enum PendingInput {
    AddPackage,
    ImportProject,
}

struct InputState {
    title: String,
    prompt: String,
    buffer: String,
    action: PendingInput,
}

#[derive(Debug, Clone)]
enum PendingConfirm {
    RemoveEnv(String),
    RebuildEnv(String),
    RemovePackage(String),
    ForgetProject(String),
}

struct ConfirmState {
    message: String,
    action: PendingConfirm,
}

enum Overlay {
    None,
    Help,
    Palette(PaletteState),
    Input(InputState),
    Confirm(ConfirmState),
    Output,
}

/// A lightweight, `Copy` discriminant for [`Overlay`] used to decide dispatch
/// without holding a borrow on `self.overlay`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlayKind {
    None,
    Help,
    Palette,
    Input,
    Confirm,
    Output,
}

impl Overlay {
    fn kind(&self) -> OverlayKind {
        match self {
            Overlay::None => OverlayKind::None,
            Overlay::Help => OverlayKind::Help,
            Overlay::Palette(_) => OverlayKind::Palette,
            Overlay::Input(_) => OverlayKind::Input,
            Overlay::Confirm(_) => OverlayKind::Confirm,
            Overlay::Output => OverlayKind::Output,
        }
    }
}

struct App {
    service: Service,
    palette_theme: ThemePalette,
    tick: Duration,
    projects: Vec<ProjectEntry>,
    project_state: ListState,
    view: Option<ProjectView>,
    focus: Focus,
    tab: DetailTab,
    env_index: usize,
    task_state: ListState,
    script_state: ListState,
    package_state: ListState,
    packages: Vec<Package>,
    packages_loaded_for: Option<String>,
    overlay: Overlay,
    run: Option<RunHandle>,
    run_queue: VecDeque<PlannedCommand>,
    run_title: String,
    output_scroll: u16,
    auto_scroll: bool,
    status: String,
    want_shell: bool,
    should_quit: bool,
}

impl App {
    fn new(service: Service) -> Self {
        let theme = ThemePalette::resolve(
            &service.config.ui.theme,
            &service.config.ui.theme_overrides,
        );
        let tick = Duration::from_millis(service.config.ui.tick_rate_ms.max(16));
        let mut app = App {
            service,
            palette_theme: theme,
            tick,
            projects: Vec::new(),
            project_state: ListState::default(),
            view: None,
            focus: Focus::Projects,
            tab: DetailTab::Environment,
            env_index: 0,
            task_state: ListState::default(),
            script_state: ListState::default(),
            package_state: ListState::default(),
            packages: Vec::new(),
            packages_loaded_for: None,
            overlay: Overlay::None,
            run: None,
            run_queue: VecDeque::new(),
            run_title: String::new(),
            output_scroll: 0,
            auto_scroll: true,
            status: "Welcome to Molt. Press ? for help, a for the command palette.".to_string(),
            want_shell: false,
            should_quit: false,
        };
        app.reload_projects();
        app
    }

    // ----- Main loop --------------------------------------------------------

    fn main_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.render(frame))?;
            self.pump_runs();

            if event::poll(self.tick)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key(key)?,
                    _ => {}
                }
            }

            if self.want_shell {
                self.want_shell = false;
                self.run_shell(terminal)?;
            }
        }
        Ok(())
    }

    /// Advance the run queue when a streamed command finishes.
    fn pump_runs(&mut self) {
        let finished_ok = match &self.run {
            Some(handle) => match handle.state() {
                RunState::Done(0) => true,
                RunState::Done(_) | RunState::Failed(_) => {
                    self.run_queue.clear();
                    false
                }
                RunState::Running => false,
            },
            None => false,
        };
        if finished_ok {
            if let Some(next) = self.run_queue.pop_front() {
                match self.service.spawn(&next, self.run_title.clone()) {
                    Ok(handle) => self.run = Some(handle),
                    Err(err) => self.status = format!("Run failed: {err}"),
                }
            }
        }
    }

    fn run_shell(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let Some(view) = self.view.clone() else {
            self.status = "No project selected.".to_string();
            return Ok(());
        };
        let env = self.current_env_name();
        match self.service.plan_shell(&view, &env) {
            Ok(cmd) => {
                ratatui::restore();
                let _ = crate::runner::run_blocking(&cmd);
                *terminal = ratatui::init();
                let _ = terminal.clear();
                self.status = "Returned from shell.".to_string();
            }
            Err(err) => self.status = format!("Shell failed: {err}"),
        }
        Ok(())
    }

    // ----- Data -------------------------------------------------------------

    fn reload_projects(&mut self) {
        self.projects = self.service.projects().to_vec();
        if self.projects.is_empty() {
            self.project_state.select(None);
            self.view = None;
            return;
        }
        let selected = self
            .project_state
            .selected()
            .unwrap_or(0)
            .min(self.projects.len() - 1);
        self.project_state.select(Some(selected));
        self.select_current_project();
    }

    fn select_current_project(&mut self) {
        let Some(index) = self.project_state.selected() else {
            return;
        };
        let Some(entry) = self.projects.get(index).cloned() else {
            return;
        };
        match self.service.load_view(&entry) {
            Ok(view) => {
                self.view = Some(view);
                self.env_index = 0;
                self.packages.clear();
                self.packages_loaded_for = None;
                self.reset_detail_selection();
            }
            Err(err) => {
                self.view = None;
                self.status = format!("Failed to load project: {err}");
            }
        }
    }

    fn reset_detail_selection(&mut self) {
        if let Some(view) = &self.view {
            self.task_state
                .select((!view.tasks.is_empty()).then_some(0));
            self.script_state
                .select((!view.scripts.is_empty()).then_some(0));
        }
        self.package_state.select(None);
    }

    fn current_env_name(&self) -> String {
        if let Some(view) = &self.view {
            if let Some(info) = view.environments.get(self.env_index) {
                return info.name.clone();
            }
            if let Some(first) = view.environments.first() {
                return first.name.clone();
            }
        }
        "default".to_string()
    }

    fn refresh_packages(&mut self) {
        let Some(view) = self.view.clone() else {
            return;
        };
        let env = self.current_env_name();
        match self.service.list_packages(&view, &env) {
            Ok(packages) => {
                self.packages = packages;
                self.packages_loaded_for = Some(env);
                self.package_state
                    .select((!self.packages.is_empty()).then_some(0));
                self.status = format!("Loaded {} packages.", self.packages.len());
            }
            Err(err) => {
                self.packages.clear();
                self.packages_loaded_for = Some(env);
                self.status = format!("Could not list packages: {err}");
            }
        }
    }

    // ----- Key handling -----------------------------------------------------

    fn on_key(&mut self, key: KeyEvent) -> Result<()> {
        // Resolve which overlay is active first so the borrow of `self.overlay`
        // ends before we dispatch to a handler that needs `&mut self`.
        let kind = self.overlay.kind();
        match kind {
            OverlayKind::None => self.on_key_main(key),
            OverlayKind::Help => {
                self.overlay = Overlay::None;
                Ok(())
            }
            OverlayKind::Output => self.on_key_output(key),
            OverlayKind::Palette => self.on_key_palette(key),
            OverlayKind::Input => self.on_key_input(key),
            OverlayKind::Confirm => self.on_key_confirm(key),
        }
    }

    fn on_key_main(&mut self, key: KeyEvent) -> Result<()> {
        let keys = self.service.config.keys.clone();
        if key_matches(&key, &keys.quit) {
            self.should_quit = true;
            return Ok(());
        }
        if key_matches(&key, &keys.help) {
            self.overlay = Overlay::Help;
            return Ok(());
        }
        if key_matches(&key, &keys.palette) || key_matches(&key, &keys.search) {
            self.overlay = Overlay::Palette(PaletteState::new());
            return Ok(());
        }
        if key_matches(&key, &keys.refresh) {
            self.select_current_project();
            self.status = "Reloaded project.".to_string();
            return Ok(());
        }
        if key_matches(&key, &keys.shell) {
            self.want_shell = true;
            return Ok(());
        }
        if key_matches(&key, &keys.install) {
            self.run_action(ActionId::InstallRequirements);
            return Ok(());
        }
        if key_matches(&key, &keys.new) {
            self.begin_input(PendingInput::ImportProject);
            return Ok(());
        }
        if key_matches(&key, &keys.run) {
            self.run_default_for_tab();
            return Ok(());
        }
        if key_matches(&key, &keys.next_tab) {
            self.tab = self.tab.next();
            self.on_tab_changed();
            return Ok(());
        }
        if key_matches(&key, &keys.prev_tab) {
            self.tab = self.tab.prev();
            self.on_tab_changed();
            return Ok(());
        }
        if key_matches(&key, &keys.delete) {
            self.delete_selected();
            return Ok(());
        }

        // Navigation depends on focus.
        match self.focus {
            Focus::Projects => {
                if key_matches(&key, &keys.down) {
                    self.move_project(1);
                } else if key_matches(&key, &keys.up) {
                    self.move_project(-1);
                } else if key_matches(&key, &keys.right) || key_matches(&key, &keys.select) {
                    self.focus = Focus::Detail;
                }
            }
            Focus::Detail => {
                if key_matches(&key, &keys.left) || key_matches(&key, &keys.back) {
                    self.focus = Focus::Projects;
                } else if key_matches(&key, &keys.down) {
                    self.move_detail(1);
                } else if key_matches(&key, &keys.up) {
                    self.move_detail(-1);
                } else if key_matches(&key, &keys.right) {
                    self.tab = self.tab.next();
                    self.on_tab_changed();
                } else if key_matches(&key, &keys.select) {
                    self.activate_detail_selection();
                }
            }
        }
        Ok(())
    }

    fn on_tab_changed(&mut self) {
        if self.tab == DetailTab::Packages && self.packages_loaded_for.as_deref() != Some(self.current_env_name().as_str()) {
            self.refresh_packages();
        }
    }

    fn move_project(&mut self, delta: i32) {
        if self.projects.is_empty() {
            return;
        }
        let len = self.projects.len() as i32;
        let current = self.project_state.selected().unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(len) as usize;
        self.project_state.select(Some(next));
        self.select_current_project();
    }

    fn move_detail(&mut self, delta: i32) {
        match self.tab {
            DetailTab::Environment => {
                // Move through scripts in the environment tab.
                if let Some(view) = &self.view {
                    step(&mut self.script_state, view.scripts.len(), delta);
                }
            }
            DetailTab::Tasks => {
                if let Some(view) = &self.view {
                    step(&mut self.task_state, view.tasks.len(), delta);
                }
            }
            DetailTab::Packages => {
                step(&mut self.package_state, self.packages.len(), delta);
            }
        }
    }

    fn activate_detail_selection(&mut self) {
        match self.tab {
            DetailTab::Environment => self.run_action(ActionId::RunScript),
            DetailTab::Tasks => self.run_action(ActionId::RunTask),
            DetailTab::Packages => self.run_action(ActionId::RemovePackage),
        }
    }

    fn run_default_for_tab(&mut self) {
        match self.tab {
            DetailTab::Tasks => self.run_action(ActionId::RunTask),
            _ => self.run_action(ActionId::RunScript),
        }
    }

    fn delete_selected(&mut self) {
        match self.tab {
            DetailTab::Packages => self.run_action(ActionId::RemovePackage),
            DetailTab::Environment => self.run_action(ActionId::RemoveEnv),
            DetailTab::Tasks => {}
        }
    }

    fn on_key_output(&mut self, key: KeyEvent) -> Result<()> {
        let keys = self.service.config.keys.clone();
        if key_matches(&key, &keys.back) || key_matches(&key, &keys.quit) {
            self.overlay = Overlay::None;
        } else if key_matches(&key, &keys.up) {
            self.auto_scroll = false;
            self.output_scroll = self.output_scroll.saturating_sub(1);
        } else if key_matches(&key, &keys.down) {
            self.output_scroll = self.output_scroll.saturating_add(1);
        } else if key_matches(&key, &keys.page_up) {
            self.auto_scroll = false;
            self.output_scroll = self.output_scroll.saturating_sub(10);
        } else if key_matches(&key, &keys.page_down) {
            self.output_scroll = self.output_scroll.saturating_add(10);
        } else if key_matches(&key, &keys.select) {
            self.auto_scroll = true;
        } else if key.code == KeyCode::Char('x') {
            if let Some(handle) = &self.run {
                handle.stop();
                self.status = "Sent stop signal to the running process.".to_string();
            }
        }
        Ok(())
    }

    fn on_key_palette(&mut self, key: KeyEvent) -> Result<()> {
        let keys = self.service.config.keys.clone();
        let mut chosen: Option<ActionId> = None;
        if let Overlay::Palette(state) = &mut self.overlay {
            if key_matches(&key, &keys.back) {
                self.overlay = Overlay::None;
                return Ok(());
            } else if key_matches(&key, &keys.select) {
                chosen = state.current();
            } else if key_matches(&key, &keys.down) {
                if !state.filtered.is_empty() {
                    state.selected = (state.selected + 1) % state.filtered.len();
                }
            } else if key_matches(&key, &keys.up) {
                if !state.filtered.is_empty() {
                    state.selected =
                        (state.selected + state.filtered.len() - 1) % state.filtered.len();
                }
            } else if key.code == KeyCode::Backspace {
                state.query.pop();
                state.refilter();
            } else if let KeyCode::Char(c) = key.code {
                state.query.push(c);
                state.refilter();
            }
        }
        if let Some(action) = chosen {
            self.overlay = Overlay::None;
            self.run_action(action);
        }
        Ok(())
    }

    fn on_key_input(&mut self, key: KeyEvent) -> Result<()> {
        let keys = self.service.config.keys.clone();
        let mut submit: Option<(PendingInput, String)> = None;
        if let Overlay::Input(state) = &mut self.overlay {
            if key_matches(&key, &keys.back) {
                self.overlay = Overlay::None;
                return Ok(());
            } else if key.code == KeyCode::Enter {
                submit = Some((state.action, state.buffer.trim().to_string()));
            } else if key.code == KeyCode::Backspace {
                state.buffer.pop();
            } else if let KeyCode::Char(c) = key.code {
                state.buffer.push(c);
            }
        }
        if let Some((action, value)) = submit {
            self.overlay = Overlay::None;
            if !value.is_empty() {
                self.submit_input(action, value);
            }
        }
        Ok(())
    }

    fn on_key_confirm(&mut self, key: KeyEvent) -> Result<()> {
        let keys = self.service.config.keys.clone();
        let confirmed = key.code == KeyCode::Char('y') || key.code == KeyCode::Enter;
        let cancelled = key_matches(&key, &keys.back) || key.code == KeyCode::Char('n');
        if confirmed {
            if let Overlay::Confirm(state) = &self.overlay {
                let action = state.action.clone();
                self.overlay = Overlay::None;
                self.perform_confirmed(action);
            }
        } else if cancelled {
            self.overlay = Overlay::None;
        }
        Ok(())
    }

    // ----- Actions ----------------------------------------------------------

    fn begin_input(&mut self, action: PendingInput) {
        let (title, prompt) = match action {
            PendingInput::AddPackage => ("Add package", "Package specifier (e.g. requests):"),
            PendingInput::ImportProject => ("Import project", "Path to project directory:"),
        };
        self.overlay = Overlay::Input(InputState {
            title: title.to_string(),
            prompt: prompt.to_string(),
            buffer: String::new(),
            action,
        });
    }

    fn submit_input(&mut self, action: PendingInput, value: String) {
        match action {
            PendingInput::AddPackage => {
                let Some(view) = self.view.clone() else {
                    return;
                };
                let env = self.current_env_name();
                match self.service.plan_add_packages(&view, &env, &[value]) {
                    Ok(cmd) => self.start_sequence(vec![cmd], "Add package"),
                    Err(err) => self.status = format!("Add failed: {err}"),
                }
            }
            PendingInput::ImportProject => {
                let path = std::path::PathBuf::from(value);
                match self.service.import_project(&path) {
                    Ok(entry) => {
                        self.status = format!("Imported '{}'.", entry.name);
                        self.reload_projects();
                    }
                    Err(err) => self.status = format!("Import failed: {err}"),
                }
            }
        }
    }

    fn run_action(&mut self, id: ActionId) {
        match id {
            ActionId::InstallRequirements => self.action_install_requirements(),
            ActionId::RunTask => self.action_run_task(),
            ActionId::RunScript => self.action_run_script(),
            ActionId::AddPackage => self.begin_input(PendingInput::AddPackage),
            ActionId::RemovePackage => self.action_remove_package(),
            ActionId::RefreshPackages => self.refresh_packages(),
            ActionId::CreateEnv => self.action_create_env(false),
            ActionId::RebuildEnv => self.action_rebuild_env(),
            ActionId::RemoveEnv => self.action_remove_env(),
            ActionId::OpenShell => self.want_shell = true,
            ActionId::ImportProject => self.begin_input(PendingInput::ImportProject),
            ActionId::ForgetProject => self.action_forget_project(),
            ActionId::Refresh => {
                self.select_current_project();
                self.status = "Reloaded project.".to_string();
            }
        }
    }

    fn action_install_requirements(&mut self) {
        let Some(view) = self.view.clone() else {
            self.status = "No project selected.".to_string();
            return;
        };
        let env = self.current_env_name();
        match self.service.plan_install_requirements(&view, &env, None) {
            Ok(cmd) => self.start_sequence(vec![cmd], "Install requirements"),
            Err(err) => self.status = format!("Install failed: {err}"),
        }
    }

    fn action_run_task(&mut self) {
        let Some(view) = self.view.clone() else {
            return;
        };
        let Some(index) = self.task_state.selected() else {
            self.status = "No task selected.".to_string();
            return;
        };
        let Some(task) = view.tasks.get(index).cloned() else {
            return;
        };
        let env = self.current_env_name();
        match self.service.plan_task(&view, &task, &env) {
            Ok(cmd) => self.start_sequence(vec![cmd], format!("Task: {}", task.name)),
            Err(err) => self.status = format!("Run failed: {err}"),
        }
    }

    fn action_run_script(&mut self) {
        let Some(view) = self.view.clone() else {
            return;
        };
        let Some(index) = self.script_state.selected() else {
            self.status = "No script selected.".to_string();
            return;
        };
        let Some(script) = view.scripts.get(index).cloned() else {
            return;
        };
        let env = self.current_env_name();
        match self.service.plan_script(&view, &script, &[], &env) {
            Ok(cmd) => self.start_sequence(vec![cmd], format!("Script: {script}")),
            Err(err) => self.status = format!("Run failed: {err}"),
        }
    }

    fn action_remove_package(&mut self) {
        let Some(index) = self.package_state.selected() else {
            self.status = "No package selected.".to_string();
            return;
        };
        let Some(package) = self.packages.get(index).cloned() else {
            return;
        };
        if self.service.config.behavior.confirm_destructive {
            self.overlay = Overlay::Confirm(ConfirmState {
                message: format!("Remove package '{}'?", package.name),
                action: PendingConfirm::RemovePackage(package.name),
            });
        } else {
            self.perform_confirmed(PendingConfirm::RemovePackage(package.name));
        }
    }

    fn action_create_env(&mut self, _rebuild: bool) {
        let Some(view) = self.view.clone() else {
            return;
        };
        let env = self.current_env_name();
        match self.service.plan_create_env(&view, &env) {
            Ok(cmds) => self.start_sequence(cmds, format!("Create env: {env}")),
            Err(err) => self.status = format!("Create failed: {err}"),
        }
    }

    fn action_rebuild_env(&mut self) {
        let env = self.current_env_name();
        if self.service.config.behavior.confirm_destructive {
            self.overlay = Overlay::Confirm(ConfirmState {
                message: format!("Rebuild environment '{env}' from scratch?"),
                action: PendingConfirm::RebuildEnv(env),
            });
        } else {
            self.perform_confirmed(PendingConfirm::RebuildEnv(env));
        }
    }

    fn action_remove_env(&mut self) {
        let env = self.current_env_name();
        if self.service.config.behavior.confirm_destructive {
            self.overlay = Overlay::Confirm(ConfirmState {
                message: format!("Delete environment '{env}' from disk?"),
                action: PendingConfirm::RemoveEnv(env),
            });
        } else {
            self.perform_confirmed(PendingConfirm::RemoveEnv(env));
        }
    }

    fn action_forget_project(&mut self) {
        let Some(entry) = self
            .project_state
            .selected()
            .and_then(|i| self.projects.get(i))
            .cloned()
        else {
            return;
        };
        self.overlay = Overlay::Confirm(ConfirmState {
            message: format!("Forget project '{}'? (files are kept)", entry.name),
            action: PendingConfirm::ForgetProject(entry.id),
        });
    }

    fn perform_confirmed(&mut self, action: PendingConfirm) {
        match action {
            PendingConfirm::RemovePackage(name) => {
                let Some(view) = self.view.clone() else {
                    return;
                };
                let env = self.current_env_name();
                match self.service.plan_remove_packages(&view, &env, &[name]) {
                    Ok(cmd) => self.start_sequence(vec![cmd], "Remove package"),
                    Err(err) => self.status = format!("Remove failed: {err}"),
                }
            }
            PendingConfirm::RemoveEnv(env) => {
                if let Some(view) = self.view.clone() {
                    match self.service.remove_env(&view, &env) {
                        Ok(()) => {
                            self.status = format!("Removed environment '{env}'.");
                            self.select_current_project();
                        }
                        Err(err) => self.status = format!("Remove failed: {err}"),
                    }
                }
            }
            PendingConfirm::RebuildEnv(env) => {
                if let Some(view) = self.view.clone() {
                    if let Err(err) = self.service.remove_env(&view, &env) {
                        self.status = format!("Rebuild failed: {err}");
                        return;
                    }
                    match self.service.plan_create_env(&view, &env) {
                        Ok(cmds) => self.start_sequence(cmds, format!("Rebuild env: {env}")),
                        Err(err) => self.status = format!("Rebuild failed: {err}"),
                    }
                }
            }
            PendingConfirm::ForgetProject(id) => match self.service.remove_project(&id) {
                Ok(_) => {
                    self.status = "Project forgotten.".to_string();
                    self.reload_projects();
                }
                Err(err) => self.status = format!("Forget failed: {err}"),
            },
        }
    }

    /// Start a queued sequence of commands, showing the output console.
    fn start_sequence(&mut self, commands: Vec<PlannedCommand>, title: impl Into<String>) {
        if commands.is_empty() {
            return;
        }
        self.run_title = title.into();
        self.run_queue = commands.into_iter().collect();
        self.output_scroll = 0;
        self.auto_scroll = true;
        let first = self.run_queue.pop_front().expect("non-empty");
        match self.service.spawn(&first, self.run_title.clone()) {
            Ok(handle) => {
                self.run = Some(handle);
                self.overlay = Overlay::Output;
                self.status = format!("Running: {}", self.run_title);
            }
            Err(err) => {
                self.run_queue.clear();
                self.status = format!("Failed to start: {err}");
            }
        }
    }

    // ----- Rendering --------------------------------------------------------

    fn render(&mut self, frame: &mut Frame) {
        let theme = self.palette_theme.clone();
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(1),
                Constraint::Min(3),
                Constraint::Length(1),
            ])
            .split(area);

        self.render_header(frame, chunks[0], &theme);
        self.render_body(frame, chunks[1], &theme);
        self.render_status(frame, chunks[2], &theme);

        match self.overlay.kind() {
            OverlayKind::None => {}
            OverlayKind::Help => self.render_help(frame, area, &theme),
            OverlayKind::Palette => self.render_palette(frame, area, &theme),
            OverlayKind::Input => self.render_input(frame, area, &theme),
            OverlayKind::Confirm => self.render_confirm(frame, area, &theme),
            OverlayKind::Output => self.render_output(frame, area, &theme),
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let title = Span::styled(
            " MOLT ",
            Style::default()
                .fg(theme.selection_fg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        );
        let subtitle = Span::styled(
            "  Python environment console",
            Style::default().fg(theme.fg_dim),
        );
        let backend = Span::styled(
            format!("  backend: {}  ", self.backend_label()),
            Style::default().fg(theme.info),
        );
        let line = Line::from(vec![title, subtitle, backend]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn backend_label(&self) -> String {
        if let Some(view) = &self.view {
            self.service
                .effective_backend_kind(&view.entry, &view.manifest)
                .label()
                .to_string()
        } else {
            match self.service.config.backend {
                crate::config::BackendPreference::Auto => "auto".to_string(),
                crate::config::BackendPreference::Uv => "uv".to_string(),
                crate::config::BackendPreference::Venv => "venv".to_string(),
            }
        }
    }

    fn render_body(&mut self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(32), Constraint::Percentage(68)])
            .split(area);
        self.render_projects(frame, columns[0], theme);
        self.render_detail(frame, columns[1], theme);
    }

    fn render_projects(&mut self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let focused = self.focus == Focus::Projects;
        let items: Vec<ListItem> = self
            .projects
            .iter()
            .map(|entry| {
                ListItem::new(Line::from(vec![Span::styled(
                    entry.name.clone(),
                    Style::default().fg(theme.fg),
                )]))
            })
            .collect();
        let list = List::new(items)
            .block(block("Projects", focused, theme))
            .highlight_style(
                Style::default()
                    .bg(theme.selection_bg)
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, area, &mut self.project_state);
    }

    fn render_detail(&mut self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let focused = self.focus == Focus::Detail;
        let outer = block(
            &self
                .view
                .as_ref()
                .map(|v| format!("Project - {}", v.entry.name))
                .unwrap_or_else(|| "Project".to_string()),
            focused,
            theme,
        );
        let inner = outer.inner(area);
        frame.render_widget(outer, area);

        if self.view.is_none() {
            let hint = Paragraph::new(
                "No project selected.\n\nPress 'n' to import a project directory, or use the\ncommand palette (a) to import one.",
            )
            .style(Style::default().fg(theme.fg_dim))
            .wrap(Wrap { trim: false });
            frame.render_widget(hint, inner);
            return;
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(2), Constraint::Min(3)])
            .split(inner);
        self.render_tabs(frame, rows[0], theme);
        match self.tab {
            DetailTab::Environment => self.render_env_tab(frame, rows[1], theme),
            DetailTab::Tasks => self.render_tasks_tab(frame, rows[1], theme),
            DetailTab::Packages => self.render_packages_tab(frame, rows[1], theme),
        }
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let titles = vec!["Environment", "Tasks", "Packages"];
        let tabs = Tabs::new(titles)
            .select(self.tab.index())
            .style(Style::default().fg(theme.fg_dim))
            .highlight_style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .divider(Span::styled(" | ", Style::default().fg(theme.muted)));
        frame.render_widget(tabs, area);
    }

    fn render_env_tab(&mut self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Min(4), Constraint::Length(8)])
            .split(area);

        // Environments summary.
        let mut lines: Vec<Line> = Vec::new();
        if let Some(view) = &self.view {
            for (i, info) in view.environments.iter().enumerate() {
                let marker = if i == self.env_index { "* " } else { "  " };
                let version = info.python_version.clone().unwrap_or_else(|| "-".to_string());
                lines.push(Line::from(vec![
                    Span::styled(marker, Style::default().fg(theme.accent)),
                    Span::styled(
                        format!("{:<12}", info.name),
                        Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{:<9}", info.status.label()),
                        Style::default().fg(status_color(info.status, theme)),
                    ),
                    Span::styled(format!("py {version:<10}"), Style::default().fg(theme.info)),
                    Span::styled(
                        info.path.display().to_string(),
                        Style::default().fg(theme.muted),
                    ),
                ]));
            }
        }
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "No environments declared.",
                Style::default().fg(theme.fg_dim),
            )));
        }
        let env_panel = Paragraph::new(Text::from(lines))
            .block(block("Environments", false, theme))
            .wrap(Wrap { trim: false });
        frame.render_widget(env_panel, split[0]);

        // Runnable scripts.
        let items: Vec<ListItem> = self
            .view
            .as_ref()
            .map(|v| {
                v.scripts
                    .iter()
                    .map(|s| ListItem::new(Span::styled(s.clone(), Style::default().fg(theme.fg))))
                    .collect()
            })
            .unwrap_or_default();
        let scripts = List::new(items)
            .block(block("Scripts (enter to run)", self.focus == Focus::Detail, theme))
            .highlight_style(
                Style::default()
                    .bg(theme.selection_bg)
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");
        frame.render_stateful_widget(scripts, split[1], &mut self.script_state);
    }

    fn render_tasks_tab(&mut self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let items: Vec<ListItem> = self
            .view
            .as_ref()
            .map(|v| {
                v.tasks
                    .iter()
                    .map(|t| {
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!("{:<16}", t.name),
                                Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(t.preview(), Style::default().fg(theme.muted)),
                        ]))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let empty = items.is_empty();
        let list = List::new(items)
            .block(block("Tasks (enter to run)", self.focus == Focus::Detail, theme))
            .highlight_style(
                Style::default()
                    .bg(theme.selection_bg)
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, area, &mut self.task_state);
        if empty {
            let hint = Paragraph::new("No tasks defined in .molt.toml.")
                .style(Style::default().fg(theme.fg_dim));
            let inner = shrink(area);
            frame.render_widget(hint, inner);
        }
    }

    fn render_packages_tab(&mut self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let items: Vec<ListItem> = self
            .packages
            .iter()
            .map(|p| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:<28}", p.name),
                        Style::default().fg(theme.fg),
                    ),
                    Span::styled(p.version.clone(), Style::default().fg(theme.info)),
                ]))
            })
            .collect();
        let title = format!("Packages ({})", self.packages.len());
        let list = List::new(items)
            .block(block(&title, self.focus == Focus::Detail, theme))
            .highlight_style(
                Style::default()
                    .bg(theme.selection_bg)
                    .fg(theme.danger)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("x ");
        frame.render_stateful_widget(list, area, &mut self.package_state);
    }

    fn render_status(&self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let hints = "[a] palette  [i] install  [r] run  [s] shell  [tab] switch  [?] help  [q] quit";
        let line = Line::from(vec![
            Span::styled(format!(" {} ", self.status), Style::default().fg(theme.fg)),
            Span::styled(
                format!(" {hints}"),
                Style::default().fg(theme.muted),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(theme.panel_bg)),
            area,
        );
    }

    fn render_help(&self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let keys = &self.service.config.keys;
        let text = vec![
            Line::from(Span::styled(
                "Molt - keyboard reference",
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            help_row("Navigate", &format!("{} / {} / {} / {}", keys.up, keys.down, keys.left, keys.right), theme),
            help_row("Select / focus", &keys.select, theme),
            help_row("Command palette", &keys.palette, theme),
            help_row("Install requirements", &keys.install, theme),
            help_row("Run task/script", &keys.run, theme),
            help_row("Switch tab", &keys.next_tab, theme),
            help_row("Open shell", &keys.shell, theme),
            help_row("Import project", &keys.new, theme),
            help_row("Delete (env/package)", &keys.delete, theme),
            help_row("Reload project", &keys.refresh, theme),
            help_row("Back / cancel", &keys.back, theme),
            help_row("Quit", &keys.quit, theme),
            Line::from(""),
            Line::from(Span::styled(
                "In the output console: up/down scroll, enter re-enables auto-scroll, x stops the process.",
                Style::default().fg(theme.fg_dim),
            )),
            Line::from(Span::styled(
                "Press any key to close.",
                Style::default().fg(theme.muted),
            )),
        ];
        let area = centered(area, 64, text.len() as u16 + 2);
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(Text::from(text))
                .block(block("Help", true, theme))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_palette(&self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let Overlay::Palette(state) = &self.overlay else {
            return;
        };
        let area = centered(area, 60, 16);
        frame.render_widget(Clear, area);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(3), Constraint::Min(3)])
            .split(area);
        let query = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(theme.accent)),
            Span::styled(state.query.clone(), Style::default().fg(theme.fg)),
            Span::styled("_", Style::default().fg(theme.muted)),
        ]))
        .block(block("Command palette", true, theme));
        frame.render_widget(query, rows[0]);

        let items: Vec<ListItem> = state
            .filtered
            .iter()
            .enumerate()
            .map(|(pos, &idx)| {
                let action = &state.actions[idx];
                let selected = pos == state.selected;
                let marker = if selected { "> " } else { "  " };
                let name_style = if selected {
                    Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.fg)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(marker, Style::default().fg(theme.accent)),
                    Span::styled(format!("{:<26}", action.label), name_style),
                    Span::styled(action.hint, Style::default().fg(theme.muted)),
                ]))
            })
            .collect();
        frame.render_widget(
            List::new(items).block(block("Actions", true, theme)),
            rows[1],
        );
    }

    fn render_input(&self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let Overlay::Input(state) = &self.overlay else {
            return;
        };
        let area = centered(area, 56, 6);
        frame.render_widget(Clear, area);
        let text = vec![
            Line::from(Span::styled(
                state.prompt.clone(),
                Style::default().fg(theme.fg_dim),
            )),
            Line::from(vec![
                Span::styled("> ", Style::default().fg(theme.accent)),
                Span::styled(state.buffer.clone(), Style::default().fg(theme.fg)),
                Span::styled("_", Style::default().fg(theme.muted)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "enter to confirm, esc to cancel",
                Style::default().fg(theme.muted),
            )),
        ];
        frame.render_widget(
            Paragraph::new(Text::from(text)).block(block(&state.title, true, theme)),
            area,
        );
    }

    fn render_confirm(&self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let Overlay::Confirm(state) = &self.overlay else {
            return;
        };
        let area = centered(area, 54, 6);
        frame.render_widget(Clear, area);
        let text = vec![
            Line::from(Span::styled(
                state.message.clone(),
                Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "[y] yes    [n] no",
                Style::default().fg(theme.warning),
            )),
        ];
        frame.render_widget(
            Paragraph::new(Text::from(text))
                .block(block("Confirm", true, theme))
                .alignment(Alignment::Left),
            area,
        );
    }

    fn render_output(&mut self, frame: &mut Frame, area: Rect, theme: &ThemePalette) {
        let area = centered_pct(area, 86, 82);
        frame.render_widget(Clear, area);

        let (state_label, state_color) = match self.run.as_ref().map(|h| h.state()) {
            Some(RunState::Running) => ("running".to_string(), theme.warning),
            Some(RunState::Done(0)) => ("done".to_string(), theme.success),
            Some(RunState::Done(code)) => (format!("exit {code}"), theme.danger),
            Some(RunState::Failed(err)) => (format!("failed: {err}"), theme.danger),
            None => ("idle".to_string(), theme.muted),
        };
        let title = format!("{} [{}]", self.run_title, state_label);

        let lines: Vec<Line> = self
            .run
            .as_ref()
            .map(|handle| {
                handle
                    .lines()
                    .into_iter()
                    .map(|line| {
                        let color = match line.stream {
                            StreamKind::Stdout => theme.fg,
                            StreamKind::Stderr => theme.danger,
                            StreamKind::Meta => theme.muted,
                        };
                        Line::from(Span::styled(line.text, Style::default().fg(color)))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let total = lines.len() as u16;
        let viewport = area.height.saturating_sub(2);
        if self.auto_scroll {
            self.output_scroll = total.saturating_sub(viewport);
        } else {
            let max = total.saturating_sub(viewport);
            if self.output_scroll > max {
                self.output_scroll = max;
            }
        }

        let mut outer = block(&title, true, theme);
        outer = outer.border_style(Style::default().fg(state_color));
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(outer)
                .wrap(Wrap { trim: false })
                .scroll((self.output_scroll, 0)),
            area,
        );
    }
}

// ----- Small helpers --------------------------------------------------------

fn block(title: &str, focused: bool, theme: &ThemePalette) -> Block<'static> {
    let border = if focused {
        theme.border_focus
    } else {
        theme.border
    };
    Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(if focused { theme.accent } else { theme.fg_dim })
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(border))
}

fn status_color(status: EnvStatus, theme: &ThemePalette) -> Color {
    match status {
        EnvStatus::Ready => theme.success,
        EnvStatus::Empty => theme.warning,
        EnvStatus::Missing => theme.muted,
        EnvStatus::Broken => theme.danger,
        EnvStatus::Unknown => theme.fg_dim,
    }
}

fn help_row(label: &str, value: &str, theme: &ThemePalette) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {label:<22}"), Style::default().fg(theme.fg)),
        Span::styled(value.to_string(), Style::default().fg(theme.accent)),
    ])
}

/// Move a list selection by `delta`, wrapping within `len` items.
fn step(state: &mut ListState, len: usize, delta: i32) {
    if len == 0 {
        state.select(None);
        return;
    }
    let current = state.selected().unwrap_or(0) as i32;
    let next = (current + delta).rem_euclid(len as i32) as usize;
    state.select(Some(next));
}

/// A centered rectangle of a fixed width/height (clamped to the area).
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect { x, y, width, height }
}

/// A centered rectangle sized as a percentage of the area.
fn centered_pct(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let width = area.width * width_pct / 100;
    let height = area.height * height_pct / 100;
    centered(area, width.max(1), height.max(1))
}

/// Shrink a rect by one cell on each side (used for inline hints).
fn shrink(area: Rect) -> Rect {
    Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    }
}
