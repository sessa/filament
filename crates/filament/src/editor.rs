//! Editing: a typed form for agents plus a raw source editor for every other
//! kind. Saves are lossless — for the agent form we apply compare-and-set on the
//! original file text so untouched keys, comments, and unknown fields survive
//! verbatim; the source editor writes exactly what's in the buffer.

use iced::widget::{column, row, text, text_editor, text_input, toggler};
use iced::{Element, Fill, Length, Theme};

use filament_core::frontmatter::split_frontmatter;
use filament_core::validate::is_valid_name;
use filament_core::{
    edit, AgentColor, AgentFrontmatter, Effort, Entry, ItemId, Memory, ModelChoice, Payload,
    PermissionMode,
};

use crate::app::Message;
use crate::theme as th;

// ---- messages ---------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum FieldMsg {
    Name(String),
    Description(String),
    Model(ModelChoice),
    Color(ColorPick),
    Effort(EffortPick),
    Permission(PermPick),
    Memory(MemoryPick),
    Background(bool),
    Tools(String),
    Disallowed(String),
}

// ---- optional-value pick_list wrappers --------------------------------------
//
// pick_list needs `T: ToString + PartialEq + Clone`, so each optional enum field
// gets a small wrapper with an explicit "unset" variant.

macro_rules! opt_pick {
    ($name:ident, $inner:ty, [$($variant:expr),* $(,)?]) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            Unset,
            Set($inner),
        }
        impl $name {
            pub fn all() -> Vec<$name> {
                let mut v = vec![$name::Unset];
                $(v.push($name::Set($variant));)*
                v
            }
            pub fn from_opt(o: Option<$inner>) -> $name {
                match o {
                    Some(x) => $name::Set(x),
                    None => $name::Unset,
                }
            }
            pub fn to_opt(self) -> Option<$inner> {
                match self {
                    $name::Unset => None,
                    $name::Set(x) => Some(x),
                }
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $name::Unset => f.write_str("—"),
                    $name::Set(x) => write!(f, "{x}"),
                }
            }
        }
    };
}

opt_pick!(
    ColorPick,
    AgentColor,
    [
        AgentColor::Red,
        AgentColor::Blue,
        AgentColor::Green,
        AgentColor::Yellow,
        AgentColor::Purple,
        AgentColor::Orange,
        AgentColor::Pink,
        AgentColor::Cyan,
    ]
);
opt_pick!(
    EffortPick,
    Effort,
    [
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::Xhigh,
        Effort::Max,
    ]
);
opt_pick!(
    PermPick,
    PermissionMode,
    [
        PermissionMode::Default,
        PermissionMode::AcceptEdits,
        PermissionMode::Auto,
        PermissionMode::DontAsk,
        PermissionMode::BypassPermissions,
        PermissionMode::Plan,
    ]
);
opt_pick!(
    MemoryPick,
    Memory,
    [Memory::User, Memory::Project, Memory::Local]
);

// ---- agent form -------------------------------------------------------------

pub struct AgentEdit {
    pub id: ItemId,
    pub path: std::path::PathBuf,
    original_raw: String,
    original_fm: AgentFrontmatter,
    original_body: String,

    name: String,
    description: String,
    model: ModelChoice,
    color: Option<AgentColor>,
    effort: Option<Effort>,
    permission_mode: Option<PermissionMode>,
    memory: Option<Memory>,
    background: bool,
    tools: String,
    disallowed_tools: String,
    body: text_editor::Content,

    pub status: Option<String>,
}

impl AgentEdit {
    pub fn new(entry: &Entry) -> Option<AgentEdit> {
        let Payload::Agent(fm) = &entry.payload else {
            return None;
        };
        let raw = entry.raw.as_ref()?;
        Some(AgentEdit {
            id: entry.id.clone(),
            path: entry.source_path.clone(),
            original_raw: raw.raw_text.clone(),
            original_fm: fm.clone(),
            original_body: raw.body_str().to_string(),
            name: fm.name.clone(),
            description: fm.description.clone(),
            model: fm.model.clone(),
            color: fm.color,
            effort: fm.effort,
            permission_mode: fm.permission_mode,
            memory: fm.memory,
            background: fm.background.unwrap_or(false),
            tools: fm.tools.as_ref().map(|t| t.to_inline()).unwrap_or_default(),
            disallowed_tools: fm
                .disallowed_tools
                .as_ref()
                .map(|t| t.to_inline())
                .unwrap_or_default(),
            body: text_editor::Content::with_text(raw.body_str()),
            status: None,
        })
    }

