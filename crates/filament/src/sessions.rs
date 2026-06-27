//! The **Sessions** section — a crow-style worktree workflow.
//!
//! State + views for pairing a git worktree with a Claude Code instance and an
//! optional issue / pull request, across three boards (sessions pipeline, PR
//! review, and the project ticket board). The orchestration (spawning git/gh off
//! the UI thread, automation, IPC) lives in [`crate::app`]; this module owns the
//! data the view needs and renders the board sidebar and the detail / forms. All
//! provider access degrades gracefully when the CLIs are absent.

use std::collections::HashSet;
use std::path::PathBuf;

use iced::widget::{button, checkbox, column, container, pick_list, row, space, text, text_input};
use iced::{border, Background, Center, Color, Element, Fill, Padding, Shadow, Theme};

use filament_core::{
    config::Config, git, github, session, CheckState, IssueRef, MergeReadiness, PrRef,
    ProjectStatus, Session, SessionState, SessionStore, Worktree,
};

use crate::app::Message;
use crate::icon;
use crate::theme as th;
use crate::widgets;

/// Which board is shown in the Sessions sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionView {
    #[default]
    Board,
    Review,
    Tickets,
}

/// Messages emitted by the Sessions section (wrapped in [`Message::Sessions`]).
#[derive(Debug, Clone)]
pub enum SessionMsg {
    Select(String),
    SetView(SessionView),
    FilterChanged(String),
    SetTicketStatus(Option<String>),
    /// Open / close the new-session form.
    ToggleNew,
    FormTitle(String),
    FormIssue(String),
    FormBase(String),
    /// Create from the open form.
    Create,
    /// Create directly from a listed issue ("Start working").
    StartIssue(u64),
    /// Start a review session from an open PR on the review board.
    StartReviewPr(u64),
    /// A create finished: `Ok(id)` to select, or an error message.
    Created(Result<String, String>),
    /// Launch `claude` in the session's worktree.
    OpenAgent(String),
    /// Launch a plain shell in the session's worktree.
    OpenShell(String),
    /// Set a session's status (pause / archive / mark in review / return active).
    SetStatus(String, SessionState),
    /// Copy a session's branch name to the clipboard.
    CopyBranch(String),
    /// Open (or create) the session's PR.
    CreatePr(String),
    PrCreated(Result<String, String>),
    // rename
    RenameStart(String),
    RenameInput(String),
    RenameCommit,
    RenameCancel,
    // links
    AddLinkStart(String),
    LinkLabel(String),
    LinkUrl(String),
    LinkCommit,
    LinkCancel,
    RemoveLink(usize),
    // multi-select
    ToggleMark(String),
    ClearMarks,
    DeleteMarked,
    // deletion (with confirmation)
    AskDelete(String),
    CancelDelete,
    Delete(String),
    RemoveOnly(String),
    Deleted(Result<(), String>),
    // sync
    Refresh,
    Refreshed(GhStatus, Vec<IssueRef>, Vec<PrRef>, Option<String>),
    AdoptOrphans,
    Adopted,
    OpenUrl(String),
    RepoInputChanged(String),
    OpenRepo,
    /// Pick a repository folder with the native file dialog.
    BrowseRepo,
    /// Switch the active repository to a known path.
    SetRepo(PathBuf),
}

/// Availability of the provider CLI, surfaced as a quiet hint in the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhStatus {
    Unknown,
    Ready,
    NotInstalled,
    NotAuthenticated,
    Error(String),
}

impl GhStatus {
    fn hint(&self) -> Option<&str> {
        match self {
            GhStatus::Unknown | GhStatus::Ready => None,
            GhStatus::NotInstalled => {
                Some("Provider CLI (gh / glab) not found — issues & PR status disabled")
            }
            GhStatus::NotAuthenticated => {
                Some("Authenticate the provider CLI to load issues & PR status")
            }
            GhStatus::Error(_) => Some("Couldn't reach the provider"),
        }
    }
}

/// The new-session form.
#[derive(Debug, Clone, Default)]
pub struct NewForm {
    pub title: String,
    pub issue_url: String,
    pub base: String,
}

/// The add-link form.
#[derive(Debug, Clone, Default)]
pub struct LinkForm {
    pub session: String,
    pub label: String,
    pub url: String,
}

/// All state backing the Sessions section.
pub struct SessionsState {
    pub store: SessionStore,
    pub repo_root: Option<PathBuf>,
    pub branches: Vec<String>,
    pub default_branch: Option<String>,
    pub selected: Option<String>,
    pub issues: Vec<IssueRef>,
    pub review_prs: Vec<PrRef>,
    pub orphans: Vec<Worktree>,
    pub gh: GhStatus,
    pub gh_present: bool,
    pub compose: Option<NewForm>,
    pub busy: Option<String>,
    pub error: Option<String>,
    /// Show sessions from every repo, not just the active one.
    pub show_all: bool,
    /// The repository path being typed in the "open repository" field.
    pub repo_input: String,
    /// Which board is shown.
    pub view: SessionView,
    /// Inline text filter across sessions / tickets.
    pub filter: String,
    /// Ticket board column filter (a pipeline label), if any.
    pub ticket_status: Option<String>,
    /// Multi-selected session ids (for batch actions).
    pub marked: HashSet<String>,
    /// In-progress rename: `(session id, buffer)`.
    pub renaming: Option<(String, String)>,
    /// Session id pending a delete confirmation.
    pub confirming: Option<String>,
    /// In-progress add-link form.
    pub linking: Option<LinkForm>,
}

impl SessionsState {
    /// Load persisted sessions and probe the repo / gh once at startup.
    pub fn load(workspace: Option<PathBuf>, show_all: bool) -> SessionsState {
        let repo_root = workspace.and_then(|w| git::repo_root(&w));
        let store = SessionStore::load();
        let gh_present = github::cli_available();
        let (branches, default_branch, orphans) = match &repo_root {
            Some(root) => (
                git::list_branches(root),
                git::default_branch(root),
                session::detect_orphans(&store, root),
            ),
            None => (Vec::new(), None, Vec::new()),
        };
        let selected = repo_root
            .as_ref()
            .and_then(|r| store.for_repo(r).next().map(|s| s.id.clone()))
            .or_else(|| store.sessions.first().map(|s| s.id.clone()));
        let repo_input = repo_root
            .as_ref()
            .map(|r| r.display().to_string())
            .unwrap_or_default();
        SessionsState {
            store,
            repo_root,
            branches,
            default_branch,
            selected,
            issues: Vec::new(),
            review_prs: Vec::new(),
            orphans,
            gh: GhStatus::Unknown,
            gh_present,
            compose: None,
            busy: None,
            error: None,
            show_all,
            repo_input,
            view: SessionView::Board,
            filter: String::new(),
            ticket_status: None,
            marked: HashSet::new(),
            renaming: None,
            confirming: None,
            linking: None,
        }
    }

