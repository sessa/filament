//! Application state, messages, and the root view/update.

use std::collections::HashMap;
use std::path::PathBuf;

use iced::widget::{button, column, container, markdown, row, rule, space, text, text_editor};
use iced::{
    border, Background, Border, Center, Color, Element, Fill, Length, Padding, Shadow,
    Subscription, Task, Theme,
};

use filament_core::{
    git, github, session, Entry, GhError, ItemId, ItemKind, NewSession, SessionStore, Workspace,
};

use crate::cli::Cli;
use crate::prefs::{PrefMsg, Prefs};
use crate::sessions::{self, SessionMsg};
use crate::theme as th;
use crate::{editor, icon, inspector, settingsview, sidebar, terminal, watcher, widgets, wizard};

/// Which top-level section is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Config,
    Sessions,
    Settings,
}

/// What the right-hand pane is doing. The editor states are boxed because they
/// are much larger than the unit `Inspect` variant.
enum Mode {
    Inspect,
    EditAgent(Box<editor::AgentEdit>),
    EditSource(Box<editor::SourceEdit>),
    Wizard(Box<wizard::Wizard>),
}

impl Mode {
    fn is_inspect(&self) -> bool {
        matches!(self, Mode::Inspect)
    }
}

pub struct App {
    workspace: Workspace,
    selection: Option<ItemId>,
    /// Parsed markdown bodies, cached per item so we don't re-parse every frame.
    previews: HashMap<ItemId, markdown::Content>,
    search: String,
    kind_filter: Option<ItemKind>,
    mode: Mode,
    /// The integrated terminal (lazily created); kept alive across hide/show.
    terminal: Option<iced_term::Terminal>,
    terminal_open: bool,
    /// A short label describing what the terminal is running / where.
    terminal_label: String,
    /// Set when the terminal failed to launch, so the panel can explain why
    /// instead of silently showing nothing.
    terminal_error: Option<String>,
    next_term_id: u64,
    /// Which top-level section is active.
    section: Section,
    /// Persisted app preferences (appearance, density, terminal, sessions).
    prefs: Prefs,
    /// The worktree-backed session manager (crow-style workflow).
    sessions: sessions::SessionsState,
}

#[derive(Debug, Clone)]
pub enum Message {
    Noop,
    Select(ItemId),
    LinkClicked(markdown::Uri),
    SearchChanged(String),
    SetKindFilter(Option<ItemKind>),
    ToggleTheme,
    Rescan,
    FsChanged,

    // sections
    SwitchSection(Section),
    Sessions(SessionMsg),
    Pref(PrefMsg),

    // terminal
    Terminal(iced_term::Event),
    ToggleTerminal,
    RunSelectedAgent,

    // editing
    EnterEditAgent,
    EnterEditSource,
    CancelEdit,
    SaveEdit,
    EditField(editor::FieldMsg),
    BodyAction(text_editor::Action),

    // creation
    NewItem,
    WizardField(wizard::WizardMsg),
    WizardCreate,
}

impl App {
    pub fn new() -> (App, Task<Message>) {
        let cli = Cli::from_env();
        let workspace = Workspace::load(cli.options());
        let prefs = Prefs::load();

        // Resolve the active repository: prefer a saved default repo, else the
        // launch workspace *only if it is actually a git repo*. (Launched from a
        // GUI, the working directory is often `/`, which is not a repo — never
        // treat that as one.)
        let repo_hint = prefs
            .default_repo
            .clone()
            .filter(|p| git::repo_root(p).is_some())
            .or_else(|| cli.workspace.clone());
        let sessions = sessions::SessionsState::load(repo_hint, prefs.show_all_sessions);

        let mut app = App {
            workspace,
            selection: None,
            previews: HashMap::new(),
            search: cli.search.clone().unwrap_or_default(),
            kind_filter: None,
            mode: Mode::Inspect,
            terminal: None,
            terminal_open: false,
            terminal_label: String::new(),
            terminal_error: None,
            next_term_id: 0,
            section: if cli.start_settings {
                Section::Settings
            } else if cli.start_sessions {
                Section::Sessions
            } else {
                Section::Config
            },
            prefs,
            sessions,
        };
        app.selection = cli
            .select
            .as_ref()
            .and_then(|name| {
                app.workspace
                    .catalog
                    .entries
                    .iter()
                    .find(|e| &e.name == name)
                    .map(|e| e.id.clone())
            })
            .or_else(|| {
                app.workspace
                    .catalog
                    .by_kind(ItemKind::Agent)
                    .find(|e| e.is_valid())
                    .map(|e| e.id.clone())
            })
            .or_else(|| {
                app.workspace
                    .catalog
                    .entries
                    .iter()
                    .find(|e| e.is_valid())
                    .map(|e| e.id.clone())
            })
            .or_else(|| app.workspace.catalog.entries.first().map(|e| e.id.clone()));
        app.ensure_preview();

        // Optional start-in-edit/wizard modes for testing and screenshots.
        if cli.start_wizard {
            app.mode = Mode::Wizard(Box::new(wizard::Wizard::new()));
        } else if cli.start_edit {
            if let Some(st) = app.selected_entry().and_then(editor::AgentEdit::new) {
                app.mode = Mode::EditAgent(Box::new(st));
            }
        }
        let startup = if cli.start_terminal {
            let cwd = app.workspace.options.workspace.clone();
            app.open_shell(cwd)
        } else {
            Task::none()
        };

        (app, startup)
    }