    pub fn apply(&mut self, msg: FieldMsg) {
        self.status = None;
        match msg {
            FieldMsg::Name(v) => self.name = v,
            FieldMsg::Description(v) => self.description = v,
            FieldMsg::Model(v) => self.model = v,
            FieldMsg::Color(v) => self.color = v.to_opt(),
            FieldMsg::Effort(v) => self.effort = v.to_opt(),
            FieldMsg::Permission(v) => self.permission_mode = v.to_opt(),
            FieldMsg::Memory(v) => self.memory = v.to_opt(),
            FieldMsg::Background(v) => self.background = v,
            FieldMsg::Tools(v) => self.tools = v,
            FieldMsg::Disallowed(v) => self.disallowed_tools = v,
        }
    }

    pub fn body_action(&mut self, action: text_editor::Action) {
        self.body.perform(action);
    }

    pub fn is_valid(&self) -> bool {
        is_valid_name(&self.name) && !self.description.trim().is_empty()
    }

    /// Build the new file text by applying only the fields that changed.
    pub fn build_text(&self) -> String {
        let mut raw = self.original_raw.clone();

        let set = |raw: &mut String, key: &str, value: &str| {
            let span = split_frontmatter(raw).frontmatter;
            *raw = edit::set_scalar(raw, span, key, value);
        };
        let remove = |raw: &mut String, key: &str| {
            let span = split_frontmatter(raw).frontmatter;
            *raw = edit::remove_key(raw, span, key);
        };

        if self.name != self.original_fm.name {
            set(&mut raw, "name", &self.name);
        }
        if self.description != self.original_fm.description {
            set(&mut raw, "description", &self.description);
        }
        if self.model != self.original_fm.model {
            set(&mut raw, "model", self.model.as_str());
        }
        apply_opt(
            &mut raw,
            "color",
            self.color,
            self.original_fm.color,
            &set,
            &remove,
            |c| c.as_str().to_string(),
        );
        apply_opt(
            &mut raw,
            "effort",
            self.effort,
            self.original_fm.effort,
            &set,
            &remove,
            |e| e.as_str().to_string(),
        );
        apply_opt(
            &mut raw,
            "permissionMode",
            self.permission_mode,
            self.original_fm.permission_mode,
            &set,
            &remove,
            |p| p.as_str().to_string(),
        );
        apply_opt(
            &mut raw,
            "memory",
            self.memory,
            self.original_fm.memory,
            &set,
            &remove,
            |m| m.as_str().to_string(),
        );

        let original_bg = self.original_fm.background.unwrap_or(false);
        if self.background != original_bg {
            if self.background {
                set(&mut raw, "background", "true");
            } else {
                remove(&mut raw, "background");
            }
        }

        apply_tools(
            &mut raw,
            "tools",
            &self.tools,
            &self.original_fm.tools,
            &set,
            &remove,
        );
        apply_tools(
            &mut raw,
            "disallowedTools",
            &self.disallowed_tools,
            &self.original_fm.disallowed_tools,
            &set,
            &remove,
        );

        let new_body = self.body.text();
        if new_body != self.original_body {
            let span = split_frontmatter(&raw).body;
            raw = edit::replace_body(&raw, span, &new_body);
        }

        raw
    }