    /// Switch the active repository and recompute repo-derived state.
    pub fn set_repo(&mut self, repo_root: Option<PathBuf>) {
        self.repo_root = repo_root;
        self.compose = None;
        self.gh = GhStatus::Unknown;
        self.marked.clear();
        self.renaming = None;
        self.confirming = None;
        self.linking = None;
        if let Some(root) = &self.repo_root {
            self.repo_input = root.display().to_string();
        }
        self.reload();
    }

    /// Distinct repositories that have sessions, plus the active one.
    pub fn recent_repos(&self) -> Vec<PathBuf> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for repo in self
            .repo_root
            .iter()
            .cloned()
            .chain(self.store.sessions.iter().map(|s| s.repo_root.clone()))
        {
            if seen.insert(repo.clone()) {
                out.push(repo);
            }
        }
        out
    }

    /// Reload the store from disk and recompute repo-derived state.
    pub fn reload(&mut self) {
        self.store = SessionStore::load_at(self.store.path.clone());
        if let Some(root) = &self.repo_root {
            self.branches = git::list_branches(root);
            self.default_branch = git::default_branch(root);
            self.orphans = session::detect_orphans(&self.store, root);
        }
        // Drop marks / rename / link / confirm targets whose session is gone.
        self.marked.retain(|id| self.store.get(id).is_some());
        let gone = |id: &str| self.store.get(id).is_none();
        if self.renaming.as_ref().is_some_and(|(id, _)| gone(id)) {
            self.renaming = None;
        }
        if self.confirming.as_deref().is_some_and(gone) {
            self.confirming = None;
        }
        if self.linking.as_ref().is_some_and(|f| gone(&f.session)) {
            self.linking = None;
        }
    }

    fn in_scope(&self, s: &Session) -> bool {
        self.show_all
            || self.repo_root.is_none()
            || self
                .repo_root
                .as_ref()
                .is_some_and(|root| same_repo(&s.repo_root, root))
    }

    fn matches_filter(&self, s: &Session) -> bool {
        let q = self.filter.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        s.title.to_lowercase().contains(&q)
            || s.branch.to_lowercase().contains(&q)
            || s.issue.as_ref().is_some_and(|i| {
                i.title.to_lowercase().contains(&q) || i.number.to_string().contains(&q)
            })
    }

    /// Sessions grouped into board columns (Working → Review → Done), plus the
    /// Paused / Archived side groups, honoring scope and the inline filter.
    fn columns(&self) -> Vec<(SessionState, Vec<&Session>)> {
        let order = [
            SessionState::Working,
            SessionState::Review,
            SessionState::Done,
            SessionState::Paused,
            SessionState::Archived,
        ];
        order
            .iter()
            .map(|state| {
                let items: Vec<&Session> = self
                    .store
                    .sessions
                    .iter()
                    .filter(|s| s.state == *state && self.in_scope(s) && self.matches_filter(s))
                    .collect();
                (*state, items)
            })
            .collect()
    }

    /// Sessions in review, for the review board.
    fn review_sessions(&self) -> Vec<&Session> {
        self.store
            .sessions
            .iter()
            .filter(|s| {
                s.state == SessionState::Review && self.in_scope(s) && self.matches_filter(s)
            })
            .collect()
    }

    /// Open PRs (from the last refresh) that don't already back a session.
    fn unsessioned_prs(&self) -> Vec<&PrRef> {
        let taken: HashSet<u64> = self
            .store
            .sessions
            .iter()
            .filter_map(|s| s.pr.as_ref().map(|p| p.number))
            .collect();
        self.review_prs
            .iter()
            .filter(|p| p.is_open() && !taken.contains(&p.number))
            .collect()
    }

    fn showing_all(&self) -> bool {
        self.show_all || self.repo_root.is_none()
    }

    fn selected_session(&self) -> Option<&Session> {
        let id = self.selected.as_ref()?;
        self.store.get(id)
    }

    /// Open issues that don't already have a session (crow's "tickets").
    fn open_tickets(&self) -> Vec<&IssueRef> {
        let taken: HashSet<u64> = self
            .store
            .sessions
            .iter()
            .filter_map(|s| s.issue.as_ref().map(|i| i.number))
            .collect();
        let q = self.filter.trim().to_lowercase();
        self.issues
            .iter()
            .filter(|i| !taken.contains(&i.number))
            .filter(|i| {
                self.ticket_status.as_deref().is_none_or(|want| {
                    i.status()
                        .map(|s| s.label() == want)
                        .unwrap_or(want == "Backlog")
                })
            })
            .filter(|i| {
                q.is_empty()
                    || i.title.to_lowercase().contains(&q)
                    || i.number.to_string().contains(&q)
            })
            .collect()
    }

    // ---- top-level views ----------------------------------------------------

    /// The left board: view switcher, status hints, and the active board.
    pub fn sidebar<'a>(&'a self, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let mut col = column![].spacing(2);

        if let Some(hint) = self.gh.hint() {
            col = col.push(banner(hint, muted));
        }
        if !self.orphans.is_empty() {
            let label = format!("{} untracked worktree(s) found", self.orphans.len());
            col = col.push(orphan_banner(label));
        }
        if let Some(busy) = &self.busy {
            col = col.push(banner(busy, theme.palette().primary));
        }
        if let Some(err) = &self.error {
            col = col.push(banner(err, th::danger()));
        }
        if !self.marked.is_empty() {
            col = col.push(self.bulk_bar(theme));
        }

        col = match self.view {
            SessionView::Board => self.board_body(col, theme),
            SessionView::Review => self.review_body(col, theme),
            SessionView::Tickets => self.tickets_body(col, theme),
        };

        let header = container(
            column![
                row![
                    widgets::primary_button(
                        icon::NEW,
                        "New session",
                        self.repo_root
                            .is_some()
                            .then_some(Message::Sessions(SessionMsg::ToggleNew)),
                        theme,
                    ),
                    space().width(Fill),
                    widgets::icon_button(icon::AGENT, "Manager", Message::OpenManager, theme),
                ]
                .align_y(Center)
                .spacing(6),
                self.view_switcher(theme),
                text_input("Filter…", &self.filter)
                    .on_input(|s| Message::Sessions(SessionMsg::FilterChanged(s)))
                    .size(th::TEXT_META)
                    .padding(6),
                self.repo_bar(theme),
            ]
            .spacing(8),
        )
        .padding(Padding {
            top: 12.0,
            right: 10.0,
            bottom: 8.0,
            left: 10.0,
        });

        let list = widgets::scroll(
            container(col).padding(Padding {
                top: 0.0,
                right: 8.0,
                bottom: 12.0,
                left: 8.0,
            }),
            theme,
        );

        column![header, list].height(Fill).into()
    }

    fn board_body<'a>(
        &'a self,
        mut col: iced::widget::Column<'a, Message>,
        theme: &Theme,
    ) -> iced::widget::Column<'a, Message> {
        let muted = th::muted(theme);
        let mut any = false;
        for (state, items) in self.columns() {
            if items.is_empty() {
                continue;
            }
            any = true;
            col = col.push(group_header(
                state.label(),
                state_color(state, theme),
                items.len(),
                muted,
            ));
            for s in items {
                col = col.push(self.session_row(s, theme));
            }
            col = col.push(space().height(8.0));
        }
        if !any {
            let msg = if self.repo_root.is_some() {
                "No sessions yet. Create one to pair a git worktree with Claude Code."
            } else {
                "No sessions yet. Open a git repository below, then create one."
            };
            col = col.push(
                text(msg)
                    .size(th::TEXT_BODY)
                    .style(move |_| text::Style { color: Some(muted) }),
            );
        }
        col
    }

    fn review_body<'a>(
        &'a self,
        mut col: iced::widget::Column<'a, Message>,
        theme: &Theme,
    ) -> iced::widget::Column<'a, Message> {
        let muted = th::muted(theme);
        let sessions = self.review_sessions();
        col = col.push(group_header(
            "In review",
            th::amber(),
            sessions.len(),
            muted,
        ));
        if sessions.is_empty() {
            col = col.push(
                text("No sessions are in review.")
                    .size(th::TEXT_META)
                    .style(move |_| text::Style { color: Some(muted) }),
            );
        }
        for s in sessions {
            col = col.push(self.session_row(s, theme));
        }
        let prs = self.unsessioned_prs();
        if !prs.is_empty() {
            col = col.push(space().height(8.0));
            col = col.push(group_header("Open PRs", muted, prs.len(), muted));
            for pr in prs {
                col = col.push(review_pr_row(pr, theme));
            }
        }
        col
    }

    fn tickets_body<'a>(
        &'a self,
        mut col: iced::widget::Column<'a, Message>,
        theme: &Theme,
    ) -> iced::widget::Column<'a, Message> {
        let muted = th::muted(theme);
        col = col.push(self.ticket_filter(theme));
        let tickets = self.open_tickets();
        if tickets.is_empty() {
            col = col.push(
                text("No open tickets (or none match the filter).")
                    .size(th::TEXT_META)
                    .style(move |_| text::Style { color: Some(muted) }),
            );
            return col;
        }
        // Group tickets by pipeline column.
        for status in ProjectStatus::PIPELINE {
            let label = status.label().to_string();
            let group: Vec<&IssueRef> = tickets
                .iter()
                .copied()
                .filter(|i| {
                    i.status()
                        .map(|s| s.label() == label)
                        .unwrap_or(label == "Backlog")
                })
                .collect();
            if group.is_empty() {
                continue;
            }
            col = col.push(group_header(label.clone(), muted, group.len(), muted));
            for issue in group {
                col = col.push(ticket_row(issue, theme));
            }
        }
        col
    }

    fn view_switcher<'a>(&self, theme: &Theme) -> Element<'a, Message> {
        let seg = |label: &'static str, v: SessionView| {
            let active = self.view == v;
            let primary = theme.palette().primary;
            let txt = theme.palette().text;
            button(text(label).size(th::TEXT_META))
                .padding(Padding {
                    top: 4.0,
                    right: 10.0,
                    bottom: 4.0,
                    left: 10.0,
                })
                .on_press(Message::Sessions(SessionMsg::SetView(v)))
                .style(move |_t, status| {
                    let hovered =
                        matches!(status, button::Status::Hovered | button::Status::Pressed);
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
                        border: border::rounded(7),
                        shadow: Shadow::default(),
                        snap: true,
                    }
                })
        };
        let surface = th::surface_strong(theme);
        container(
            row![
                seg("Board", SessionView::Board),
                seg("Review", SessionView::Review),
                seg("Tickets", SessionView::Tickets),
            ]
            .spacing(2),
        )
        .padding(2)
        .style(move |_| container::Style {
            background: Some(Background::Color(surface)),
            border: border::rounded(9),
            ..container::Style::default()
        })
        .into()
    }

    fn ticket_filter<'a>(&self, _theme: &Theme) -> Element<'a, Message> {
        let mut options: Vec<String> = vec!["All".to_string()];
        options.extend(
            ProjectStatus::PIPELINE
                .iter()
                .map(|s| s.label().to_string()),
        );
        let selected = Some(self.ticket_status.clone().unwrap_or_else(|| "All".into()));
        pick_list(options, selected, |choice| {
            Message::Sessions(SessionMsg::SetTicketStatus(
                (choice != "All").then_some(choice),
            ))
        })
        .text_size(th::TEXT_META)
        .padding(6)
        .width(Fill)
        .into()
    }

    fn bulk_bar<'a>(&self, theme: &Theme) -> Element<'a, Message> {
        let n = self.marked.len();
        let accent = theme.palette().primary;
        let line = row![
            text(format!("{n} selected"))
                .size(th::TEXT_META)
                .width(Fill)
                .style(move |_| text::Style {
                    color: Some(accent)
                }),
            widgets::icon_button(
                icon::TRASH,
                "Delete",
                Message::Sessions(SessionMsg::DeleteMarked),
                theme
            ),
            widgets::icon_button(
                icon::CLOSE,
                "Clear",
                Message::Sessions(SessionMsg::ClearMarks),
                theme
            ),
        ]
        .spacing(6)
        .align_y(Center);
        container(line)
            .padding(Padding {
                top: 6.0,
                right: 8.0,
                bottom: 6.0,
                left: 10.0,
            })
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(th::with_alpha(accent, 0.10))),
                border: border::rounded(9),
                ..container::Style::default()
            })
            .into()
    }

    /// The active-repository control: current repo, a switcher, a path field.
    fn repo_bar<'a>(&'a self, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let recent = self.recent_repos();

        let current: Element<Message> = {
            let label = match &self.repo_root {
                Some(root) => format!("Repository · {}", repo_name(root)),
                None => "No repository selected".to_string(),
            };
            row![
                icon::icon(icon::FOLDER)
                    .size(12)
                    .style(move |_: &Theme| text::Style { color: Some(muted) }),
                text(label)
                    .size(th::TEXT_META)
                    .width(Fill)
                    .style(move |_| text::Style { color: Some(muted) }),
            ]
            .spacing(6)
            .align_y(Center)
            .into()
        };

        let switcher: Option<Element<Message>> = (recent.len() > 1).then(|| {
            let options: Vec<RepoOption> = recent.iter().cloned().map(RepoOption).collect();
            let selected = self.repo_root.clone().map(RepoOption);
            pick_list(options, selected, |opt| {
                Message::Sessions(SessionMsg::SetRepo(opt.0))
            })
            .text_size(th::TEXT_META)
            .padding(6)
            .width(Fill)
            .into()
        });

        // Browsing opens the OS folder picker; the text field stays for typing or
        // pasting an exact path (Enter opens it).
        let open_row = row![
            text_input("/path/to/repo", &self.repo_input)
                .on_input(|s| Message::Sessions(SessionMsg::RepoInputChanged(s)))
                .on_submit(Message::Sessions(SessionMsg::OpenRepo))
                .size(th::TEXT_META)
                .padding(6)
                .width(Fill),
            widgets::icon_button(
                icon::FOLDER_OPEN,
                "Browse…",
                Message::Sessions(SessionMsg::BrowseRepo),
                theme,
            ),
        ]
        .spacing(6)
        .align_y(Center);

        let mut col = column![current].spacing(7);
        if let Some(sw) = switcher {
            col = col.push(sw);
        }
        col = col.push(open_row);

        let bg = th::surface(theme);
        let bdr = th::hairline(theme);
        container(col)
            .padding(9)
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg)),
                border: iced::Border {
                    color: bdr,
                    width: 1.0,
                    radius: th::RADIUS_CARD.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    /// The right pane: setup wizard, new-session form, the selected session, or
    /// a prompt.
    pub fn detail<'a>(&'a self, config: &Config, theme: &Theme) -> Element<'a, Message> {
        if !config.initialized {
            return self.setup_wizard(config, theme);
        }
        if let Some(form) = &self.compose {
            return self.form_view(form, theme);
        }
        match self.selected_session() {
            Some(s) => self.session_detail(s, theme),
            None => self.placeholder(theme),
        }
    }

    fn placeholder<'a>(&'a self, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let msg = if self.repo_root.is_some() {
            "Select a session, or create one to start working in an isolated worktree."
        } else {
            "Open Filament inside a git repository to manage worktree sessions."
        };
        container(
            column![
                icon::icon(icon::SESSIONS)
                    .size(34)
                    .style(move |_: &Theme| text::Style { color: Some(muted) }),
                text(msg)
                    .size(th::TEXT_BODY)
                    .style(move |_| text::Style { color: Some(muted) }),
            ]
            .spacing(14)
            .align_x(Center),
        )
        .center_x(Fill)
        .center_y(Fill)
        .padding(24)
        .into()
    }

    fn setup_wizard<'a>(&'a self, config: &Config, theme: &Theme) -> Element<'a, Message> {
        use filament_core::{CodeProvider, TaskProvider};
        let muted = th::muted(theme);
        let dev_root = config
            .dev_root
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let provider = pick_list(
            CodeProvider::ALL.to_vec(),
            Some(config.default_provider),
            |p| Message::Cfg(crate::app::CfgMsg::SetProvider(p)),
        )
        .padding(8)
        .width(Fill);
        let task = pick_list(
            TaskProvider::ALL.to_vec(),
            Some(config.default_task_provider),
            |p| Message::Cfg(crate::app::CfgMsg::SetTaskProvider(p)),
        )
        .padding(8)
        .width(Fill);

        let body = column![
            text("Welcome to Filament Sessions")
                .size(th::TEXT_H2),
            text("Pick how your work is tracked, then get started. You can change all of this later in Settings.")
                .size(th::TEXT_META)
                .style(move |_| text::Style { color: Some(muted) }),
            labeled("Code backend (PRs / CI)", provider.into(), None, theme),
            labeled("Task backend (issues / tickets)", task.into(), None, theme),
            labeled(
                "Development root (optional)",
                text_input("~/Dev", &dev_root)
                    .on_input(|s| Message::Cfg(crate::app::CfgMsg::SetDevRoot(s)))
                    .padding(8)
                    .into(),
                None,
                theme,
            ),
            labeled(
                "Branch prefix (optional)",
                text_input("feature/", &config.branch_prefix)
                    .on_input(|s| Message::Cfg(crate::app::CfgMsg::SetBranchPrefix(s)))
                    .padding(8)
                    .into(),
                None,
                theme,
            ),
            widgets::primary_button(
                icon::ROCKET,
                "Get started",
                Some(Message::Cfg(crate::app::CfgMsg::MarkInitialized)),
                theme,
            ),
        ]
        .spacing(14)
        .width(Fill);

        widgets::scroll(
            container(widgets::card_titleless(body.into(), theme)).padding(th::PAD_PANE),
            theme,
        )
        .into()
    }

    fn form_view<'a>(&'a self, form: &'a NewForm, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let can_create = self.repo_root.is_some()
            && (!form.title.trim().is_empty() || !form.issue_url.trim().is_empty());

        let base_field: Element<Message> = if self.branches.is_empty() {
            text("No branches found")
                .size(13)
                .style(move |_| text::Style { color: Some(muted) })
                .into()
        } else {
            pick_list(self.branches.clone(), Some(form.base.clone()), |b| {
                Message::Sessions(SessionMsg::FormBase(b))
            })
            .padding(8)
            .width(Fill)
            .into()
        };

        let issue_hint = if self.gh_present {
            "Optional — an issue URL/number/key (resolved via the task backend)"
        } else {
            "Optional — install the provider CLI to link & resolve issues"
        };

        let body = column![
            labeled(
                "Title",
                text_input("Add authentication to the API", &form.title)
                    .on_input(|s| Message::Sessions(SessionMsg::FormTitle(s)))
                    .padding(8)
                    .into(),
                None,
                theme,
            ),
            labeled(
                "Issue",
                text_input(issue_hint, &form.issue_url)
                    .on_input(|s| Message::Sessions(SessionMsg::FormIssue(s)))
                    .padding(8)
                    .into(),
                None,
                theme,
            ),
            labeled("Base branch", base_field, None, theme),
            row![
                widgets::primary_button(
                    icon::ROCKET,
                    "Create session",
                    can_create.then_some(Message::Sessions(SessionMsg::Create)),
                    theme,
                ),
                widgets::icon_button(
                    icon::CLOSE,
                    "Cancel",
                    Message::Sessions(SessionMsg::ToggleNew),
                    theme,
                ),
            ]
            .spacing(8),
        ]
        .spacing(14)
        .width(Fill);

        let mut content = column![text("New session").size(th::TEXT_H2)]
            .spacing(th::GAP_SECTION)
            .width(Fill);
        content = content.push(widgets::card_titleless(body.into(), theme));
        if let Some(err) = &self.error {
            content = content.push(text(err.clone()).size(12).style(move |_| text::Style {
                color: Some(th::danger()),
            }));
        }
        widgets::scroll(container(content).padding(th::PAD_PANE), theme).into()
    }

    fn session_detail<'a>(&'a self, s: &'a Session, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let accent = state_color(s.state, theme);

        // Header: title (editable) + state pill.
        let title_el: Element<Message> = match &self.renaming {
            Some((id, buf)) if id == &s.id => text_input("Session title", buf)
                .on_input(|v| Message::Sessions(SessionMsg::RenameInput(v)))
                .on_submit(Message::Sessions(SessionMsg::RenameCommit))
                .size(th::TEXT_H2)
                .padding(6)
                .width(Fill)
                .into(),
            _ => text(s.title.clone()).size(th::TEXT_H2).width(Fill).into(),
        };
        let rename_controls: Element<Message> =
            if self.renaming.as_ref().is_some_and(|(id, _)| id == &s.id) {
                row![
                    widgets::icon_only(
                        icon::SAVE,
                        Message::Sessions(SessionMsg::RenameCommit),
                        theme
                    ),
                    widgets::icon_only(
                        icon::CLOSE,
                        Message::Sessions(SessionMsg::RenameCancel),
                        theme
                    ),
                ]
                .spacing(4)
                .into()
            } else {
                widgets::icon_only(
                    icon::EDIT,
                    Message::Sessions(SessionMsg::RenameStart(s.id.clone())),
                    theme,
                )
            };
        let header = row![
            icon::icon(icon::BRANCH)
                .size(18)
                .style(move |_: &Theme| text::Style {
                    color: Some(accent)
                }),
            title_el,
            rename_controls,
            widgets::pill(s.state.label(), accent, th::with_alpha(accent, 0.15)),
        ]
        .spacing(10)
        .align_y(Center);

        // Worktree card.
        let exists = s.worktree_exists();
        let mut wt_meta = row![
            widgets::kv_pill("branch", &s.branch, theme),
            widgets::kv_pill("base", &s.base_branch, theme),
            widgets::kv_pill("via", s.provider.label(), theme),
        ]
        .spacing(6);
        if !exists {
            wt_meta = wt_meta.push(widgets::pill(
                "missing",
                th::danger(),
                th::with_alpha(th::danger(), 0.15),
            ));
        }
        let path_line = row![
            icon::icon(icon::FOLDER_OPEN)
                .size(13)
                .style(move |_: &Theme| text::Style { color: Some(muted) }),
            text(s.worktree.display().to_string())
                .size(12)
                .style(move |_| text::Style { color: Some(muted) }),
        ]
        .spacing(6)
        .align_y(Center);

        let actions = row![
            widgets::primary_button(
                icon::RUN,
                "Run Claude",
                exists.then_some(Message::Sessions(SessionMsg::OpenAgent(s.id.clone()))),
                theme,
            ),
            widgets::icon_button(
                icon::TERMINAL,
                "Shell",
                Message::Sessions(SessionMsg::OpenShell(s.id.clone())),
                theme,
            ),
            widgets::icon_button(
                icon::BRANCH,
                "Copy branch",
                Message::Sessions(SessionMsg::CopyBranch(s.branch.clone())),
                theme,
            ),
            space().width(Fill),
            widgets::icon_button(
                icon::TRASH,
                "Remove",
                Message::Sessions(SessionMsg::AskDelete(s.id.clone())),
                theme,
            ),
        ]
        .spacing(8)
        .align_y(Center);

        let worktree_card = widgets::card(
            "Worktree",
            column![wt_meta, path_line, self.status_actions(s, theme), actions]
                .spacing(12)
                .into(),
            theme,
        );

        let mut content = column![header, worktree_card]
            .spacing(th::GAP_SECTION)
            .width(Fill);

        // Delete confirmation.
        if self.confirming.as_deref() == Some(&s.id) {
            content = content.push(self.confirm_card(s, theme));
        }

        // Issue card.
        if let Some(issue) = &s.issue {
            content = content.push(widgets::card(
                "Issue",
                ref_row(
                    icon::ISSUE,
                    issue.number,
                    &issue.title,
                    &issue.state,
                    &issue.url,
                    theme,
                ),
                theme,
            ));
        }

        // Pull request card.
        if let Some(pr) = &s.pr {
            content = content.push(widgets::card(
                "Pull request",
                self.pr_card(pr, theme),
                theme,
            ));
        } else if self.gh == GhStatus::Ready {
            let hint = if s.pr_suggested {
                "This session has work but no PR. Open one, then Refresh."
            } else {
                "No pull request yet. Push the branch and open one, then Refresh."
            };
            content = content.push(widgets::card(
                "Pull request",
                column![
                    text(hint)
                        .size(13)
                        .style(move |_| text::Style { color: Some(muted) }),
                    widgets::icon_button(
                        icon::PR,
                        "Open PR",
                        Message::Sessions(SessionMsg::CreatePr(s.id.clone())),
                        theme,
                    ),
                ]
                .spacing(10)
                .into(),
                theme,
            ));
        }

        // Links card.
        content = content.push(self.links_card(s, theme));

        if let Some(err) = &self.error {
            content = content.push(text(err.clone()).size(12).style(move |_| text::Style {
                color: Some(th::danger()),
            }));
        }

        widgets::scroll(container(content).padding(th::PAD_PANE), theme).into()
    }

    /// The status quick-action row (mark in review / pause / archive / return).
    fn status_actions<'a>(&self, s: &Session, theme: &Theme) -> Element<'a, Message> {
        let mut r = row![].spacing(8).align_y(Center);
        let id = s.id.clone();
        if s.state != SessionState::Working {
            r = r.push(widgets::icon_button(
                icon::RUN,
                "Set active",
                Message::Sessions(SessionMsg::SetStatus(id.clone(), SessionState::Working)),
                theme,
            ));
        }
        if s.state != SessionState::Review {
            r = r.push(widgets::icon_button(
                icon::PR,
                "Mark in review",
                Message::Sessions(SessionMsg::SetStatus(id.clone(), SessionState::Review)),
                theme,
            ));
        }
        if s.state != SessionState::Paused {
            r = r.push(widgets::icon_button(
                icon::CLOCK,
                "Pause",
                Message::Sessions(SessionMsg::SetStatus(id.clone(), SessionState::Paused)),
                theme,
            ));
        }
        if s.state != SessionState::Archived {
            r = r.push(widgets::icon_button(
                icon::CONFIG,
                "Archive",
                Message::Sessions(SessionMsg::SetStatus(id.clone(), SessionState::Archived)),
                theme,
            ));
        }
        r.into()
    }

    fn confirm_card<'a>(&self, s: &Session, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let body = column![
            text("Remove this session?")
                .size(th::TEXT_BODY),
            text("\"Remove session\" keeps the worktree on disk; \"Delete worktree\" removes it (refused if it has uncommitted changes, and never on a protected branch).")
                .size(th::TEXT_META)
                .style(move |_| text::Style { color: Some(muted) }),
            row![
                widgets::icon_button(
                    icon::CLOSE,
                    "Remove session",
                    Message::Sessions(SessionMsg::RemoveOnly(s.id.clone())),
                    theme,
                ),
                widgets::primary_button(
                    icon::TRASH,
                    "Delete worktree",
                    Some(Message::Sessions(SessionMsg::Delete(s.id.clone()))),
                    theme,
                ),
                space().width(Fill),
                widgets::icon_button(
                    icon::CLOSE,
                    "Cancel",
                    Message::Sessions(SessionMsg::CancelDelete),
                    theme,
                ),
            ]
            .spacing(8)
            .align_y(Center),
        ]
        .spacing(10);
        widgets::card("Confirm", body.into(), theme)
    }

    fn pr_card<'a>(&self, pr: &'a PrRef, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let mut pr_col = column![ref_row(
            icon::PR,
            pr.number,
            &pr.title,
            &pr.state,
            &pr.url,
            theme
        )]
        .spacing(10);
        let mut badges = row![].spacing(6);
        if pr.is_draft {
            badges = badges.push(widgets::pill("draft", muted, th::with_alpha(muted, 0.15)));
        }
        if let Some(rev) = &pr.review_decision {
            let (fg, label) = review_style(rev, theme);
            badges = badges.push(widgets::pill(label, fg, th::with_alpha(fg, 0.15)));
        }
        badges = badges.push(merge_badge(pr.merge_readiness(), theme));
        badges = badges.push(checks_badge(
            pr.checks.passing,
            pr.checks.failing,
            pr.checks.pending,
            theme,
        ));
        pr_col = pr_col.push(badges);
        pr_col.into()
    }

    fn links_card<'a>(&self, s: &Session, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let mut col = column![].spacing(8);
        for (i, link) in s.links.iter().enumerate() {
            col = col.push(
                row![
                    icon::icon(icon::LINK)
                        .size(13)
                        .style(move |_: &Theme| text::Style { color: Some(muted) }),
                    text(link.label.clone()).size(13).width(Fill),
                    widgets::icon_only(
                        icon::LINK,
                        Message::Sessions(SessionMsg::OpenUrl(link.url.clone())),
                        theme
                    ),
                    widgets::icon_only(
                        icon::TRASH,
                        Message::Sessions(SessionMsg::RemoveLink(i)),
                        theme
                    ),
                ]
                .spacing(7)
                .align_y(Center),
            );
        }
        match &self.linking {
            Some(f) if f.session == s.id => {
                col = col.push(
                    column![
                        text_input("Label", &f.label)
                            .on_input(|v| Message::Sessions(SessionMsg::LinkLabel(v)))
                            .padding(7),
                        text_input("https://…", &f.url)
                            .on_input(|v| Message::Sessions(SessionMsg::LinkUrl(v)))
                            .on_submit(Message::Sessions(SessionMsg::LinkCommit))
                            .padding(7),
                        row![
                            widgets::primary_button(
                                icon::SAVE,
                                "Add",
                                (!f.label.trim().is_empty() && !f.url.trim().is_empty())
                                    .then_some(Message::Sessions(SessionMsg::LinkCommit)),
                                theme
                            ),
                            widgets::icon_button(
                                icon::CLOSE,
                                "Cancel",
                                Message::Sessions(SessionMsg::LinkCancel),
                                theme
                            ),
                        ]
                        .spacing(8),
                    ]
                    .spacing(7),
                );
            }
            _ => {
                col = col.push(widgets::icon_button(
                    icon::NEW,
                    "Add link",
                    Message::Sessions(SessionMsg::AddLinkStart(s.id.clone())),
                    theme,
                ));
            }
        }
        widgets::card("Links", col.into(), theme)
    }

    /// A board session row (clickable, with a multi-select checkbox).
    fn session_row<'a>(&self, s: &'a Session, theme: &Theme) -> Element<'a, Message> {
        let base_text = theme.palette().text;
        let muted = th::muted(theme);
        let accent = theme.palette().primary;
        let selected = self.selected.as_deref() == Some(&s.id);
        let selected_bg = th::with_alpha(accent, 0.18);
        let hover_bg = th::with_alpha(base_text, 0.06);

        let mut meta = row![
            icon::icon(icon::BRANCH)
                .size(11)
                .style(move |_: &Theme| text::Style { color: Some(muted) }),
            text(s.branch.clone())
                .size(th::TEXT_META)
                .style(move |_| text::Style { color: Some(muted) }),
        ]
        .spacing(5)
        .align_y(Center);
        if self.showing_all() {
            meta = meta.push(
                text(format!("· {}", repo_name(&s.repo_root)))
                    .size(th::TEXT_META)
                    .style(move |_| text::Style { color: Some(muted) }),
            );
        }
        if let Some(issue) = &s.issue {
            meta = meta.push(
                text(format!("#{}", issue.number))
                    .size(11)
                    .style(move |_| text::Style { color: Some(muted) }),
            );
        }
        if let Some(pr) = &s.pr {
            meta = meta.push(check_dot(pr.checks.overall(), theme));
            meta = meta.push(
                text(format!("PR #{}", pr.number))
                    .size(11)
                    .style(move |_| text::Style { color: Some(muted) }),
            );
        }

        let body = column![
            text(s.title.clone())
                .size(th::TEXT_BODY)
                .style(move |_| text::Style {
                    color: Some(base_text)
                }),
            meta,
        ]
        .spacing(3)
        .width(Fill);

        let mark = checkbox(self.marked.contains(&s.id))
            .on_toggle({
                let id = s.id.clone();
                move |_| Message::Sessions(SessionMsg::ToggleMark(id.clone()))
            })
            .size(15);

        let clickable = button(body)
            .width(Fill)
            .padding(Padding {
                top: 7.0,
                right: 8.0,
                bottom: 7.0,
                left: 9.0,
            })
            .on_press(Message::Sessions(SessionMsg::Select(s.id.clone())))
            .style(move |_t, status| {
                let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
                let background = if selected {
                    Some(Background::Color(selected_bg))
                } else if hovered {
                    Some(Background::Color(hover_bg))
                } else {
                    None
                };
                button::Style {
                    background,
                    text_color: base_text,
                    border: border::rounded(8),
                    shadow: Shadow::default(),
                    snap: true,
                }
            });

        row![mark, clickable].spacing(4).align_y(Center).into()
    }
}

