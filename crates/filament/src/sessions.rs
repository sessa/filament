//! The **Sessions** section — a crow-style worktree workflow.
//!
//! State + views for pairing a git worktree with a Claude Code instance and an
//! optional GitHub issue / pull request. The orchestration (spawning git/gh off
//! the UI thread) lives in [`crate::app`]; this module owns the data the view
//! needs and renders the board sidebar and the session detail / new-session
//! form. All git/gh access degrades gracefully when the tools are absent.

use std::path::PathBuf;

use iced::widget::{button, column, container, row, space, text, text_input};
use iced::{border, Background, Center, Color, Element, Fill, Padding, Shadow, Theme};

use filament_core::{
    git, github, session, CheckState, IssueRef, Session, SessionState, SessionStore, Worktree,
};

use crate::app::Message;
use crate::icon;
use crate::theme as th;
use crate::widgets;

/// Messages emitted by the Sessions section (wrapped in [`Message::Sessions`]).
#[derive(Debug, Clone)]
pub enum SessionMsg {
    Select(String),
    /// Open / close the new-session form.
    ToggleNew,
    FormTitle(String),
    FormIssue(String),
    FormBase(String),
    /// Create from the open form.
    Create,
    /// Create directly from a listed issue ("Start working").
    StartIssue(u64),
    /// A create finished: `Ok(id)` to select, or an error message.
    Created(Result<String, String>),
    /// Launch `claude` in the session's worktree.
    OpenAgent(String),
    /// Launch a plain shell in the session's worktree.
    OpenShell(String),
    /// Remove the session (and its worktree, unless on a protected branch).
    Delete(String),
    Deleted(Result<(), String>),
    /// Re-poll GitHub for PR/CI status and open issues.
    Refresh,
    Refreshed(GhStatus, Vec<IssueRef>),
    /// Adopt all detected orphan worktrees.
    AdoptOrphans,
    Adopted,
    /// Open a URL in the system browser.
    OpenUrl(String),
    /// The repository path input changed.
    RepoInputChanged(String),
    /// Open the repository named in the path input.
    OpenRepo,
    /// Pick a repository folder with the native file dialog.
    BrowseRepo,
    /// Switch the active repository to a known path.
    SetRepo(PathBuf),
}

/// Availability of the GitHub CLI, surfaced as a quiet hint in the UI.
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
                Some("GitHub CLI (gh) not found — issues & PR status disabled")
            }
            GhStatus::NotAuthenticated => Some("Run `gh auth login` to load issues & PR status"),
            GhStatus::Error(_) => Some("Couldn't reach GitHub"),
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

/// All state backing the Sessions section.
pub struct SessionsState {
    pub store: SessionStore,
    pub repo_root: Option<PathBuf>,
    pub branches: Vec<String>,
    pub default_branch: Option<String>,
    pub selected: Option<String>,
    pub issues: Vec<IssueRef>,
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
}

