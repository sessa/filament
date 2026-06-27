//! The automation suite — crow's Settings → Automation, as pure decisions.
//!
//! These functions decide *what* automation would do given the current sessions,
//! tickets, and [`Config`]; the app's executor decides *whether* to act and
//! performs the side effects (creating sessions, opening/merging PRs, typing
//! follow-ups into a terminal). Keeping the policy here makes every rule
//! unit-testable without a forge. Each rule is gated by its config toggle, all of
//! which default off except auto-complete (matching crow).

use std::path::Path;

use crate::config::Config;
use crate::session::{IssueRef, PrRef, Session, SessionState, SessionStore};

/// A single thing automation would like to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoAction {
    /// Create a session from an issue carrying the auto-create label.
    CreateSession { issue: IssueRef },
    /// Suggest opening a PR for a session that has work but no PR.
    SuggestPr { session_id: String },
    /// Start a review session for a now-reviewable PR.
    StartReview { session_id: String },
    /// Type a "address the requested changes" follow-up into the session.
    RespondChangesRequested { session_id: String },
    /// Type an "investigate the CI failure" follow-up into the session.
    RespondFailedCi { session_id: String },
    /// Squash-merge a session's approved, green, labeled PR.
    Merge { session_id: String },
    /// Move a session to Done after its PR merged / issue closed.
    Complete { session_id: String },
}

/// A batch of [`AutoAction`]s computed from the current state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutomationPlan {
    pub actions: Vec<AutoAction>,
}

impl AutomationPlan {
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

// ---- individual rules -------------------------------------------------------

/// Whether an issue should spawn a session automatically.
pub fn wants_auto_create(issue: &IssueRef, cfg: &Config) -> bool {
    cfg.automation.auto_create
        && !issue.is_closed()
        && issue.has_label(&cfg.automation.auto_label)
        && !cfg.ticket_excluded(issue.repo.as_deref().unwrap_or(""))
}

/// Whether to suggest opening a PR for `session`.
pub fn wants_suggest_pr(session: &Session, cfg: &Config) -> bool {
    cfg.automation.suggest_pr
        && session.state == SessionState::Working
        && session.pr.is_none()
        && session.has_work_evidence()
        && !session.pr_suggested
}

/// Whether `session` should auto-complete (with positive work evidence).
pub fn wants_complete(session: &Session, cfg: &Config) -> bool {
    cfg.automation.auto_complete
        && session.auto_completed()
        && session.has_work_evidence()
        && session.state != SessionState::Done
}

/// Whether `pr` is squash-merge-ready under auto-merge.
pub fn pr_auto_mergeable(pr: &PrRef, cfg: &Config) -> bool {
    cfg.automation.auto_merge
        && pr.is_open()
        && pr.has_label(&cfg.automation.merge_label)
        && pr.is_approved()
        && pr.is_mergeable()
        && pr.checks.failing == 0
        && pr.checks.pending == 0
}

/// Whether a PR is reviewable (open and not a draft).
pub fn pr_reviewable(pr: &PrRef) -> bool {
    pr.is_open() && !pr.is_draft
}

/// The follow-up instruction typed into a session when a review requests changes.
pub fn respond_changes_prompt() -> &'static str {
    "A reviewer requested changes on this PR. Please read the review comments, address every point, and push the fix."
}

/// The follow-up instruction typed into a session when CI fails.
pub fn respond_ci_prompt() -> &'static str {
    "CI is failing on this PR. Please inspect the failing checks, reproduce locally, fix the cause, and push."
}

// ---- whole-board plan -------------------------------------------------------