/// The display name of a repository (its directory name, or full path if empty).
fn repo_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

/// Whether two repo paths refer to the same repository (canonicalized).
fn same_repo(a: &std::path::Path, b: &std::path::Path) -> bool {
    let canon = |p: &std::path::Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    canon(a) == canon(b)
}

/// A repository choice for the switcher `pick_list`, shown as `name — parent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoOption(pub PathBuf);

impl std::fmt::Display for RepoOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = repo_name(&self.0);
        match self.0.parent().and_then(|p| p.file_name()) {
            Some(parent) => write!(f, "{name}  ·  {}", parent.to_string_lossy()),
            None => f.write_str(&name),
        }
    }
}

// ---- row / badge helpers ----------------------------------------------------

fn state_color(state: SessionState, theme: &Theme) -> Color {
    match state {
        SessionState::Working => theme.palette().primary,
        SessionState::Review => th::amber(),
        SessionState::Done => Color::from_rgb8(0x9E, 0xCE, 0x6A),
        SessionState::Paused => Color::from_rgb8(0xE0, 0xAF, 0x68),
        SessionState::Archived => th::muted(theme),
    }
}

fn group_header<'a>(
    label: impl Into<String>,
    accent: Color,
    count: usize,
    muted: Color,
) -> Element<'a, Message> {
    let label: String = label.into();
    row![
        widgets::swatch(accent, 8.0),
        text(label.to_uppercase())
            .size(11)
            .style(move |_| text::Style { color: Some(muted) }),
        space().width(Fill),
        text(count.to_string())
            .size(11)
            .style(move |_| text::Style { color: Some(muted) }),
    ]
    .align_y(Center)
    .spacing(7)
    .padding(Padding {
        top: 10.0,
        right: 6.0,
        bottom: 4.0,
        left: 8.0,
    })
    .into()
}

