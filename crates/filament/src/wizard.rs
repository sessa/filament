//! The "new item" wizard: pick a kind + scope, name it, and write a template.

use std::path::{Path, PathBuf};

use iced::widget::{column, pick_list, text, text_input};
use iced::{Color, Element, Fill, Theme};

use filament_core::validate::is_valid_name;
use filament_core::{edit, AgentFrontmatter, CommandFrontmatter, ItemKind, SkillFrontmatter};

use crate::app::Message;
use crate::theme as th;
use crate::widgets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopePick {
    Project,
    User,
}

impl ScopePick {
    pub fn all() -> Vec<ScopePick> {
        vec![ScopePick::Project, ScopePick::User]
    }
}

impl std::fmt::Display for ScopePick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ScopePick::Project => "Project (workspace)",
            ScopePick::User => "User (~/.claude)",
        })
    }
}

#[derive(Debug, Clone)]
pub enum WizardMsg {
    Kind(ItemKind),
    Scope(ScopePick),
    Name(String),
}

pub struct Wizard {
    kind: ItemKind,
    scope: ScopePick,
    name: String,
    pub error: Option<String>,
}

impl Wizard {
    pub fn new() -> Wizard {
        Wizard {
            kind: ItemKind::Agent,
            scope: ScopePick::Project,
            name: String::new(),
            error: None,
        }
    }

    pub fn apply(&mut self, msg: WizardMsg) {
        self.error = None;
        match msg {
            WizardMsg::Kind(k) => self.kind = k,
            WizardMsg::Scope(s) => self.scope = s,
            WizardMsg::Name(n) => self.name = n,
        }
    }

    /// Write the template file, returning the kind + path on success.
    pub fn create(
        &self,
        workspace: Option<&Path>,
        home: Option<&Path>,
    ) -> Result<(ItemKind, PathBuf), String> {
        if !is_valid_name(&self.name) {
            return Err(
                "Name must be lowercase letters, digits, or hyphens, starting with a letter."
                    .into(),
            );
        }
        let base = match self.scope {
            ScopePick::Project => workspace,
            ScopePick::User => home,
        }
        .ok_or_else(|| "No directory available for this scope.".to_string())?;
        let claude = base.join(".claude");

        let (path, template) = match self.kind {
            ItemKind::Agent => (
                claude.join("agents").join(format!("{}.md", self.name)),
                AgentFrontmatter::template(&self.name),
            ),
            ItemKind::Skill => (
                claude.join("skills").join(&self.name).join("SKILL.md"),
                SkillFrontmatter::template(&self.name),
            ),
            ItemKind::Command => (
                claude.join("commands").join(format!("{}.md", self.name)),
                CommandFrontmatter::template(&self.name),
            ),
            other => return Err(format!("{other} items can't be created here.")),
        };

        if path.exists() {
            return Err(format!("Already exists: {}", path.display()));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        edit::atomic_write(&path, &template).map_err(|e| e.to_string())?;
        Ok((self.kind, path))
    }

    pub fn view<'a>(&'a self, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let kinds = vec![ItemKind::Agent, ItemKind::Skill, ItemKind::Command];

        let mut col = column![text("Create a new item").size(20)]
            .spacing(16)
            .width(Fill);

        col = col.push(field(
            "type",
            pick_list(kinds, Some(self.kind), |k| {
                Message::WizardField(WizardMsg::Kind(k))
            })
            .padding(8)
            .width(Fill)
            .into(),
            muted,
        ));
        col = col.push(field(
            "scope",
            pick_list(ScopePick::all(), Some(self.scope), |s| {
                Message::WizardField(WizardMsg::Scope(s))
            })
            .padding(8)
            .width(Fill)
            .into(),
            muted,
        ));
        col = col.push(field(
            "name",
            text_input("my-agent", &self.name)
                .on_input(|n| Message::WizardField(WizardMsg::Name(n)))
                .padding(8)
                .into(),
            muted,
        ));

        if let Some(err) = &self.error {
            let danger = th::danger();
            col = col.push(text(err.clone()).size(12).style(move |_| text::Style {
                color: Some(danger),
            }));
        } else {
            col = col.push(
                text("Use the Create button in the toolbar to write the new file.")
                    .size(12)
                    .style(move |_| text::Style { color: Some(muted) }),
            );
        }

        widgets::card_titleless(col.into(), theme)
    }
}

fn field<'a>(label: &'a str, input: Element<'a, Message>, muted: Color) -> Element<'a, Message> {
    column![
        text(label)
            .size(11)
            .style(move |_| text::Style { color: Some(muted) }),
        input,
    ]
    .spacing(4)
    .width(Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use filament_core::parse::parse_agent;
    use filament_core::Scope;

    #[test]
    fn creates_a_valid_agent_template() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = Wizard::new();
        w.apply(WizardMsg::Name("my-bot".to_string())); // kind=Agent, scope=Project by default

        let (kind, path) = w.create(Some(dir.path()), None).expect("create ok");
        assert_eq!(kind, ItemKind::Agent);
        assert_eq!(path, dir.path().join(".claude/agents/my-bot.md"));
        assert!(path.exists());

        // The written template parses as a valid agent named `my-bot`.
        let entry = parse_agent(&path, Scope::Project, 0);
        assert!(entry.is_valid());
        assert_eq!(entry.name, "my-bot");
    }

    #[test]
    fn rejects_invalid_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = Wizard::new();
        w.apply(WizardMsg::Name("Bad Name".to_string()));
        assert!(w.create(Some(dir.path()), None).is_err());
    }
}