/// Compute every automation action implied by the current `store`, the open
/// `issues`, and `cfg`, scoped to `repo`.
pub fn plan(
    store: &SessionStore,
    repo: &Path,
    issues: &[IssueRef],
    cfg: &Config,
) -> AutomationPlan {
    let mut actions = Vec::new();

    // 1) Auto-create from labeled issues that don't already have a session.
    let taken: std::collections::HashSet<u64> = store
        .sessions
        .iter()
        .filter_map(|s| s.issue.as_ref().map(|i| i.number))
        .collect();
    for issue in issues {
        if wants_auto_create(issue, cfg) && !taken.contains(&issue.number) {
            actions.push(AutoAction::CreateSession {
                issue: issue.clone(),
            });
        }
    }

    // 2) Per-session rules, scoped to the active repo.
    for s in store.for_repo(repo) {
        if wants_complete(s, cfg) {
            actions.push(AutoAction::Complete {
                session_id: s.id.clone(),
            });
            continue; // a completing session needs nothing else
        }
        if wants_suggest_pr(s, cfg) {
            actions.push(AutoAction::SuggestPr {
                session_id: s.id.clone(),
            });
        }
        if let Some(pr) = &s.pr {
            if pr_auto_mergeable(pr, cfg) {
                actions.push(AutoAction::Merge {
                    session_id: s.id.clone(),
                });
            }
            if cfg.automation.auto_start_review && pr_reviewable(pr) {
                actions.push(AutoAction::StartReview {
                    session_id: s.id.clone(),
                });
            }
            if cfg.automation.respond_changes_requested && pr.changes_requested() {
                actions.push(AutoAction::RespondChangesRequested {
                    session_id: s.id.clone(),
                });
            }
            if cfg.automation.respond_failed_ci && pr.is_open() && pr.checks.failing > 0 {
                actions.push(AutoAction::RespondFailedCi {
                    session_id: s.id.clone(),
                });
            }
        }
    }

    AutomationPlan { actions }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::CodeProvider;

    fn cfg_all_on() -> Config {
        let mut c = Config::default();
        c.automation.auto_create = true;
        c.automation.suggest_pr = true;
        c.automation.auto_merge = true;
        c.automation.respond_changes_requested = true;
        c.automation.respond_failed_ci = true;
        c
    }

    fn labeled_issue(n: u64, label: &str) -> IssueRef {
        IssueRef {
            number: n,
            title: format!("issue {n}"),
            state: "OPEN".into(),
            labels: vec![label.into()],
            ..IssueRef::default()
        }
    }

    #[test]
    fn auto_create_only_for_labeled_open_unclaimed() {
        let cfg = cfg_all_on();
        let store = SessionStore::default();
        let repo = Path::new("/r");
        let issues = vec![
            labeled_issue(1, "crow:auto"),
            labeled_issue(2, "other"),
            IssueRef {
                number: 3,
                state: "CLOSED".into(),
                labels: vec!["crow:auto".into()],
                ..IssueRef::default()
            },
        ];
        let p = plan(&store, repo, &issues, &cfg);
        assert_eq!(p.actions.len(), 1);
        assert!(matches!(&p.actions[0], AutoAction::CreateSession { issue } if issue.number == 1));
    }

    #[test]
    fn suggest_pr_respects_evidence_and_memory() {
        let cfg = cfg_all_on();
        let s = Session {
            id: "s1".into(),
            repo_root: "/r".into(),
            branch: "feature".into(),
            base_branch: "main".into(),
            provider: CodeProvider::GitHub,
            ..Session::default()
        };
        assert!(wants_suggest_pr(&s, &cfg));
        let mut suggested = s.clone();
        suggested.pr_suggested = true;
        assert!(!wants_suggest_pr(&suggested, &cfg));
        let mut no_evidence = s.clone();
        no_evidence.branch = "main".into();
        assert!(!wants_suggest_pr(&no_evidence, &cfg));
    }

    #[test]
    fn auto_merge_needs_green_approved_labeled() {
        let cfg = cfg_all_on();
        let mut pr = PrRef {
            number: 1,
            state: "OPEN".into(),
            review_decision: Some("APPROVED".into()),
            mergeable: Some("MERGEABLE".into()),
            labels: vec!["crow:merge".into()],
            ..PrRef::default()
        };
        pr.checks.passing = 3;
        assert!(pr_auto_mergeable(&pr, &cfg));
        pr.checks.failing = 1;
        assert!(!pr_auto_mergeable(&pr, &cfg));
        pr.checks.failing = 0;
        pr.labels.clear();
        assert!(!pr_auto_mergeable(&pr, &cfg));
    }

    #[test]
    fn plan_collects_per_session_actions() {
        let cfg = cfg_all_on();
        let mut store = SessionStore::default();
        let mut s = Session {
            id: "s1".into(),
            repo_root: "/r".into(),
            branch: "feature".into(),
            base_branch: "main".into(),
            ..Session::default()
        };
        s.pr = Some(PrRef {
            number: 9,
            state: "OPEN".into(),
            review_decision: Some("CHANGES_REQUESTED".into()),
            ..PrRef::default()
        });
        s.pr.as_mut().unwrap().checks.failing = 2;
        store.sessions.push(s);
        let p = plan(&store, Path::new("/r"), &[], &cfg);
        assert!(p
            .actions
            .iter()
            .any(|a| matches!(a, AutoAction::RespondChangesRequested { .. })));
        assert!(p
            .actions
            .iter()
            .any(|a| matches!(a, AutoAction::RespondFailedCi { .. })));
    }
}