fn ticket_row<'a>(issue: &'a IssueRef, theme: &Theme) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let base_text = theme.palette().text;
    let line = row![
        icon::icon(icon::ISSUE)
            .size(13)
            .style(move |_: &Theme| text::Style { color: Some(muted) }),
        text(format!("#{}", issue.number))
            .size(12)
            .style(move |_| text::Style { color: Some(muted) }),
        text(issue.title.clone())
            .size(13)
            .width(Fill)
            .style(move |_| text::Style {
                color: Some(base_text)
            }),
        widgets::icon_only(
            icon::ROCKET,
            Message::Sessions(SessionMsg::StartIssue(issue.number)),
            theme
        ),
    ]
    .spacing(7)
    .align_y(Center);
    container(line)
        .padding(Padding {
            top: 4.0,
            right: 6.0,
            bottom: 4.0,
            left: 9.0,
        })
        .into()
}

fn review_pr_row<'a>(pr: &'a PrRef, theme: &Theme) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let base_text = theme.palette().text;
    let line = row![
        check_dot(pr.checks.overall(), theme),
        text(format!("#{}", pr.number))
            .size(12)
            .style(move |_| text::Style { color: Some(muted) }),
        text(pr.title.clone())
            .size(13)
            .width(Fill)
            .style(move |_| text::Style {
                color: Some(base_text)
            }),
        widgets::icon_only(
            icon::LINK,
            Message::Sessions(SessionMsg::OpenUrl(pr.url.clone())),
            theme
        ),
        widgets::icon_only(
            icon::ROCKET,
            Message::Sessions(SessionMsg::StartReviewPr(pr.number)),
            theme
        ),
    ]
    .spacing(7)
    .align_y(Center);
    container(line)
        .padding(Padding {
            top: 4.0,
            right: 6.0,
            bottom: 4.0,
            left: 9.0,
        })
        .into()
}