    pub fn title(&self) -> String {
        "Filament — Claude Code".to_string()
    }

    pub fn theme(&self) -> Theme {
        th::build(self.prefs.theme, self.prefs.accent)
    }

    /// Global UI zoom, driven by the density preference.
    pub fn scale_factor(&self) -> f32 {
        self.prefs.density.scale()
    }

    /// The root window background is fully transparent; the rounded frame in
    /// `view` paints the warm glass surface so the window has soft corners and
    /// OS blur shows through outside them.
    pub fn app_style(&self, theme: &Theme) -> iced::theme::Style {
        iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
        }
    }

    /// Watch the config locations so external edits refresh the UI live, plus
    /// the terminal backend's event stream when a terminal exists.
    pub fn subscription(&self) -> Subscription<Message> {
        let watch = watcher::subscription(self.watch_roots());
        match &self.terminal {
            Some(term) => Subscription::batch([watch, term.subscription().map(Message::Terminal)]),
            None => watch,
        }
    }

    fn term_opts(&self) -> terminal::TermOpts {
        terminal::TermOpts {
            dark: self.prefs.theme.is_dark(),
            font_size: self.prefs.terminal_font_size,
        }
    }

    /// Launch `claude` in `cwd`.
    fn open_claude(&mut self, cwd: Option<PathBuf>) -> Task<Message> {
        let cwd = usable_cwd(cwd);
        let label = label_for("claude", cwd.as_deref());
        let settings = terminal::agent_settings(cwd, self.term_opts());
        self.open_terminal(settings, label)
    }

    /// Launch a plain shell in `cwd`.
    fn open_shell(&mut self, cwd: Option<PathBuf>) -> Task<Message> {
        let cwd = usable_cwd(cwd);
        let label = label_for("shell", cwd.as_deref());
        let settings = terminal::shell_settings(cwd, &self.prefs.shell, self.term_opts());
        self.open_terminal(settings, label)
    }

    /// Create (or replace) the integrated terminal with the given settings and
    /// reveal the panel. Replacing an existing terminal ends its session. On
    /// success the new terminal is focused so the user can type immediately
    /// (which also drives its first resize/render); on failure the panel stays
    /// open to show why, instead of silently showing nothing.
    fn open_terminal(
        &mut self,
        settings: iced_term::settings::Settings,
        label: String,
    ) -> Task<Message> {
        let id = self.next_term_id;
        self.next_term_id += 1;
        match iced_term::Terminal::new(id, settings) {
            Ok(term) => {
                let widget_id = term.widget_id().clone();
                self.terminal = Some(term);
                self.terminal_open = true;
                self.terminal_label = label;
                self.terminal_error = None;
                iced_term::TerminalView::focus(widget_id)
            }
            Err(e) => {
                self.terminal = None;
                self.terminal_open = true;
                self.terminal_label = label;
                self.terminal_error = Some(format!("Couldn't start the terminal: {e}"));
                Task::none()
            }
        }
    }

    fn watch_roots(&self) -> Vec<std::path::PathBuf> {
        let mut roots = Vec::new();
        if let Some(ws) = &self.workspace.options.workspace {
            let claude = ws.join(".claude");
            if claude.is_dir() {
                roots.push(claude);
            }
            let mcp = ws.join(".mcp.json");
            if mcp.is_file() {
                roots.push(mcp);
            }
        }
        if let Some(home) = self.workspace.options.home_dir() {
            let claude = home.join(".claude");
            if claude.is_dir() {
                roots.push(claude);
            }
        }
        roots
    }

    /// Re-read the catalog, dropping a selection that no longer exists.
    fn rescan(&mut self) {
        self.workspace.rescan();
        self.previews.clear();
        if let Some(id) = &self.selection {
            if self.workspace.catalog.get(id).is_none() {
                self.selection = None;
            }
        }
        self.ensure_preview();
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Select(id) => {
                // Navigation is disabled while editing to avoid losing edits.
                if self.mode.is_inspect() {
                    self.selection = Some(id);
                    self.ensure_preview();
                }
            }
            Message::LinkClicked(_uri) => {}
            Message::SearchChanged(q) => self.search = q,
            Message::SetKindFilter(kind) => {
                self.kind_filter = if self.kind_filter == kind { None } else { kind };
            }
            Message::ToggleTheme => {
                self.prefs.theme = self.prefs.theme.next();
                self.prefs.save();
            }
            Message::Rescan | Message::FsChanged => self.rescan(),

            Message::Noop => {}
            Message::SwitchSection(section) => {
                if self.section != section {
                    self.section = section;
                    self.mode = Mode::Inspect;
                    // First time entering Sessions: kick a GitHub sync.
                    if section == Section::Sessions
                        && self.sessions.gh_present
                        && self.sessions.gh == sessions::GhStatus::Unknown
                        && self.sessions.repo_root.is_some()
                    {
                        return self.refresh_sessions();
                    }
                }
            }
            Message::Sessions(msg) => return self.update_sessions(msg),
            Message::Pref(msg) => self.update_prefs(msg),

            Message::ToggleTerminal => {
                if self.terminal_open {
                    // Visible (a live terminal or an error notice) → hide it.
                    self.terminal_open = false;
                } else if self.terminal.is_some() {
                    // Hidden but still alive → reveal it.
                    self.terminal_open = true;
                } else {
                    // Nothing yet (or a prior launch failed) → start a shell.
                    let cwd = self.active_cwd();
                    return self.open_shell(cwd);
                }
            }
            Message::RunSelectedAgent => {
                let cwd = self.active_cwd();
                return self.open_claude(cwd);
            }
            Message::Terminal(iced_term::Event::BackendCall(_, cmd)) => {
                if let Some(term) = &mut self.terminal {
                    let action = term.handle(iced_term::Command::ProxyToBackend(cmd));
                    if matches!(action, iced_term::actions::Action::Shutdown) {
                        self.terminal = None;
                        self.terminal_open = false;
                    }
                }
            }

            Message::EnterEditAgent => {
                let st = self.selected_entry().and_then(editor::AgentEdit::new);
                if let Some(st) = st {
                    self.mode = Mode::EditAgent(Box::new(st));
                }
            }
            Message::EnterEditSource => {
                let info = self
                    .selected_entry()
                    .map(|e| (e.id.clone(), e.source_path.clone()));
                if let Some((id, path)) = info {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        self.mode =
                            Mode::EditSource(Box::new(editor::SourceEdit::new(id, path, &text)));
                    }
                }
            }
            Message::CancelEdit => self.mode = Mode::Inspect,
            Message::EditField(msg) => {
                if let Mode::EditAgent(st) = &mut self.mode {
                    st.apply(msg);
                }
            }
            Message::BodyAction(action) => match &mut self.mode {
                Mode::EditAgent(st) => st.body_action(action),
                Mode::EditSource(st) => st.body_action(action),
                _ => {}
            },
            Message::SaveEdit => self.save_edit(),

            Message::NewItem => self.mode = Mode::Wizard(Box::new(wizard::Wizard::new())),
            Message::WizardField(msg) => {
                if let Mode::Wizard(w) = &mut self.mode {
                    w.apply(msg);
                }
            }
            Message::WizardCreate => self.create_from_wizard(),
        }
        Task::none()
    }

    /// The working directory the terminal / Run actions should default to: the
    /// active repo when one is selected, else the launch workspace.
    fn active_cwd(&self) -> Option<PathBuf> {
        self.sessions
            .repo_root
            .clone()
            .or_else(|| self.workspace.options.workspace.clone())
    }

    fn update_prefs(&mut self, msg: PrefMsg) {
        match msg {
            PrefMsg::SetTheme(m) => self.prefs.theme = m,
            PrefMsg::SetAccent(a) => self.prefs.accent = a,
            PrefMsg::SetDensity(d) => self.prefs.density = d,
            PrefMsg::TermFontDelta(delta) => self.prefs.bump_terminal_font(delta),
            PrefMsg::ShellChanged(s) => self.prefs.shell = s,
            PrefMsg::ToggleShowAll(v) => {
                self.prefs.show_all_sessions = v;
                self.sessions.show_all = v;
            }
        }
        self.prefs.save();
    }

    fn save_edit(&mut self) {
        enum Outcome {
            Saved(ItemId),
            Failed(String),
        }
        let outcome = match &self.mode {
            Mode::EditAgent(st) => Some(if !st.is_valid() {
                Outcome::Failed("Fix the validation errors before saving.".into())
            } else {
                match filament_core::edit::atomic_write(&st.path, &st.build_text()) {
                    Ok(()) => Outcome::Saved(st.id.clone()),
                    Err(e) => Outcome::Failed(format!("Save failed: {e}")),
                }
            }),
            Mode::EditSource(st) => Some(
                match filament_core::edit::atomic_write(&st.path, &st.text()) {
                    Ok(()) => Outcome::Saved(st.id.clone()),
                    Err(e) => Outcome::Failed(format!("Save failed: {e}")),
                },
            ),
            _ => None,
        };

        match outcome {
            Some(Outcome::Saved(id)) => {
                self.workspace.rescan();
                self.previews.clear();
                self.selection = Some(id);
                self.mode = Mode::Inspect;
                self.ensure_preview();
            }
            Some(Outcome::Failed(msg)) => match &mut self.mode {
                Mode::EditAgent(st) => st.status = Some(msg),
                Mode::EditSource(st) => st.status = Some(msg),
                _ => {}
            },
            None => {}
        }
    }

    fn create_from_wizard(&mut self) {
        let workspace = self.workspace.options.workspace.clone();
        let home = self.workspace.options.home_dir();
        let result = match &self.mode {
            Mode::Wizard(w) => Some(w.create(workspace.as_deref(), home.as_deref())),
            _ => None,
        };
        match result {
            Some(Ok((kind, path))) => {
                self.workspace.rescan();
                self.previews.clear();
                self.selection = Some(filament_core::ItemId::for_path(kind, &path));
                self.mode = Mode::Inspect;
                self.ensure_preview();
            }
            Some(Err(e)) => {
                if let Mode::Wizard(w) = &mut self.mode {
                    w.error = Some(e);
                }
            }
            None => {}
        }
    }

    // ---- sessions ----------------------------------------------------------

    fn update_sessions(&mut self, msg: SessionMsg) -> Task<Message> {
        match msg {
            SessionMsg::Select(id) => {
                self.sessions.selected = Some(id);
                self.sessions.compose = None;
                self.sessions.error = None;
            }
            SessionMsg::ToggleNew => {
                if self.sessions.compose.is_some() {
                    self.sessions.compose = None;
                } else {
                    let base = self
                        .sessions
                        .default_branch
                        .clone()
                        .or_else(|| self.sessions.branches.first().cloned())
                        .unwrap_or_default();
                    self.sessions.compose = Some(sessions::NewForm {
                        base,
                        ..Default::default()
                    });
                }
                self.sessions.error = None;
            }
            SessionMsg::FormTitle(v) => {
                if let Some(f) = &mut self.sessions.compose {
                    f.title = v;
                }
            }
            SessionMsg::FormIssue(v) => {
                if let Some(f) = &mut self.sessions.compose {
                    f.issue_url = v;
                }
            }
            SessionMsg::FormBase(v) => {
                if let Some(f) = &mut self.sessions.compose {
                    f.base = v;
                }
            }
            SessionMsg::Create => return self.create_session(),
            SessionMsg::StartIssue(number) => return self.start_issue(number),
            SessionMsg::Created(result) => {
                self.sessions.busy = None;
                match result {
                    Ok(id) => {
                        self.sessions.reload();
                        self.sessions.selected = Some(id);
                        self.sessions.compose = None;
                        self.sessions.error = None;
                    }
                    Err(e) => self.sessions.error = Some(e),
                }
            }
            SessionMsg::OpenAgent(id) => {
                if let Some(cwd) = self.sessions.store.get(&id).map(|s| s.worktree.clone()) {
                    self.sessions.selected = Some(id);
                    return self.open_claude(Some(cwd));
                }
            }
            SessionMsg::OpenShell(id) => {
                if let Some(cwd) = self.sessions.store.get(&id).map(|s| s.worktree.clone()) {
                    self.sessions.selected = Some(id);
                    return self.open_shell(Some(cwd));
                }
            }
            SessionMsg::Delete(id) => return self.delete_session(id),
            SessionMsg::Deleted(result) => {
                self.sessions.busy = None;
                match result {
                    Ok(()) => self.sessions.reload(),
                    Err(e) => {
                        self.sessions.error = Some(e);
                        self.sessions.reload();
                    }
                }
            }
            SessionMsg::Refresh => return self.refresh_sessions(),
            SessionMsg::Refreshed(status, issues) => {
                self.sessions.gh = status;
                self.sessions.issues = issues;
                self.sessions.reload();
                self.sessions.busy = None;
            }
            SessionMsg::AdoptOrphans => return self.adopt_orphans(),
            SessionMsg::Adopted => {
                self.sessions.reload();
                self.sessions.busy = None;
            }
            SessionMsg::OpenUrl(url) => open_url(&url),
            SessionMsg::RepoInputChanged(v) => self.sessions.repo_input = v,
            SessionMsg::OpenRepo => {
                let path = self.sessions.repo_input.trim().to_string();
                if !path.is_empty() {
                    self.set_active_repo(PathBuf::from(path));
                }
            }
            SessionMsg::BrowseRepo => {
                // The native folder picker is modal; it runs on this (main) thread
                // and blocks until the user chooses or cancels.
                let start = self
                    .sessions
                    .repo_root
                    .clone()
                    .or_else(|| self.active_cwd())
                    .filter(|p| p.is_dir());
                let mut dialog = rfd::FileDialog::new().set_title("Open a git repository");
                if let Some(dir) = start {
                    dialog = dialog.set_directory(dir);
                }
                if let Some(path) = dialog.pick_folder() {
                    self.sessions.repo_input = path.display().to_string();
                    self.set_active_repo(path);
                }
            }
            SessionMsg::SetRepo(path) => self.set_active_repo(path),
        }
        Task::none()
    }

    /// Point the Sessions board at `path` (resolved to its git root), persist it
    /// as the default repo, and recompute repo-derived state.
    fn set_active_repo(&mut self, path: PathBuf) {
        match git::repo_root(&path) {
            Some(root) => {
                self.sessions.set_repo(Some(root.clone()));
                self.prefs.default_repo = Some(root);
                self.prefs.save();
                self.sessions.error = None;
            }
            None => {
                self.sessions.error = Some(format!("Not a git repository: {}", path.display()));
            }
        }
    }

    fn create_session(&mut self) -> Task<Message> {
        let (Some(repo), Some(form)) = (
            self.sessions.repo_root.clone(),
            self.sessions.compose.clone(),
        ) else {
            return Task::none();
        };
        let store_path = self.sessions.store.path.clone();
        let gh_present = self.sessions.gh_present;
        let now = now_unix();
        self.sessions.busy = Some("Creating session…".into());
        self.sessions.error = None;
        run_async(move || {
            let mut store = SessionStore::load_at(store_path);
            let issue = (gh_present && !form.issue_url.trim().is_empty())
                .then(|| github::view_issue(&repo, form.issue_url.trim()).ok())
                .flatten();
            let req = NewSession {
                title: form.title.clone(),
                base_branch: form.base.clone(),
                issue,
            };
            let result = match session::create_session(&mut store, &repo, req, now) {
                Ok(s) => {
                    let _ = store.save();
                    Ok(s.id)
                }
                Err(e) => Err(e.to_string()),
            };
            Message::Sessions(SessionMsg::Created(result))
        })
    }

    fn start_issue(&mut self, number: u64) -> Task<Message> {
        let Some(repo) = self.sessions.repo_root.clone() else {
            return Task::none();
        };
        let Some(issue) = self
            .sessions
            .issues
            .iter()
            .find(|i| i.number == number)
            .cloned()
        else {
            return Task::none();
        };
        let store_path = self.sessions.store.path.clone();
        let base = self
            .sessions
            .default_branch
            .clone()
            .or_else(|| self.sessions.branches.first().cloned())
            .unwrap_or_else(|| "main".into());
        let now = now_unix();
        self.sessions.busy = Some(format!("Creating session for #{number}…"));
        self.sessions.error = None;
        run_async(move || {
            let mut store = SessionStore::load_at(store_path);
            let req = NewSession {
                title: issue.title.clone(),
                base_branch: base,
                issue: Some(issue),
            };
            let result = match session::create_session(&mut store, &repo, req, now) {
                Ok(s) => {
                    let _ = store.save();
                    Ok(s.id)
                }
                Err(e) => Err(e.to_string()),
            };
            Message::Sessions(SessionMsg::Created(result))
        })
    }

    fn delete_session(&mut self, id: String) -> Task<Message> {
        let store_path = self.sessions.store.path.clone();
        if self.sessions.selected.as_deref() == Some(&id) {
            self.sessions.selected = None;
        }
        self.sessions.busy = Some("Removing session…".into());
        self.sessions.error = None;
        run_async(move || {
            let mut store = SessionStore::load_at(store_path);
            let result = session::remove_session(&mut store, &id, true).map_err(|e| e.to_string());
            if result.is_ok() {
                let _ = store.save();
            }
            Message::Sessions(SessionMsg::Deleted(result))
        })
    }

    fn refresh_sessions(&mut self) -> Task<Message> {
        let Some(repo) = self.sessions.repo_root.clone() else {
            return Task::none();
        };
        let store_path = self.sessions.store.path.clone();
        let gh_present = self.sessions.gh_present;
        self.sessions.busy = Some("Syncing with GitHub…".into());
        run_async(move || {
            if !gh_present {
                return Message::Sessions(SessionMsg::Refreshed(
                    sessions::GhStatus::NotInstalled,
                    Vec::new(),
                ));
            }
            let mut store = SessionStore::load_at(store_path);
            let issues_res = github::list_open_issues(&repo, 50);
            let status = match &issues_res {
                Ok(_) => sessions::GhStatus::Ready,
                Err(GhError::NotInstalled) => sessions::GhStatus::NotInstalled,
                Err(GhError::NotAuthenticated) => sessions::GhStatus::NotAuthenticated,
                Err(e) => sessions::GhStatus::Error(e.to_string()),
            };
            let issues = issues_res.unwrap_or_default();

            if status == sessions::GhStatus::Ready {
                let ids: Vec<String> = store.for_repo(&repo).map(|s| s.id.clone()).collect();
                for id in ids {
                    let Some((branch, issue_key)) = store.get(&id).map(|s| {
                        (
                            s.branch.clone(),
                            s.issue.as_ref().map(|i| i.number.to_string()),
                        )
                    }) else {
                        continue;
                    };
                    let pr = github::pr_for_branch(&repo, &branch).ok().flatten();
                    let issue = issue_key.and_then(|k| github::view_issue(&repo, &k).ok());
                    if let Some(s) = store.get_mut(&id) {
                        if pr.is_some() {
                            s.pr = pr;
                        }
                        if issue.is_some() {
                            s.issue = issue;
                        }
                        s.state = s.derive_state();
                    }
                }
                let _ = store.save();
            }
            Message::Sessions(SessionMsg::Refreshed(status, issues))
        })
    }

    fn adopt_orphans(&mut self) -> Task<Message> {
        let Some(repo) = self.sessions.repo_root.clone() else {
            return Task::none();
        };
        let store_path = self.sessions.store.path.clone();
        let now = now_unix();
        self.sessions.busy = Some("Adopting worktrees…".into());
        run_async(move || {
            let mut store = SessionStore::load_at(store_path);
            for w in session::detect_orphans(&store, &repo) {
                session::adopt_orphan(&mut store, &repo, &w, now);
            }
            let _ = store.save();
            Message::Sessions(SessionMsg::Adopted)
        })
    }

    pub fn view(&self) -> Element<'_, Message> {
        let theme = self.theme();

        let detail: Element<Message> = match self.section {
            Section::Config => match &self.mode {
                Mode::Inspect => self.detail(),
                Mode::EditAgent(st) => {
                    widgets::scroll(container(st.view(&theme)).padding(th::PAD_PANE), &theme).into()
                }
                Mode::EditSource(st) => container(st.view(&theme))
                    .padding(th::PAD_PANE)
                    .height(Fill)
                    .into(),
                Mode::Wizard(w) => {
                    widgets::scroll(container(w.view(&theme)).padding(th::PAD_PANE), &theme).into()
                }
            },
            Section::Sessions => self.sessions.detail(&theme),
            Section::Settings => {
                settingsview::view(&self.prefs, &theme, self.sessions.repo_root.as_deref())
            }
        };

        // Right pane: the detail/editor in a glass panel, terminal docked below.
        let detail_panel = container(detail)
            .width(Fill)
            .height(Length::FillPortion(3))
            .clip(true)
            .style(widgets::panel(&theme));

        let docked: Option<Element<Message>> = if self.terminal_open {
            if let Some(term) = &self.terminal {
                Some(self.terminal_panel(term, &theme))
            } else {
                self.terminal_error
                    .as_deref()
                    .map(|err| self.terminal_error_panel(err, &theme))
            }
        } else {
            None
        };

        let right_pane: Element<Message> = match docked {
            Some(panel) => column![
                detail_panel,
                container(panel).height(Length::FillPortion(2)),
            ]
            .spacing(th::GAP_PANEL)
            .into(),
            None => detail_panel.into(),
        };

        let (sidebar_inner, sidebar_width) = match self.section {
            Section::Config => (
                sidebar::view(
                    &self.workspace.catalog,
                    self.selection.as_ref(),
                    &theme,
                    &self.search,
                    self.kind_filter,
                ),
                288.0,
            ),
            Section::Sessions => (self.sessions.sidebar(&theme), 332.0),
            Section::Settings => (settingsview::sidebar(&theme), 288.0),
        };
        let sidebar_panel = container(sidebar_inner)
            .width(Length::Fixed(sidebar_width))
            .height(Fill)
            .clip(true)
            .style(widgets::panel(&theme));

        let body = row![sidebar_panel, right_pane]
            .spacing(th::GAP_PANEL)
            .height(Fill);

        // On macOS the native title bar is transparent and our content runs
        // full-height behind it, with the traffic-light buttons floating at the
        // top-left. macOS pins those to the title-bar region, so we reserve a
        // strip of that height at the very top (the buttons live there, over the
        // glass) and drop the toolbar + body just beneath it. The toolbar is the
        // same rounded panel as everywhere else — no fighting the buttons for a
        // shared row. Elsewhere the native title bar is above us, so we go
        // straight to the toolbar.
        let content = if cfg!(target_os = "macos") {
            let titlebar =
                container(space())
                    .width(Fill)
                    .height(Length::Fixed(macos_titlebar_height(
                        self.prefs.density.scale(),
                    )));
            column![
                titlebar,
                column![self.header(&theme), body]
                    .spacing(th::GAP_PANEL)
                    .padding(Padding {
                        top: 0.0,
                        right: th::GAP_PANEL,
                        bottom: th::GAP_PANEL,
                        left: th::GAP_PANEL,
                    }),
            ]
            .height(Fill)
        } else {
            column![self.header(&theme), body]
                .spacing(th::GAP_PANEL)
                .padding(th::GAP_PANEL)
                .height(Fill)
        };

        // Rounded glass frame: the visible "window", with soft corners over the
        // OS blur outside the radius.
        let frame_bg = th::app_background(&theme);
        let frame_border = th::hairline(&theme);
        container(content)
            .width(Fill)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(frame_bg)),
                border: Border {
                    color: frame_border,
                    width: 1.0,
                    radius: 16.0.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    fn terminal_panel<'a>(
        &'a self,
        term: &'a iced_term::Terminal,
        theme: &Theme,
    ) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let accent = theme.palette().primary;
        let bg = th::surface_strong(theme);
        let bdr = th::hairline(theme);
        let shadow = th::panel_shadow();
        let label = if self.terminal_label.is_empty() {
            "Terminal".to_string()
        } else {
            self.terminal_label.clone()
        };

        let bar = container(
            row![
                icon::icon(icon::TERMINAL)
                    .size(13)
                    .style(move |_: &Theme| text::Style {
                        color: Some(accent)
                    }),
                text(label)
                    .size(th::TEXT_META)
                    .style(move |_| text::Style { color: Some(muted) }),
                space().width(Fill),
                widgets::icon_only(icon::CLOSE, Message::ToggleTerminal, theme),
            ]
            .align_y(Center)
            .spacing(8),
        )
        .padding(Padding {
            top: 5.0,
            right: 6.0,
            bottom: 5.0,
            left: 12.0,
        })
        .width(Fill);

        let view = container(iced_term::TerminalView::show(term).map(Message::Terminal))
            .padding(8)
            .width(Fill)
            .height(Fill);

        container(column![bar, rule::horizontal(1), view].height(Fill))
            .clip(true)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    color: bdr,
                    width: 1.0,
                    radius: th::RADIUS_PANEL.into(),
                },
                shadow,
                ..container::Style::default()
            })
            .into()
    }

    /// Shown in place of the terminal when a launch fails, so the failure is
    /// visible (and dismissable) rather than an empty panel.
    fn terminal_error_panel<'a>(&'a self, err: &'a str, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let danger = theme.palette().danger;
        let bg = th::surface_strong(theme);
        let bdr = th::hairline(theme);
        let shadow = th::panel_shadow();

        let bar = container(
            row![
                icon::icon(icon::WARNING)
                    .size(13)
                    .style(move |_: &Theme| text::Style {
                        color: Some(danger)
                    }),
                text("Terminal")
                    .size(th::TEXT_META)
                    .style(move |_| text::Style { color: Some(muted) }),
                space().width(Fill),
                widgets::icon_only(icon::CLOSE, Message::ToggleTerminal, theme),
            ]
            .align_y(Center)
            .spacing(8),
        )
        .padding(Padding {
            top: 5.0,
            right: 6.0,
            bottom: 5.0,
            left: 12.0,
        })
        .width(Fill);

        let body = container(
            text(err)
                .size(th::TEXT_BODY)
                .style(move |_| text::Style { color: Some(muted) }),
        )
        .padding(16)
        .width(Fill)
        .height(Fill);

        container(column![bar, rule::horizontal(1), body].height(Fill))
            .clip(true)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    color: bdr,
                    width: 1.0,
                    radius: th::RADIUS_PANEL.into(),
                },
                shadow,
                ..container::Style::default()
            })
            .into()
    }

    /// The segmented Config / Sessions / Settings switcher shown in the header.
    fn section_toggle<'a>(&self, theme: &Theme) -> Element<'a, Message> {
        let seg = |glyph: char, label: &'static str, section: Section, active: bool| {
            let primary = theme.palette().primary;
            let txt = theme.palette().text;
            button(
                row![icon::icon(glyph).size(13), text(label).size(th::TEXT_UI)]
                    .spacing(6)
                    .align_y(Center),
            )
            .padding(Padding {
                top: 5.0,
                right: 11.0,
                bottom: 5.0,
                left: 11.0,
            })
            .on_press(Message::SwitchSection(section))
            .style(move |_t, status| {
                let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
                let (bg, fg) = if active {
                    (th::with_alpha(primary, 0.22), txt)
                } else if hovered {
                    (th::with_alpha(txt, 0.10), txt)
                } else {
                    (Color::TRANSPARENT, th::with_alpha(txt, 0.7))
                };
                button::Style {
                    background: Some(Background::Color(bg)),
                    text_color: fg,
                    border: border::rounded(8),
                    shadow: Shadow::default(),
                    snap: true,
                }
            })
        };
        let surface = th::surface_strong(theme);
        container(
            row![
                seg(
                    icon::CONFIG,
                    "Config",
                    Section::Config,
                    self.section == Section::Config
                ),
                seg(
                    icon::SESSIONS,
                    "Sessions",
                    Section::Sessions,
                    self.section == Section::Sessions
                ),
                seg(
                    icon::SETTINGS,
                    "Settings",
                    Section::Settings,
                    self.section == Section::Settings
                ),
            ]
            .spacing(2),
        )
        .padding(2)
        .style(move |_| container::Style {
            background: Some(Background::Color(surface)),
            border: border::rounded(10),
            ..container::Style::default()
        })
        .into()
    }

    fn header<'a>(&'a self, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let accent = theme.palette().primary;
        let workspace = self
            .workspace
            .options
            .workspace
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();

        // Cycles Dark → Light → Ayu; the glyph shows the active theme.
        let theme_glyph = match self.prefs.theme {
            crate::prefs::ThemeMode::Dark => icon::MOON,
            crate::prefs::ThemeMode::Light => icon::SUN,
            crate::prefs::ThemeMode::Ayu => icon::PALETTE,
        };
        let theme_toggle = widgets::icon_only(theme_glyph, Message::ToggleTheme, theme);
        let terminal_button = widgets::icon_button(
            icon::TERMINAL,
            if self.terminal_open {
                "Hide Terminal"
            } else {
                "Terminal"
            },
            Message::ToggleTerminal,
            theme,
        );

        let mut right = row![].spacing(8).align_y(Center);
        match self.section {
            Section::Sessions => {
                right = right.push(widgets::icon_button(
                    icon::REFRESH,
                    "Refresh",
                    Message::Sessions(SessionMsg::Refresh),
                    theme,
                ));
                right = right.push(terminal_button);
                right = right.push(theme_toggle);
            }
            Section::Settings => {
                right = right.push(terminal_button);
                right = right.push(theme_toggle);
            }
            Section::Config => match &self.mode {
                Mode::Inspect => {
                    let invalid = self.workspace.catalog.invalid_count();
                    if invalid > 0 {
                        right = right.push(widgets::pill(
                            format!("{invalid} invalid"),
                            th::danger(),
                            th::with_alpha(th::danger(), 0.15),
                        ));
                    }
                    right = right.push(widgets::icon_button(
                        icon::NEW,
                        "New",
                        Message::NewItem,
                        theme,
                    ));
                    right = right.push(terminal_button);
                    right = right.push(widgets::icon_only(icon::REFRESH, Message::Rescan, theme));
                    right = right.push(theme_toggle);
                }
                Mode::EditAgent(st) => {
                    let on_save = st.is_valid().then_some(Message::SaveEdit);
                    right = right.push(widgets::primary_button(icon::SAVE, "Save", on_save, theme));
                    right = right.push(widgets::icon_button(
                        icon::CLOSE,
                        "Cancel",
                        Message::CancelEdit,
                        theme,
                    ));
                }
                Mode::EditSource(_) => {
                    right = right.push(widgets::primary_button(
                        icon::SAVE,
                        "Save",
                        Some(Message::SaveEdit),
                        theme,
                    ));
                    right = right.push(widgets::icon_button(
                        icon::CLOSE,
                        "Cancel",
                        Message::CancelEdit,
                        theme,
                    ));
                }
                Mode::Wizard(_) => {
                    right = right.push(widgets::primary_button(
                        icon::NEW,
                        "Create",
                        Some(Message::WizardCreate),
                        theme,
                    ));
                    right = right.push(widgets::icon_button(
                        icon::CLOSE,
                        "Cancel",
                        Message::CancelEdit,
                        theme,
                    ));
                }
            },
        }

        let bar = row![
            icon::icon(icon::AGENT)
                .size(17)
                .style(move |_: &Theme| text::Style {
                    color: Some(accent)
                }),
            text("Filament").size(th::TEXT_TITLE),
            space().width(12.0),
            self.section_toggle(theme),
            space().width(10.0),
            text(workspace)
                .size(th::TEXT_META)
                .style(move |_| text::Style { color: Some(muted) }),
            space().width(Fill),
            right,
        ]
        .align_y(Center)
        .spacing(4);

        container(bar)
            .padding(Padding {
                top: 9.0,
                right: 12.0,
                bottom: 9.0,
                left: 14.0,
            })
            .style(widgets::panel(theme))
            .into()
    }

    fn detail(&self) -> Element<'_, Message> {
        match self.selected_entry() {
            Some(entry) => inspector::view(entry, self.previews.get(&entry.id), &self.theme()),
            None => self.placeholder(),
        }
    }

    fn placeholder(&self) -> Element<'_, Message> {
        let muted = th::muted(&self.theme());
        container(
            text("Select an item to inspect")
                .size(th::TEXT_BODY)
                .style(move |_| text::Style { color: Some(muted) }),
        )
        .center_x(Fill)
        .center_y(Fill)
        .into()
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.selection
            .as_ref()
            .and_then(|id| self.workspace.catalog.get(id))
    }

    fn ensure_preview(&mut self) {
        let Some(id) = self.selection.clone() else {
            return;
        };
        if self.previews.contains_key(&id) {
            return;
        }
        if let Some(entry) = self.workspace.catalog.get(&id) {
            if let Some(body) = entry.body() {
                let content = markdown::Content::parse(body);
                self.previews.insert(id, content);
            }
        }
    }
}