    pub fn view<'a>(&'a self, theme: &Theme) -> Element<'a, Message> {
        let name_err = if self.name.trim().is_empty() {
            Some("Name is required.")
        } else if !is_valid_name(&self.name) {
            Some("Lowercase letters, digits, hyphens; must start with a letter.")
        } else {
            None
        };
        let desc_err = self
            .description
            .trim()
            .is_empty()
            .then_some("Description is required.");

        let form = column![
            field(
                "name",
                text_input("agent-name", &self.name)
                    .on_input(|s| Message::EditField(FieldMsg::Name(s)))
                    .padding(8)
                    .into(),
                name_err,
                theme,
            ),
            field(
                "description",
                text_input(
                    "When Claude should delegate to this agent",
                    &self.description
                )
                .on_input(|s| Message::EditField(FieldMsg::Description(s)))
                .padding(8)
                .into(),
                desc_err,
                theme,
            ),
            row![
                field("model", model_pick(&self.model), None, theme),
                field("color", color_pick(self.color), None, theme),
            ]
            .spacing(12),
            row![
                field("effort", effort_pick(self.effort), None, theme),
                field("permission", perm_pick(self.permission_mode), None, theme),
            ]
            .spacing(12),
            row![
                field("memory", memory_pick(self.memory), None, theme),
                field(
                    "background",
                    toggler(self.background)
                        .on_toggle(|b| Message::EditField(FieldMsg::Background(b)))
                        .into(),
                    None,
                    theme,
                ),
            ]
            .spacing(12),
            field(
                "tools (comma-separated)",
                text_input("Read, Grep, mcp__github__*, Agent(debugger)", &self.tools)
                    .on_input(|s| Message::EditField(FieldMsg::Tools(s)))
                    .padding(8)
                    .into(),
                None,
                theme,
            ),
            field(
                "disallowed tools",
                text_input("Write, Edit", &self.disallowed_tools)
                    .on_input(|s| Message::EditField(FieldMsg::Disallowed(s)))
                    .padding(8)
                    .into(),
                None,
                theme,
            ),
            field(
                "system prompt",
                text_editor(&self.body)
                    .on_action(Message::BodyAction)
                    .padding(10)
                    .height(Length::Fixed(280.0))
                    .into(),
                None,
                theme,
            ),
        ]
        .spacing(14)
        .width(Fill);

        let mut content = column![editor_title(
            &format!("Editing agent · {}", self.id_name()),
            theme
        )]
        .spacing(16)
        .width(Fill);
        content = content.push(form);
        if let Some(s) = &self.status {
            content = content.push(status_line(s, theme));
        }
        content.into()
    }

    fn id_name(&self) -> String {
        self.name.clone()
    }
}

// ---- source editor ----------------------------------------------------------

pub struct SourceEdit {
    pub id: ItemId,
    pub path: std::path::PathBuf,
    content: text_editor::Content,
    pub status: Option<String>,
}

impl SourceEdit {
    pub fn new(id: ItemId, path: std::path::PathBuf, text: &str) -> SourceEdit {
        SourceEdit {
            id,
            path,
            content: text_editor::Content::with_text(text),
            status: None,
        }
    }

    pub fn body_action(&mut self, action: text_editor::Action) {
        self.status = None;
        self.content.perform(action);
    }

    pub fn text(&self) -> String {
        self.content.text()
    }

    pub fn view<'a>(&'a self, theme: &Theme) -> Element<'a, Message> {
        let title = format!("Editing source · {}", self.path.display());
        let mut content = column![editor_title(&title, theme)].spacing(12).width(Fill);
        content = content.push(
            text_editor(&self.content)
                .on_action(Message::BodyAction)
                .padding(12)
                .height(Fill),
        );
        if let Some(s) = &self.status {
            content = content.push(status_line(s, theme));
        }
        content.height(Fill).into()
    }
}

// ---- helpers ----------------------------------------------------------------

#[allow(clippy::type_complexity)]
fn apply_opt<T: PartialEq + Copy>(
    raw: &mut String,
    key: &str,
    current: Option<T>,
    original: Option<T>,
    set: &dyn Fn(&mut String, &str, &str),
    remove: &dyn Fn(&mut String, &str),
    to_str: impl Fn(T) -> String,
) {
    if current == original {
        return;
    }
    match current {
        Some(v) => set(raw, key, &to_str(v)),
        None => remove(raw, key),
    }
}