/// One issue/PR reference line: icon, `#num`, title, state, open-in-browser.
fn ref_row<'a>(
    glyph: char,
    number: u64,
    title: &'a str,
    state: &'a str,
    url: &'a str,
    theme: &Theme,
) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let base_text = theme.palette().text;
    row![
        icon::icon(glyph)
            .size(14)
            .style(move |_: &Theme| text::Style { color: Some(muted) }),
        text(format!("#{number}"))
            .size(13)
            .style(move |_| text::Style { color: Some(muted) }),
        text(title.to_string())
            .size(13)
            .width(Fill)
            .style(move |_| text::Style {
                color: Some(base_text)
            }),
        widgets::pill(state.to_string(), muted, th::with_alpha(muted, 0.15)),
        widgets::icon_only(
            icon::LINK,
            Message::Sessions(SessionMsg::OpenUrl(url.to_string())),
            theme
        ),
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

fn check_dot<'a>(state: CheckState, theme: &Theme) -> Element<'a, Message> {
    let (glyph, color) = match state {
        CheckState::Passing => (icon::CHECK_OK, state_color(SessionState::Done, theme)),
        CheckState::Failing => (icon::CHECK_FAIL, th::danger()),
        CheckState::Pending => (icon::CLOCK, th::amber()),
        CheckState::None => return space().width(0.0).into(),
    };
    icon::icon(glyph)
        .size(12)
        .style(move |_: &Theme| text::Style { color: Some(color) })
        .into()
}

fn merge_badge<'a>(readiness: MergeReadiness, theme: &Theme) -> Element<'a, Message> {
    let (label, color) = match readiness {
        MergeReadiness::Mergeable => ("mergeable", state_color(SessionState::Done, theme)),
        MergeReadiness::Conflicting => ("conflicting", th::danger()),
        MergeReadiness::Merged => ("merged", Color::from_rgb8(0x9B, 0x7B, 0xD4)),
        MergeReadiness::Unknown => return space().width(0.0).into(),
    };
    widgets::pill(label, color, th::with_alpha(color, 0.15))
}

fn check_part<'a>(glyph: char, n: u32, color: Color) -> Option<Element<'a, Message>> {
    (n > 0).then(|| {
        row![
            icon::icon(glyph)
                .size(12)
                .style(move |_: &Theme| text::Style { color: Some(color) }),
            text(n.to_string())
                .size(11)
                .style(move |_| text::Style { color: Some(color) }),
        ]
        .spacing(3)
        .align_y(Center)
        .into()
    })
}

fn checks_badge<'a>(
    passing: u32,
    failing: u32,
    pending: u32,
    theme: &Theme,
) -> Element<'a, Message> {
    if passing + failing + pending == 0 {
        let muted = th::muted(theme);
        return widgets::pill("no checks", muted, th::with_alpha(muted, 0.12));
    }
    let green = state_color(SessionState::Done, theme);
    let amber = th::amber();
    let red = th::danger();
    let mut r = row![].spacing(8).align_y(Center);
    for part in [
        check_part(icon::CHECK_OK, passing, green),
        check_part(icon::CLOCK, pending, amber),
        check_part(icon::CHECK_FAIL, failing, red),
    ]
    .into_iter()
    .flatten()
    {
        r = r.push(part);
    }
    let bg = th::surface_strong(theme);
    container(r)
        .padding(Padding {
            top: 2.0,
            right: 8.0,
            bottom: 2.0,
            left: 8.0,
        })
        .style(move |_| container::Style {
            background: Some(Background::Color(bg)),
            border: border::rounded(7),
            ..container::Style::default()
        })
        .into()
}