/// Pick a working directory the terminal can actually start in.
///
/// A stale or missing path (e.g. a deleted worktree, or `/` from a GUI launch)
/// makes the child shell fail to start — which looks like "nothing shows up in
/// the shell". Fall back to the user's home directory, then to the process
/// default (`None`), so a terminal always opens somewhere valid.
fn usable_cwd(cwd: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(dir) = cwd.filter(|p| p.is_dir()) {
        return Some(dir);
    }
    directories::UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .filter(|p| p.is_dir())
}

/// Height to reserve at the very top of the window for the macOS title-bar
/// region, where the traffic-light buttons float over the glass.
///
/// With a transparent title bar + full-size content view (see `window_settings`
/// in `main`), our content runs to the top and the buttons sit in the standard
/// title-bar band. We keep that band clear so the toolbar drops cleanly beneath
/// it. The band is fixed in physical window points regardless of our UI scale,
/// so convert it into pre-scale logical points.
fn macos_titlebar_height(scale: f32) -> f32 {
    /// Standard macOS title-bar height in window points, plus a little room.
    const TITLEBAR_PT: f32 = 30.0;
    TITLEBAR_PT / scale
}

/// A short label for the terminal header: `program · dirname`.
fn label_for(program: &str, cwd: Option<&std::path::Path>) -> String {
    match cwd.and_then(|p| p.file_name()).map(|s| s.to_string_lossy()) {
        Some(name) => format!("{program} · {name}"),
        None => program.to_string(),
    }
}

/// Current Unix time in seconds (0 if the clock is before the epoch).
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Run blocking work (git / gh) off the UI thread and deliver its `Message`.
///
/// The closure produces the result `Message` directly; it runs on a dedicated
/// OS thread and is handed back through a oneshot channel so the Iced executor
/// never blocks on git or network I/O.
fn run_async(work: impl FnOnce() -> Message + Send + 'static) -> Task<Message> {
    let (tx, rx) = iced::futures::channel::oneshot::channel::<Message>();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    Task::perform(rx, |r| r.unwrap_or(Message::Noop))
}

/// Open a URL in the system browser (best effort; failures are ignored).
fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    let _ = cmd.spawn();
}
