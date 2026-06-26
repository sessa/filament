//! The **Sessions** section — a crow-style worktree workflow.
//!
//! State + views for pairing a git worktree with a Claude Code instance and an
//! optional GitHub issue / pull request. The orchestration (spawning git/gh off
//! the UI thread) lives in [`crate::app`]; this module owns the data the view
//! needs and renders the board sidebar and the session detail / new-session
//! form. All git/gh access degrades gracefully when the tools are absent.

use std::path::PathBuf;

use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
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
}

impl SessionsState {
    /// Load persisted sessions and probe the repo / gh once at startup.
    ///
    /// `workspace` is resolved to its enclosing git repository root when it sits
    /// inside one, so sessions attach to the repo regardless of which subdir the
    /// app was opened from.
    pub fn load(workspace: Option<PathBuf>) -> SessionsState {
        let repo_root = workspace.and_then(|w| git::repo_root(&w).or(Some(w)));
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
            .and_then(|r| store.for_repo(r).next().map(|s| s.id.clone()));
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
        }
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

    /// Sessions for the active repo, grouped into board columns.
    fn columns(&self) -> Vec<(SessionState, Vec<&Session>)> {
        let Some(root) = &self.repo_root else {
            return SessionState::ALL.iter().map(|s| (*s, Vec::new())).collect();
        };
        SessionState::ALL
            .iter()
            .map(|state| {
                let items: Vec<&Session> = self
                    .store
                    .for_repo(root)
                    .filter(|s| s.state == *state)
                    .collect();
                (*state, items)
            })
            .collect()
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
            col = col.push(
                text("No sessions yet. Create one to pair a git worktree with Claude Code.")
                    .size(13)
                    .style(move |_| text::Style { color: Some(muted) }),
            );
        }

        let header = container(widgets::primary_button(
            icon::NEW,
            "New session",
            self.repo_root
                .is_some()
                .then_some(Message::Sessions(SessionMsg::ToggleNew)),
            theme,
        ))
        .padding(Padding {
            top: 12.0,
            right: 10.0,
            bottom: 8.0,
            left: 10.0,
        });

        let list = scrollable(container(col).padding(Padding {
            top: 0.0,
            right: 8.0,
            bottom: 12.0,
            left: 8.0,
        }))
        .height(Fill);

        column![header, list].height(Fill).into()
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
                    .size(40)
                    .style(move |_: &Theme| text::Style { color: Some(muted) }),
                text(msg)
                    .size(14)
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

        let mut content = column![text("New session").size(16)]
            .spacing(16)
            .width(Fill);
        content = content.push(widgets::card_titleless(body.into(), theme));
        if let Some(err) = &self.error {
            content = content.push(text(err.clone()).size(12).style(move |_| text::Style {
                color: Some(th::danger()),
            }));
        }
        scrollable(container(content).padding(24))
            .height(Fill)
            .into()
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
            text(s.title.clone()).size(20).width(Fill),
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

        let mut content = column![header, worktree_card].spacing(16).width(Fill);

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

        scrollable(container(content).padding(24))
            .height(Fill)
            .into()
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
        SessionState::Review => Color::from_rgb8(0xE0, 0xAF, 0x68),
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

fn session_row<'a>(s: &'a Session, selected: bool, theme: &Theme) -> Element<'a, Message> {
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
            .size(11)
            .style(move |_| text::Style { color: Some(muted) }),
    ]
    .spacing(5)
    .align_y(Center);
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
        text(s.title.clone()).size(14).style(move |_| text::Style {
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
        CheckState::Pending => (icon::CLOCK, Color::from_rgb8(0xE0, 0xAF, 0x68)),
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
    let amber = Color::from_rgb8(0xE0, 0xAF, 0x68);
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
    let amber = Color::from_rgb8(0xE0, 0xAF, 0x68);
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