impl SessionsState {
    /// Load persisted sessions and probe the repo / gh once at startup.
    ///
    /// `workspace` is resolved to its enclosing git repository root when it sits
    /// inside one, so sessions attach to the repo regardless of which subdir the
    /// app was opened from.
    pub fn load(workspace: Option<PathBuf>, show_all: bool) -> SessionsState {
        // Only resolve to a *real* git repo. Launched from a GUI, the working
        // directory is often `/`, which is not a repo — never treat that as one.
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
        // Prefer a session in the active repo; otherwise the most recent session.
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
            orphans,
            gh: GhStatus::Unknown,
            gh_present,
            compose: None,
            busy: None,
            error: None,
            show_all,
            repo_input,
        }
    }

    /// Switch the active repository and recompute repo-derived state.
    pub fn set_repo(&mut self, repo_root: Option<PathBuf>) {
        self.repo_root = repo_root;
        self.compose = None;
        self.gh = GhStatus::Unknown;
        if let Some(root) = &self.repo_root {
            self.repo_input = root.display().to_string();
        }
        self.reload();
    }

    /// Distinct repositories that have sessions, plus the active one — for the
    /// quick "switch repository" picker.
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
    }

    /// Sessions grouped into board columns. When `show_all` is set (or there is
    /// no active repo), every tracked session is shown so the user can always see
    /// the agents they've had running; otherwise it's filtered to the active repo.
    fn columns(&self) -> Vec<(SessionState, Vec<&Session>)> {
        let all = self.show_all || self.repo_root.is_none();
        let in_scope = |s: &Session| -> bool {
            all || self
                .repo_root
                .as_ref()
                .is_some_and(|root| same_repo(&s.repo_root, root))
        };
        SessionState::ALL
            .iter()
            .map(|state| {
                let items: Vec<&Session> = self
                    .store
                    .sessions
                    .iter()
                    .filter(|s| s.state == *state && in_scope(s))
                    .collect();
                (*state, items)
            })
            .collect()
    }

    /// Whether the board is currently showing sessions across repositories.
    fn showing_all(&self) -> bool {
        self.show_all || self.repo_root.is_none()
    }

    fn selected_session(&self) -> Option<&Session> {
        let id = self.selected.as_ref()?;
        self.store.get(id)
    }

    /// Open issues that don't already have a session (crow's "tickets").
    fn open_tickets(&self) -> Vec<&IssueRef> {
        let taken: std::collections::HashSet<u64> = self
            .store
            .sessions
            .iter()
            .filter_map(|s| s.issue.as_ref().map(|i| i.number))
            .collect();
        self.issues
            .iter()
            .filter(|i| !taken.contains(&i.number))
            .collect()
    }

    // ---- views --------------------------------------------------------------

    /// The left board: new-session action, status hints, and grouped sessions.
    pub fn sidebar<'a>(&'a self, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let mut col = column![].spacing(2);

        if let Some(hint) = self.gh.hint() {
            col = col.push(banner(hint, th::muted(theme)));
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
                col = col.push(session_row(
                    s,
                    self.selected.as_deref() == Some(&s.id),
                    self.showing_all(),
                    theme,
                ));
            }
            col = col.push(space().height(8.0));
        }

        let tickets = self.open_tickets();
        if !tickets.is_empty() {
            col = col.push(group_header("Tickets", muted, tickets.len(), muted));
            for issue in tickets {
                col = col.push(ticket_row(issue, theme));
            }
        }

        if !any && tickets_empty(&self.issues, &self.store) {
            let msg = if self.repo_root.is_some() {
                "No sessions yet. Create one to pair a git worktree with Claude Code."
            } else {
                "No sessions yet. Open a git repository below, then create one to pair a worktree with Claude Code."
            };
            col = col.push(
                text(msg)
                    .size(th::TEXT_BODY)
                    .style(move |_| text::Style { color: Some(muted) }),
            );
        }

        let header = container(
            column![
                widgets::primary_button(
                    icon::NEW,
                    "New session",
                    self.repo_root
                        .is_some()
                        .then_some(Message::Sessions(SessionMsg::ToggleNew)),
                    theme,
                ),
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

    /// The active-repository control: current repo, a switcher across known
    /// repos, and a path field to open another.
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
            iced::widget::pick_list(options, selected, |opt| {
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

    /// The right pane: the new-session form, the selected session, or a prompt.
    pub fn detail<'a>(&'a self, theme: &Theme) -> Element<'a, Message> {
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
            iced::widget::pick_list(self.branches.clone(), Some(form.base.clone()), |b| {
                Message::Sessions(SessionMsg::FormBase(b))
            })
            .padding(8)
            .width(Fill)
            .into()
        };

        let issue_hint = if self.gh_present {
            "Optional — e.g. https://github.com/org/repo/issues/42 (resolved via gh)"
        } else {
            "Optional — install the gh CLI to link & resolve issues"
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

        // Header: title + state pill.
        let header = row![
            icon::icon(icon::BRANCH)
                .size(18)
                .style(move |_: &Theme| text::Style {
                    color: Some(accent)
                }),
            text(s.title.clone()).size(th::TEXT_H2).width(Fill),
            widgets::pill(s.state.label(), accent, th::with_alpha(accent, 0.15)),
        ]
        .spacing(10)
        .align_y(Center);

        // Worktree card.
        let exists = s.worktree_exists();
        let mut wt_meta = row![
            widgets::kv_pill("branch", &s.branch, theme),
            widgets::kv_pill("base", &s.base_branch, theme),
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
            space().width(Fill),
            widgets::icon_button(
                icon::TRASH,
                "Remove",
                Message::Sessions(SessionMsg::Delete(s.id.clone())),
                theme,
            ),
        ]
        .spacing(8)
        .align_y(Center);
        let worktree_card = widgets::card(
            "Worktree",
            column![wt_meta, path_line, actions].spacing(12).into(),
            theme,
        );

        let mut content = column![header, worktree_card]
            .spacing(th::GAP_SECTION)
            .width(Fill);

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
            let mut pr_col = column![ref_row(
                icon::PR,
                pr.number,
                &pr.title,
                &pr.state,
                &pr.url,
                theme,
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
            badges = badges.push(checks_badge(
                pr.checks.passing,
                pr.checks.failing,
                pr.checks.pending,
                theme,
            ));
            pr_col = pr_col.push(badges);
            content = content.push(widgets::card("Pull request", pr_col.into(), theme));
        } else if self.gh == GhStatus::Ready {
            content = content.push(widgets::card(
                "Pull request",
                text("No pull request yet. Push the branch and open one, then Refresh.")
                    .size(13)
                    .style(move |_| text::Style { color: Some(muted) })
                    .into(),
                theme,
            ));
        }

        if let Some(err) = &self.error {
            content = content.push(text(err.clone()).size(12).style(move |_| text::Style {
                color: Some(th::danger()),
            }));
        }

        widgets::scroll(container(content).padding(th::PAD_PANE), theme).into()
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

fn tickets_empty(issues: &[IssueRef], store: &SessionStore) -> bool {
    let taken: std::collections::HashSet<u64> = store
        .sessions
        .iter()
        .filter_map(|s| s.issue.as_ref().map(|i| i.number))
        .collect();
    !issues.iter().any(|i| !taken.contains(&i.number))
}

// ---- row / badge helpers ----------------------------------------------------

fn state_color(state: SessionState, theme: &Theme) -> Color {
    match state {
        SessionState::Working => theme.palette().primary,
        SessionState::Review => th::amber(),
        SessionState::Done => Color::from_rgb8(0x9E, 0xCE, 0x6A),
    }
}

fn group_header<'a>(
    label: &'a str,
    accent: Color,
    count: usize,
    muted: Color,
) -> Element<'a, Message> {
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

fn session_row<'a>(
    s: &'a Session,
    selected: bool,
    show_repo: bool,
    theme: &Theme,
) -> Element<'a, Message> {
    let base_text = theme.palette().text;
    let muted = th::muted(theme);
    let accent = theme.palette().primary;
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
    if show_repo {
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
    .spacing(3);

    button(body)
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
        widgets::pill(state.to_string(), muted, th::with_alpha(muted, 0.15),),
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