fn apply_tools(
    raw: &mut String,
    key: &str,
    current: &str,
    original: &Option<filament_core::ToolList>,
    set: &dyn Fn(&mut String, &str, &str),
    remove: &dyn Fn(&mut String, &str),
) {
    let original_str = original.as_ref().map(|t| t.to_inline()).unwrap_or_default();
    let current = current.trim();
    if current == original_str {
        return;
    }
    if current.is_empty() {
        remove(raw, key);
    } else {
        set(raw, key, current);
    }
}

fn field<'a>(
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

fn editor_title<'a>(title: &str, theme: &Theme) -> Element<'a, Message> {
    let muted = th::muted(theme);
    text(title.to_string())
        .size(15)
        .style(move |_| text::Style { color: Some(muted) })
        .into()
}

fn status_line<'a>(status: &str, theme: &Theme) -> Element<'a, Message> {
    let danger = th::danger();
    let _ = theme;
    text(status.to_string())
        .size(12)
        .style(move |_| text::Style {
            color: Some(danger),
        })
        .into()
}

fn model_pick(current: &ModelChoice) -> Element<'_, Message> {
    iced::widget::pick_list(ModelChoice::ALIASES.to_vec(), Some(current.clone()), |m| {
        Message::EditField(FieldMsg::Model(m))
    })
    .padding(8)
    .width(Fill)
    .into()
}

fn color_pick<'a>(current: Option<AgentColor>) -> Element<'a, Message> {
    iced::widget::pick_list(ColorPick::all(), Some(ColorPick::from_opt(current)), |v| {
        Message::EditField(FieldMsg::Color(v))
    })
    .padding(8)
    .width(Fill)
    .into()
}

fn effort_pick<'a>(current: Option<Effort>) -> Element<'a, Message> {
    iced::widget::pick_list(
        EffortPick::all(),
        Some(EffortPick::from_opt(current)),
        |v| Message::EditField(FieldMsg::Effort(v)),
    )
    .padding(8)
    .width(Fill)
    .into()
}

fn perm_pick<'a>(current: Option<PermissionMode>) -> Element<'a, Message> {
    iced::widget::pick_list(PermPick::all(), Some(PermPick::from_opt(current)), |v| {
        Message::EditField(FieldMsg::Permission(v))
    })
    .padding(8)
    .width(Fill)
    .into()
}

fn memory_pick<'a>(current: Option<Memory>) -> Element<'a, Message> {
    iced::widget::pick_list(
        MemoryPick::all(),
        Some(MemoryPick::from_opt(current)),
        |v| Message::EditField(FieldMsg::Memory(v)),
    )
    .padding(8)
    .width(Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use filament_core::parse::parse_agent;
    use filament_core::Scope;

    #[test]
    fn build_text_applies_only_changed_fields_losslessly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.md");
        std::fs::write(
            &path,
            "---\nname: t\ndescription: desc\nmodel: sonnet\ncolor: green\ntools: Read, Grep\neffort: high\n---\nBody line one.\nBody line two.\n",
        )
        .unwrap();

        let entry = parse_agent(&path, Scope::Project, 0);
        let mut edit = AgentEdit::new(&entry).expect("agent edit");

        edit.apply(FieldMsg::Model(ModelChoice::Opus));
        edit.apply(FieldMsg::Color(ColorPick::Unset));
        edit.apply(FieldMsg::Tools("Read, Bash".to_string()));

        let out = edit.build_text();

        // Changed fields applied.
        assert!(out.contains("model: opus"), "{out}");
        assert!(!out.contains("model: sonnet"));
        assert!(out.contains("tools: Read, Bash"));
        // Color removed.
        assert!(!out.contains("color:"), "{out}");
        // Untouched fields and body preserved.
        assert!(out.contains("name: t"));
        assert!(out.contains("description: desc"));
        assert!(out.contains("effort: high"));
        assert!(out.contains("Body line one."));
        assert!(out.contains("Body line two."));
    }
}
