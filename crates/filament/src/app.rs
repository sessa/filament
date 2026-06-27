//! Application state, messages, and the root view/update.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use iced::widget::{button, column, container, markdown, row, rule, space, text, text_editor};
use iced::{
    border, Background, Border, Center, Color, Element, Fill, Length, Padding, Shadow,
    Subscription, Task, Theme,
};

use filament_core::{
    automation, config::Config, git, ipc, provider, session, CodeProvider, Entry, ItemId, ItemKind,
    NewSession, SessionState, SessionStore, TerminalRec, Workspace,
};

use crate::cli::Cli;
use crate::prefs::{PrefMsg, Prefs};
use crate::sessions::{self, SessionMsg};
use crate::theme as th;
use crate::{
    editor, icon, inspector, ipc_server, settingsview, sidebar, terminal, watcher, widgets, wizard,
};

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

/// One integrated terminal tab.
pub struct TermTab {
    pub id: u64,
    pub term: iced_term::Terminal,
    /// A short label describing what's running / where.
    pub label: String,
    /// "claude" / "shell" / "manager" / "command".
    pub kind: String,
    /// The session this terminal belongs to (if any).
    pub session_id: Option<String>,
    /// The persisted [`TerminalRec`] id (when created via a session / IPC).
    pub uid: Option<String>,
}

pub struct App {
    workspace: Workspace,
    selection: Option<ItemId>,
    /// Parsed markdown bodies, cached per item so we don't re-parse every frame.
    previews: HashMap<ItemId, markdown::Content>,
    search: String,
    kind_filter: Option<ItemKind>,
    mode: Mode,
    /// Integrated terminal tabs (kept alive across hide/show).
    terminals: Vec<TermTab>,
    /// Index into `terminals` of the focused tab.
    active_tab: Option<usize>,
    terminal_open: bool,
    /// Set when a terminal failed to launch, so the panel can explain why
    /// instead of silently showing nothing.
    terminal_error: Option<String>,
    next_term_id: u64,
    /// Blink phase for the focused terminal cursor (toggled by a timer).
    term_cursor_on: bool,
    /// Which top-level section is active.
    section: Section,
    /// Persisted app preferences (appearance, density, terminal, sessions).
    prefs: Prefs,
    /// Cross-backend / automation configuration (crow's `config.json`).
    config: Config,
    /// A transient status line (automation / IPC feedback), shown in the header.
    notice: Option<String>,
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
    Cfg(CfgMsg),

    // terminal
    Terminal(iced_term::Event),
    TerminalBlink,
    ToggleTerminal,
    RunSelectedAgent,
    OpenManager,
    SelectTab(usize),
    CloseTab(u64),

    // background / ipc
    PollTick,
    Ipc(ipc::Signal),
    DismissNotice,

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

/// An automation toggle (Settings → Automation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoFlag {
    Create,
    SuggestPr,
    StartReview,
    RespondChanges,
    RespondCi,
    Merge,
    Complete,
    ManagerAuto,
    RemoteControl,
}

/// Messages that mutate the workspace [`Config`] from the Settings UI.
#[derive(Debug, Clone)]
pub enum CfgMsg {
    SetProvider(CodeProvider),
    SetTaskProvider(filament_core::TaskProvider),
    SetBranchPrefix(String),
    SetGitlabHost(String),
    SetDevRoot(String),
    SetPollSeconds(String),
    SetExcludeReview(String),
    SetExcludeTicket(String),
    SetAutoLabel(String),
    SetMergeLabel(String),
    SetJiraSite(String),
    SetJiraProject(String),
    Toggle(AutoFlag, bool),
    MarkInitialized,
}