fn review_style(decision: &str, theme: &Theme) -> (Color, &'static str) {
    match decision {
        "APPROVED" => (state_color(SessionState::Done, theme), "approved"),
        "CHANGES_REQUESTED" => (th::danger(), "changes requested"),
        _ => (th::muted(theme), "review required"),
    }
}

fn banner<'a>(message: &'a str, fg: Color) -> Element<'a, Message> {
    container(
        text(message)
            .size(12)
            .style(move |_| text::Style { color: Some(fg) }),
    )
    .padding(Padding {
        top: 7.0,
        right: 10.0,
        bottom: 7.0,
        left: 10.0,
    })
    .width(Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(th::with_alpha(fg, 0.10))),
        border: border::rounded(9),
        ..container::Style::default()
    })
    .into()
}

fn orphan_banner<'a>(label: String) -> Element<'a, Message> {
    let amber = th::amber();
    let line = row![
        text(label)
            .size(12)
            .width(Fill)
            .style(move |_| text::Style { color: Some(amber) }),
        button(text("Adopt").size(12))
            .padding(Padding {
                top: 3.0,
                right: 10.0,
                bottom: 3.0,
                left: 10.0,
            })
            .on_press(Message::Sessions(SessionMsg::AdoptOrphans))
            .style(move |_t, status| {
                let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: Some(Background::Color(th::with_alpha(
                        amber,
                        if hovered { 0.30 } else { 0.18 },
                    ))),
                    text_color: amber,
                    border: border::rounded(8),
                    shadow: Shadow::default(),
                    snap: true,
                }
            }),
    ]
    .spacing(8)
    .align_y(Center);
    container(line)
        .padding(Padding {
            top: 6.0,
            right: 8.0,
            bottom: 6.0,
            left: 10.0,
        })
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(th::with_alpha(amber, 0.10))),
            border: border::rounded(9),
            ..container::Style::default()
        })
        .into()
}

/// A labeled form field (mirrors the editor's field styling).
fn labeled<'a>(
    label: &'a str,
    input: Element<'a, Message>,
    error: Option<&'a str>,
    theme: &Theme,
) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let danger = th::danger();
    let mut col = column![
        text(label)
            .size(11)
            .style(move |_| text::Style { color: Some(muted) }),
        input,
    ]
    .spacing(4);
    if let Some(err) = error {
        col = col.push(text(err).size(11).style(move |_| text::Style {
            color: Some(danger),
        }));
    }
    col.width(Fill).into()
}