impl App {
    pub fn new() -> (App, Task<Message>) {
        let cli = Cli::from_env();
        let workspace = Workspace::load(cli.options());
        let prefs = Prefs::load();
        let config = Config::load();

        // Resolve the active repository: prefer a saved default repo, else the
        // launch workspace *only if it is actually a git repo*. (Launched from a
        // GUI, the working directory is often `/`, which is not a repo — never
        // treat that as one.)
        let repo_hint = prefs
            .default_repo
            .clone()
            .filter(|p| git::repo_root(p).is_some())
            .or_else(|| {
                config
                    .dev_root
                    .clone()
                    .filter(|p| git::repo_root(p).is_some())
            })
            .or_else(|| cli.workspace.clone());
        let sessions = sessions::SessionsState::load(repo_hint, prefs.show_all_sessions);

        let mut app = App {
            workspace,
            selection: None,
            previews: HashMap::new(),
            search: cli.search.clone().unwrap_or_default(),
            kind_filter: None,
            mode: Mode::Inspect,
            terminals: Vec::new(),
            active_tab: None,
            terminal_open: false,
            terminal_error: None,
            next_term_id: 0,
            term_cursor_on: true,
            section: if cli.start_settings {
                Section::Settings
            } else if cli.start_sessions {
                Section::Sessions
            } else {
                Section::Config
            },
            prefs,
            config,
            notice: None,
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
    /// every terminal tab's event stream, the IPC server, and the background
    /// GitHub/GitLab poll timer.
    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![watcher::subscription(self.watch_roots())];
        for tab in &self.terminals {
            subs.push(tab.term.subscription().map(Message::Terminal));
        }
        // Blink the focused terminal cursor while the panel is showing one.
        if self.terminal_open && self.active_tab.is_some() {
            subs.push(blink_subscription());
        }
        subs.push(ipc_server::subscription(self.sessions.store.path.clone()));
        subs.push(poll_subscription(self.config.poll_seconds));
        Subscription::batch(subs)
    }

    fn term_opts(&self) -> terminal::TermOpts {
        terminal::TermOpts {
            dark: self.prefs.theme.is_dark(),
            font_size: self.prefs.terminal_font_size,
        }
    }

    /// Launch `claude` in `cwd` as a new tab.
    fn open_claude(&mut self, cwd: Option<PathBuf>) -> Task<Message> {
        let cwd = usable_cwd(cwd);
        let label = label_for("claude", cwd.as_deref());
        let settings = terminal::agent_settings(cwd, self.term_opts());
        self.open_terminal(settings, label, "claude", None, None)
    }

    /// Launch a plain shell in `cwd` as a new tab.
    fn open_shell(&mut self, cwd: Option<PathBuf>) -> Task<Message> {
        let cwd = usable_cwd(cwd);
        let label = label_for("shell", cwd.as_deref());
        let settings = terminal::shell_settings(cwd, &self.prefs.shell, self.term_opts());
        self.open_terminal(settings, label, "shell", None, None)
    }

    /// Open (or focus an existing) **manager** Claude terminal — crow's
    /// persistent orchestration session in plan/auto mode.
    fn open_manager(&mut self) -> Task<Message> {
        if let Some(idx) = self.terminals.iter().position(|t| t.kind == "manager") {
            self.active_tab = Some(idx);
            self.terminal_open = true;
            return Task::none();
        }
        let cwd = usable_cwd(
            self.config
                .dev_root
                .clone()
                .filter(|p| p.is_dir())
                .or_else(|| self.active_cwd()),
        );
        let settings = terminal::manager_settings(
            cwd,
            self.term_opts(),
            self.config.automation.manager_auto_permission,
            self.config.automation.remote_control,
        );
        self.open_terminal(settings, "manager · claude".into(), "manager", None, None)
    }

    /// Launch a session's claude/shell/command terminal as a tab, recording it
    /// in the session store so the CLI can see it.
    fn open_session_terminal(&mut self, session_id: &str, rec: &TerminalRec) -> Task<Message> {
        let cwd = usable_cwd(Some(rec.cwd.clone()));
        let opts = self.term_opts();
        let settings = match rec.kind.as_str() {
            "claude" => terminal::agent_settings(cwd, opts),
            "command" => rec
                .command
                .as_deref()
                .map(|c| terminal::command_settings(cwd.clone(), opts, c))
                .unwrap_or_else(|| terminal::shell_settings(cwd.clone(), &self.prefs.shell, opts)),
            _ => terminal::shell_settings(cwd, &self.prefs.shell, opts),
        };
        self.open_terminal(
            settings,
            rec.name.clone(),
            &rec.kind,
            Some(session_id.to_string()),
            Some(rec.id.clone()),
        )
    }

    /// Create a new terminal tab with the given settings and focus it (which also
    /// drives its first resize/render). On failure the panel stays open to show
    /// why, instead of silently showing nothing.
    fn open_terminal(
        &mut self,
        settings: iced_term::settings::Settings,
        label: String,
        kind: &str,
        session_id: Option<String>,
        uid: Option<String>,
    ) -> Task<Message> {
        let id = self.next_term_id;
        self.next_term_id += 1;
        match iced_term::Terminal::new(id, settings) {
            Ok(term) => {
                let widget_id = term.widget_id().clone();
                log::info!("terminal #{id} opened: kind={kind} label={label:?}");
                self.terminals.push(TermTab {
                    id,
                    term,
                    label,
                    kind: kind.to_string(),
                    session_id,
                    uid,
                });
                self.active_tab = Some(self.terminals.len() - 1);
                self.terminal_open = true;
                self.terminal_error = None;
                iced_term::TerminalView::focus(widget_id)
            }
            Err(e) => {
                log::error!("terminal #{id} failed to start ({kind}): {e}");
                self.terminal_open = true;
                self.terminal_error = Some(format!("Couldn't start the terminal: {e}"));
                Task::none()
            }
        }
    }

    /// Close the tab with backend id `id`, fixing up the active index.
    fn close_tab(&mut self, id: u64) {
        let Some(idx) = self.terminals.iter().position(|t| t.id == id) else {
            return;
        };
        // Drop the persisted terminal record, if any.
        if let Some(tab) = self.terminals.get(idx) {
            if let (Some(sid), Some(uid)) = (tab.session_id.clone(), tab.uid.clone()) {
                if let Some(s) = self.sessions.store.get_mut(&sid) {
                    s.terminals.retain(|t| t.id != uid);
                    let _ = self.sessions.store.save();
                }
            }
        }
        self.terminals.remove(idx);
        if self.terminals.is_empty() {
            self.active_tab = None;
            self.terminal_open = false;
        } else {
            self.active_tab = Some(idx.min(self.terminals.len() - 1));
        }
    }

    /// Type `text` (with a trailing newline) into a session's terminal — the
    /// `filament send` / auto-respond path. Falls back to the active tab.
    fn send_to_terminal(&mut self, session_id: Option<&str>, text: &str) {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(b'\n');
        let idx = session_id
            .and_then(|sid| {
                self.terminals
                    .iter()
                    .position(|t| t.session_id.as_deref() == Some(sid))
            })
            .or(self.active_tab);
        if let Some(tab) = idx.and_then(|i| self.terminals.get_mut(i)) {
            let _ = tab.term.handle(iced_term::Command::ProxyToBackend(
                iced_term::BackendCommand::Write(bytes),
            ));
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
                    // First time entering Sessions: kick a provider sync.
                    if section == Section::Sessions
                        && self.sessions.gh == sessions::GhStatus::Unknown
                        && self.sessions.repo_root.is_some()
                    {
                        return self.refresh_sessions();
                    }
                }
            }
            Message::Sessions(msg) => return self.update_sessions(msg),
            Message::Pref(msg) => self.update_prefs(msg),
            Message::Cfg(msg) => return self.update_cfg(msg),

            Message::ToggleTerminal => {
                if self.terminal_open {
                    // Visible (live terminals or an error notice) → hide it.
                    self.terminal_open = false;
                } else if !self.terminals.is_empty() || self.terminal_error.is_some() {
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
            Message::OpenManager => return self.open_manager(),
            Message::SelectTab(idx) => {
                if idx < self.terminals.len() {
                    self.active_tab = Some(idx);
                    self.terminal_open = true;
                }
            }
            Message::CloseTab(id) => self.close_tab(id),
            Message::Terminal(iced_term::Event::BackendCall(id, cmd)) => {
                if let Some(tab) = self.terminals.iter_mut().find(|t| t.id == id) {
                    let action = tab.term.handle(iced_term::Command::ProxyToBackend(cmd));
                    // Keep the cursor solid while output is flowing; the blink
                    // timer resumes the blink once the terminal goes idle.
                    tab.term.set_cursor_visible(true);
                    self.term_cursor_on = true;
                    if matches!(action, iced_term::actions::Action::Shutdown) {
                        log::info!(
                            "terminal #{id} ({}) process exited — closing tab",
                            tab.label
                        );
                        self.close_tab(id);
                    }
                }
            }
            Message::TerminalBlink => {
                self.term_cursor_on = !self.term_cursor_on;
                if let Some(tab) = self.active_tab.and_then(|i| self.terminals.get_mut(i)) {
                    tab.term.set_cursor_visible(self.term_cursor_on);
                }
            }
            Message::PollTick => {
                if self.sessions.repo_root.is_some() && self.sessions.busy.is_none() {
                    return self.refresh_sessions();
                }
            }
            Message::Ipc(signal) => return self.handle_ipc(signal),
            Message::DismissNotice => self.notice = None,

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

    fn update_cfg(&mut self, msg: CfgMsg) -> Task<Message> {
        match msg {
            CfgMsg::SetProvider(p) => self.config.default_provider = p,
            CfgMsg::SetTaskProvider(p) => self.config.default_task_provider = p,
            CfgMsg::SetBranchPrefix(s) => self.config.branch_prefix = s,
            CfgMsg::SetGitlabHost(s) => self.config.gitlab_host = s,
            CfgMsg::SetDevRoot(s) => {
                self.config.dev_root = (!s.trim().is_empty()).then(|| PathBuf::from(s.trim()));
                self.config.dev_root_buf = s;
            }
            CfgMsg::SetPollSeconds(s) => {
                let t = s.trim();
                if t.is_empty() {
                    self.config.poll_seconds = 0;
                } else if let Ok(n) = t.parse() {
                    self.config.poll_seconds = n;
                }
                self.config.poll_buf = s;
            }
            CfgMsg::SetExcludeReview(s) => {
                self.config.exclude_review_repos = split_csv(&s);
                self.config.exclude_review_buf = s;
            }
            CfgMsg::SetExcludeTicket(s) => {
                self.config.exclude_ticket_repos = split_csv(&s);
                self.config.exclude_ticket_buf = s;
            }
            CfgMsg::SetAutoLabel(s) => self.config.automation.auto_label = s,
            CfgMsg::SetMergeLabel(s) => self.config.automation.merge_label = s,
            CfgMsg::SetJiraSite(s) => self.config.jira.site_url = s,
            CfgMsg::SetJiraProject(s) => self.config.jira.project_key = s,
            CfgMsg::Toggle(flag, v) => {
                let a = &mut self.config.automation;
                match flag {
                    AutoFlag::Create => a.auto_create = v,
                    AutoFlag::SuggestPr => a.suggest_pr = v,
                    AutoFlag::StartReview => a.auto_start_review = v,
                    AutoFlag::RespondChanges => a.respond_changes_requested = v,
                    AutoFlag::RespondCi => a.respond_failed_ci = v,
                    AutoFlag::Merge => a.auto_merge = v,
                    AutoFlag::Complete => a.auto_complete = v,
                    AutoFlag::ManagerAuto => a.manager_auto_permission = v,
                    AutoFlag::RemoteControl => a.remote_control = v,
                }
            }
            CfgMsg::MarkInitialized => self.config.initialized = true,
        }
        let _ = self.config.save();
        Task::none()
    }

    /// Act on a [`ipc::Signal`] forwarded from the IPC server.
    fn handle_ipc(&mut self, signal: ipc::Signal) -> Task<Message> {
        match signal {
            ipc::Signal::Refresh => self.sessions.reload(),
            ipc::Signal::Select { session } => {
                self.section = Section::Sessions;
                self.sessions.reload();
                if self.sessions.store.get(&session).is_some() {
                    self.sessions.selected = Some(session);
                }
            }
            ipc::Signal::OpenTerminal { session, terminal } => {
                self.sessions.reload();
                return self.open_session_terminal(&session, &terminal);
            }
            ipc::Signal::CloseTerminal { terminal, .. } => {
                if let Some(id) = self
                    .terminals
                    .iter()
                    .find(|t| t.uid.as_deref() == Some(&terminal))
                    .map(|t| t.id)
                {
                    self.close_tab(id);
                }
                self.sessions.reload();
            }
            ipc::Signal::RenameTerminal { terminal, name, .. } => {
                if let Some(tab) = self
                    .terminals
                    .iter_mut()
                    .find(|t| t.uid.as_deref() == Some(&terminal))
                {
                    tab.label = name;
                }
                self.sessions.reload();
            }
            ipc::Signal::Send {
                session,
                terminal,
                text,
            } => {
                let idx = terminal.as_deref().and_then(|uid| {
                    self.terminals
                        .iter()
                        .position(|t| t.uid.as_deref() == Some(uid))
                });
                if let Some(i) = idx {
                    let mut bytes = text.into_bytes();
                    bytes.push(b'\n');
                    if let Some(tab) = self.terminals.get_mut(i) {
                        let _ = tab.term.handle(iced_term::Command::ProxyToBackend(
                            iced_term::BackendCommand::Write(bytes),
                        ));
                    }
                } else {
                    self.send_to_terminal(Some(&session), &text);
                }
            }
            ipc::Signal::Hook { .. } => self.sessions.reload(),
        }
        Task::none()
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
                self.sessions.renaming = None;
                self.sessions.confirming = None;
                self.sessions.linking = None;
            }
            SessionMsg::SetView(v) => self.sessions.view = v,
            SessionMsg::FilterChanged(v) => self.sessions.filter = v,
            SessionMsg::SetTicketStatus(s) => self.sessions.ticket_status = s,
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
            SessionMsg::StartReviewPr(number) => return self.start_review_pr(number),
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
                    self.sessions.selected = Some(id.clone());
                    let cwd = usable_cwd(Some(cwd));
                    let label = label_for("claude", cwd.as_deref());
                    let settings = terminal::agent_settings(cwd, self.term_opts());
                    return self.open_terminal(settings, label, "claude", Some(id), None);
                }
            }
            SessionMsg::OpenShell(id) => {
                if let Some(cwd) = self.sessions.store.get(&id).map(|s| s.worktree.clone()) {
                    self.sessions.selected = Some(id.clone());
                    let cwd = usable_cwd(Some(cwd));
                    let label = label_for("shell", cwd.as_deref());
                    let settings =
                        terminal::shell_settings(cwd, &self.prefs.shell, self.term_opts());
                    return self.open_terminal(settings, label, "shell", Some(id), None);
                }
            }
            SessionMsg::SetStatus(id, state) => {
                if let Some(s) = self.sessions.store.get_mut(&id) {
                    if state == SessionState::Working {
                        s.set_manual(None);
                    } else {
                        s.set_manual(Some(state));
                    }
                    let _ = self.sessions.store.save();
                    self.sessions.reload();
                }
            }
            SessionMsg::CopyBranch(branch) => {
                self.notice = Some(format!("Copied branch: {branch}"));
                return iced::clipboard::write(branch);
            }
            SessionMsg::CreatePr(id) => return self.create_pr(id),
            SessionMsg::PrCreated(result) => {
                self.sessions.busy = None;
                match result {
                    Ok(url) => {
                        self.notice = Some(format!("Opened PR: {url}"));
                        return self.refresh_sessions();
                    }
                    Err(e) => self.sessions.error = Some(e),
                }
            }
            SessionMsg::RenameStart(id) => {
                let title = self
                    .sessions
                    .store
                    .get(&id)
                    .map(|s| s.title.clone())
                    .unwrap_or_default();
                self.sessions.renaming = Some((id, title));
            }
            SessionMsg::RenameInput(v) => {
                if let Some((_, buf)) = &mut self.sessions.renaming {
                    *buf = v;
                }
            }
            SessionMsg::RenameCommit => {
                if let Some((id, buf)) = self.sessions.renaming.take() {
                    let title = buf.trim().to_string();
                    if !title.is_empty() {
                        if let Some(s) = self.sessions.store.get_mut(&id) {
                            s.title = title;
                            let _ = self.sessions.store.save();
                        }
                    }
                    self.sessions.reload();
                }
            }
            SessionMsg::RenameCancel => self.sessions.renaming = None,
            SessionMsg::AddLinkStart(id) => {
                self.sessions.linking = Some(sessions::LinkForm {
                    session: id,
                    ..Default::default()
                });
            }
            SessionMsg::LinkLabel(v) => {
                if let Some(f) = &mut self.sessions.linking {
                    f.label = v;
                }
            }
            SessionMsg::LinkUrl(v) => {
                if let Some(f) = &mut self.sessions.linking {
                    f.url = v;
                }
            }
            SessionMsg::LinkCommit => {
                if let Some(f) = self.sessions.linking.take() {
                    if !f.label.trim().is_empty() && !f.url.trim().is_empty() {
                        if let Some(s) = self.sessions.store.get_mut(&f.session) {
                            s.links.push(filament_core::SessionLink {
                                label: f.label.trim().to_string(),
                                url: f.url.trim().to_string(),
                                kind: "link".into(),
                            });
                            let _ = self.sessions.store.save();
                        }
                    }
                    self.sessions.reload();
                }
            }
            SessionMsg::LinkCancel => self.sessions.linking = None,
            SessionMsg::RemoveLink(idx) => {
                if let Some(id) = self.sessions.selected.clone() {
                    if let Some(s) = self.sessions.store.get_mut(&id) {
                        if idx < s.links.len() {
                            s.links.remove(idx);
                            let _ = self.sessions.store.save();
                        }
                    }
                    self.sessions.reload();
                }
            }
            SessionMsg::ToggleMark(id) => {
                if !self.sessions.marked.remove(&id) {
                    self.sessions.marked.insert(id);
                }
            }
            SessionMsg::ClearMarks => self.sessions.marked.clear(),
            SessionMsg::DeleteMarked => return self.delete_marked(),
            SessionMsg::AskDelete(id) => self.sessions.confirming = Some(id),
            SessionMsg::CancelDelete => self.sessions.confirming = None,
            SessionMsg::Delete(id) => {
                self.sessions.confirming = None;
                return self.delete_session(id, true);
            }
            SessionMsg::RemoveOnly(id) => {
                self.sessions.confirming = None;
                return self.delete_session(id, false);
            }
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
            SessionMsg::Refreshed(status, issues, review_prs, notice) => {
                self.sessions.gh = status;
                self.sessions.issues = issues;
                self.sessions.review_prs = review_prs;
                self.sessions.reload();
                self.sessions.busy = None;
                if notice.is_some() {
                    self.notice = notice;
                }
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
        let now = now_unix();
        let prefix = self.config.branch_prefix_for(&repo);
        let provider = self.config.provider_for(&repo);
        let task_provider = self.config.task_provider_for(&repo);
        let host = self.config.host_for(&repo);
        let jira = self.config.jira.clone();
        self.sessions.busy = Some("Creating session…".into());
        self.sessions.error = None;
        run_async(move || {
            let mut store = SessionStore::load_at(store_path);
            let key = form.issue_url.trim();
            let issue = (!key.is_empty())
                .then(|| {
                    provider::view_issue(task_provider, &repo, host.as_deref(), &jira, key).ok()
                })
                .flatten();
            let req = NewSession {
                title: form.title.clone(),
                base_branch: form.base.clone(),
                issue,
                provider,
                task_provider,
            };
            let result = match session::create_session(&mut store, &repo, req, &prefix, now) {
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
        let prefix = self.config.branch_prefix_for(&repo);
        let provider = self.config.provider_for(&repo);
        let task_provider = self.config.task_provider_for(&repo);
        self.sessions.busy = Some(format!("Creating session for #{number}…"));
        self.sessions.error = None;
        run_async(move || {
            let mut store = SessionStore::load_at(store_path);
            let req = NewSession {
                title: issue.title.clone(),
                base_branch: base,
                issue: Some(issue),
                provider,
                task_provider,
            };
            let result = match session::create_session(&mut store, &repo, req, &prefix, now) {
                Ok(s) => {
                    let _ = store.save();
                    Ok(s.id)
                }
                Err(e) => Err(e.to_string()),
            };
            Message::Sessions(SessionMsg::Created(result))
        })
    }

    fn delete_session(&mut self, id: String, delete_worktree: bool) -> Task<Message> {
        let store_path = self.sessions.store.path.clone();
        if self.sessions.selected.as_deref() == Some(&id) {
            self.sessions.selected = None;
        }
        self.sessions.busy = Some("Removing session…".into());
        self.sessions.error = None;
        run_async(move || {
            let mut store = SessionStore::load_at(store_path);
            let result = session::remove_session(&mut store, &id, delete_worktree)
                .map_err(|e| e.to_string());
            if result.is_ok() {
                let _ = store.save();
            }
            Message::Sessions(SessionMsg::Deleted(result))
        })
    }

    fn delete_marked(&mut self) -> Task<Message> {
        let ids: Vec<String> = self.sessions.marked.iter().cloned().collect();
        if ids.is_empty() {
            return Task::none();
        }
        let store_path = self.sessions.store.path.clone();
        self.sessions.marked.clear();
        self.sessions.busy = Some(format!("Removing {} session(s)…", ids.len()));
        self.sessions.error = None;
        run_async(move || {
            let mut store = SessionStore::load_at(store_path);
            for id in ids {
                let _ = session::remove_session(&mut store, &id, true);
            }
            let _ = store.save();
            Message::Sessions(SessionMsg::Deleted(Ok(())))
        })
    }

    fn start_review_pr(&mut self, number: u64) -> Task<Message> {
        let Some(repo) = self.sessions.repo_root.clone() else {
            return Task::none();
        };
        let Some(pr) = self
            .sessions
            .review_prs
            .iter()
            .find(|p| p.number == number)
            .cloned()
        else {
            return Task::none();
        };
        let Some(branch) = pr.head.clone() else {
            self.sessions.error = Some("PR head branch unknown — Refresh and try again.".into());
            return Task::none();
        };
        let store_path = self.sessions.store.path.clone();
        let provider = self.config.provider_for(&repo);
        let now = now_unix();
        self.sessions.busy = Some(format!("Starting review of #{number}…"));
        self.sessions.error = None;
        run_async(move || {
            let mut store = SessionStore::load_at(store_path);
            let result =
                match session::create_review_session(&mut store, &repo, &branch, pr, provider, now)
                {
                    Ok(s) => {
                        let _ = store.save();
                        Ok(s.id)
                    }
                    Err(e) => Err(e.to_string()),
                };
            Message::Sessions(SessionMsg::Created(result))
        })
    }

    fn create_pr(&mut self, id: String) -> Task<Message> {
        let Some(repo) = self.sessions.repo_root.clone() else {
            return Task::none();
        };
        let Some((provider, branch, title)) = self
            .sessions
            .store
            .get(&id)
            .map(|s| (s.provider, s.branch.clone(), s.title.clone()))
        else {
            return Task::none();
        };
        let host = self.config.host_for(&repo);
        self.sessions.busy = Some("Opening PR…".into());
        self.sessions.error = None;
        run_async(move || {
            let res = provider::create_pr(provider, &repo, host.as_deref(), &branch, &title, false)
                .map_err(|e| e.to_string());
            Message::Sessions(SessionMsg::PrCreated(res))
        })
    }

    fn refresh_sessions(&mut self) -> Task<Message> {
        let Some(repo) = self.sessions.repo_root.clone() else {
            return Task::none();
        };
        let store_path = self.sessions.store.path.clone();
        let task_provider = self.config.task_provider_for(&repo);
        let code_provider = self.config.provider_for(&repo);
        let host = self.config.host_for(&repo);
        let jira = self.config.jira.clone();
        let config = self.config.clone();
        let now = now_unix();
        self.sessions.busy = Some("Syncing…".into());
        run_async(move || {
            let mut store = SessionStore::load_at(store_path);
            let issues_res =
                provider::list_open_issues(task_provider, &repo, host.as_deref(), &jira, 50);
            let status = match &issues_res {
                Ok(_) => sessions::GhStatus::Ready,
                Err(provider::ProviderError::NotInstalled) => sessions::GhStatus::NotInstalled,
                Err(provider::ProviderError::NotAuthenticated) => {
                    sessions::GhStatus::NotAuthenticated
                }
                Err(e) => sessions::GhStatus::Error(e.to_string()),
            };
            let issues = issues_res.unwrap_or_default();

            if status == sessions::GhStatus::Ready {
                let ids: Vec<String> = store.for_repo(&repo).map(|s| s.id.clone()).collect();
                for id in ids {
                    let Some((branch, prov, tprov, issue_key)) = store.get(&id).map(|s| {
                        (
                            s.branch.clone(),
                            s.provider,
                            s.task_provider,
                            s.issue.as_ref().map(|i| i.number.to_string()),
                        )
                    }) else {
                        continue;
                    };
                    let pr = provider::pr_for_branch(prov, &repo, host.as_deref(), &branch)
                        .ok()
                        .flatten();
                    let issue = issue_key.and_then(|k| {
                        provider::view_issue(tprov, &repo, host.as_deref(), &jira, &k).ok()
                    });
                    if let Some(s) = store.get_mut(&id) {
                        if pr.is_some() {
                            s.pr = pr;
                        }
                        if issue.is_some() {
                            s.issue = issue;
                        }
                        s.sync_state();
                        s.last_synced_unix = now;
                    }
                }
                let _ = store.save();
            }

            // Open PRs for the review board.
            let review_prs = if status == sessions::GhStatus::Ready {
                provider::list_review_prs(code_provider, &repo, host.as_deref(), 50)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            let notice = apply_automation(&mut store, &repo, &issues, &config, now);
            let _ = store.save();
            Message::Sessions(SessionMsg::Refreshed(status, issues, review_prs, notice))
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
            Section::Sessions => self.sessions.detail(&self.config, &theme),
            Section::Settings => settingsview::view(
                &self.prefs,
                &self.config,
                &theme,
                self.sessions.repo_root.as_deref(),
            ),
        };

        // Right pane: the detail/editor in a glass panel, terminal docked below.
        let detail_panel = container(detail)
            .width(Fill)
            .height(Length::FillPortion(3))
            .clip(true)
            .style(widgets::panel(&theme));

        let docked: Option<Element<Message>> = if self.terminal_open {
            if !self.terminals.is_empty() {
                Some(self.terminal_panel(&theme))
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

    fn terminal_panel<'a>(&'a self, theme: &Theme) -> Element<'a, Message> {
        let accent = theme.palette().primary;
        let txt = theme.palette().text;
        let muted = th::muted(theme);
        let bg = th::surface_strong(theme);
        let bdr = th::hairline(theme);
        let shadow = th::panel_shadow();

        // Tab strip across all terminals, with a close (×) on each.
        let mut tabs =
            row![icon::icon(icon::TERMINAL)
                .size(13)
                .style(move |_: &Theme| text::Style {
                    color: Some(accent)
                })]
            .spacing(6)
            .align_y(Center);
        for (i, tab) in self.terminals.iter().enumerate() {
            let active = self.active_tab == Some(i);
            let id = tab.id;
            let label = tab.label.clone();
            let chip = button(
                row![
                    text(label).size(th::TEXT_META),
                    button(icon::icon(icon::CLOSE).size(10))
                        .padding(0)
                        .on_press(Message::CloseTab(id))
                        .style(|_t, _s| button::Style {
                            background: None,
                            text_color: Color::from_rgb(0.7, 0.4, 0.4),
                            border: border::rounded(4),
                            shadow: Shadow::default(),
                            snap: true,
                        }),
                ]
                .spacing(6)
                .align_y(Center),
            )
            .padding(Padding {
                top: 3.0,
                right: 8.0,
                bottom: 3.0,
                left: 10.0,
            })
            .on_press(Message::SelectTab(i))
            .style(move |_t, status| {
                let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
                let bgc = if active {
                    th::with_alpha(accent, 0.22)
                } else if hovered {
                    th::with_alpha(txt, 0.10)
                } else {
                    Color::TRANSPARENT
                };
                button::Style {
                    background: Some(Background::Color(bgc)),
                    text_color: if active {
                        txt
                    } else {
                        th::with_alpha(txt, 0.75)
                    },
                    border: border::rounded(7),
                    shadow: Shadow::default(),
                    snap: true,
                }
            });
            tabs = tabs.push(chip);
        }
        tabs = tabs.push(space().width(Fill));
        tabs = tabs.push(widgets::icon_only(
            icon::TERMINAL,
            Message::ToggleTerminal,
            theme,
        ));

        let bar = container(tabs)
            .padding(Padding {
                top: 5.0,
                right: 6.0,
                bottom: 5.0,
                left: 12.0,
            })
            .width(Fill);

        // The embedded `iced_term` canvas needs a fully opaque backing: the app
        // window is transparent (glass), and the panel's own `surface_strong`
        // fill is ~7% alpha, so without this the terminal composites against the
        // blurred desktop and reads as blank. Match the exact color `iced_term`'s
        // palette uses for the terminal background (see `terminal::palette`).
        let term_bg = if self.prefs.theme.is_dark() {
            Color::from_rgb8(0x1B, 0x1A, 0x18)
        } else {
            Color::from_rgb8(0xFB, 0xFA, 0xF6)
        };
        let inner: Element<Message> = match self.active_tab.and_then(|i| self.terminals.get(i)) {
            Some(tab) => container(iced_term::TerminalView::show(&tab.term).map(Message::Terminal))
                .padding(Padding {
                    top: 10.0,
                    right: 14.0,
                    bottom: 10.0,
                    left: 14.0,
                })
                .width(Fill)
                .height(Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(term_bg)),
                    ..container::Style::default()
                })
                .into(),
            None => container(
                text("No terminal")
                    .size(th::TEXT_META)
                    .style(move |_| text::Style { color: Some(muted) }),
            )
            .center_x(Fill)
            .center_y(Fill)
            .into(),
        };

        container(column![bar, rule::horizontal(1), inner].height(Fill))
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
        ]
        .align_y(Center)
        .spacing(4);

        let bar = match &self.notice {
            Some(n) => {
                let chip = button(
                    row![
                        icon::icon(icon::INFO).size(11),
                        text(n.clone()).size(th::TEXT_META),
                        icon::icon(icon::CLOSE).size(10),
                    ]
                    .spacing(6)
                    .align_y(Center),
                )
                .padding(Padding {
                    top: 3.0,
                    right: 9.0,
                    bottom: 3.0,
                    left: 9.0,
                })
                .on_press(Message::DismissNotice)
                .style(move |_t, _s| button::Style {
                    background: Some(Background::Color(th::with_alpha(accent, 0.14))),
                    text_color: accent,
                    border: border::rounded(8),
                    shadow: Shadow::default(),
                    snap: true,
                });
                bar.push(chip).push(space().width(8.0))
            }
            None => bar,
        };
        let bar = bar.push(right).align_y(Center).spacing(4);

        // The macOS title-bar strip (where the traffic lights live) is added in
        // `view`; here the header is just the rounded toolbar panel, the same on
        // every platform.
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

/// Split a comma-separated text field into trimmed, non-empty entries.
fn split_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Execute the enabled automation rules against `store` in the background (mutating
/// the store and, for merge/create, calling git / the forge). Returns a short
/// notice summarizing what happened. Respond / start-review actions are surfaced
/// rather than executed here (they need the live terminals).
fn apply_automation(
    store: &mut SessionStore,
    repo: &std::path::Path,
    issues: &[filament_core::IssueRef],
    config: &Config,
    now: u64,
) -> Option<String> {
    let plan = automation::plan(store, repo, issues, config);
    if plan.is_empty() {
        return None;
    }
    let host = config.host_for(repo);
    let mut applied = 0u32;
    let mut surfaced = 0u32;
    for action in plan.actions {
        match action {
            automation::AutoAction::Complete { session_id } => {
                if let Some(s) = store.get_mut(&session_id) {
                    s.set_manual(None);
                    s.state = SessionState::Done;
                    applied += 1;
                }
            }
            automation::AutoAction::SuggestPr { session_id } => {
                if let Some(s) = store.get_mut(&session_id) {
                    s.pr_suggested = true;
                    applied += 1;
                }
            }
            automation::AutoAction::Merge { session_id } => {
                if let Some((prov, branch)) = store
                    .get(&session_id)
                    .map(|s| (s.provider, s.branch.clone()))
                {
                    if provider::merge_pr(prov, repo, host.as_deref(), &branch).is_ok() {
                        applied += 1;
                    }
                }
            }
            automation::AutoAction::CreateSession { issue } => {
                let req = NewSession {
                    title: issue.title.clone(),
                    base_branch: git::default_branch(repo).unwrap_or_else(|| "main".into()),
                    issue: Some(issue),
                    provider: config.provider_for(repo),
                    task_provider: config.task_provider_for(repo),
                };
                let prefix = config.branch_prefix_for(repo);
                if session::create_session(store, repo, req, &prefix, now).is_ok() {
                    applied += 1;
                }
            }
            automation::AutoAction::StartReview { .. }
            | automation::AutoAction::RespondChangesRequested { .. }
            | automation::AutoAction::RespondFailedCi { .. } => surfaced += 1,
        }
    }
    if applied == 0 && surfaced == 0 {
        None
    } else {
        let mut parts = Vec::new();
        if applied > 0 {
            parts.push(format!("automation applied {applied} action(s)"));
        }
        if surfaced > 0 {
            parts.push(format!("{surfaced} need attention"));
        }
        Some(parts.join("; "))
    }
}

/// A ~530ms timer that emits [`Message::TerminalBlink`] to drive the focused
/// terminal's cursor blink (a common terminal cadence).
fn blink_subscription() -> Subscription<Message> {
    Subscription::run_with(0u8, |_| {
        iced::stream::channel(
            4,
            move |mut out: iced::futures::channel::mpsc::Sender<Message>| async move {
                std::thread::spawn(move || loop {
                    std::thread::sleep(Duration::from_millis(530));
                    if out.try_send(Message::TerminalBlink).is_err() {
                        break;
                    }
                });
                std::future::pending::<()>().await;
            },
        )
    })
}

/// A timer subscription that emits [`Message::PollTick`] every `seconds`
/// (disabled when `seconds == 0`). Keyed by the interval so it restarts when the
/// poll setting changes.
fn poll_subscription(seconds: u64) -> Subscription<Message> {
    if seconds == 0 {
        return Subscription::none();
    }
    Subscription::run_with(seconds, |secs: &u64| {
        let secs = *secs;
        iced::stream::channel(
            4,
            move |mut out: iced::futures::channel::mpsc::Sender<Message>| async move {
                std::thread::spawn(move || loop {
                    std::thread::sleep(Duration::from_secs(secs));
                    if out.try_send(Message::PollTick).is_err() {
                        break;
                    }
                });
                std::future::pending::<()>().await;
            },
        )
    })
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
