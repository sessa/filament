//! Application state, messages, and the root view/update.

use std::collections::HashMap;

use iced::widget::{column, container, markdown, row, rule, scrollable, space, text, text_editor};
use iced::{Center, Element, Fill, Length, Padding, Subscription, Task, Theme};

use filament_core::{Entry, ItemId, ItemKind, Workspace};

use crate::cli::Cli;
use crate::theme as th;
use crate::{editor, inspector, sidebar, watcher, widgets, wizard};

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
    dark: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Select(ItemId),
    LinkClicked(markdown::Uri),
    SearchChanged(String),
    SetKindFilter(Option<ItemKind>),
    ToggleTheme,
    Rescan,
    FsChanged,

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

        let mut app = App {
            workspace,
            selection: None,
            previews: HashMap::new(),
            search: cli.search.clone().unwrap_or_default(),
            kind_filter: None,
            mode: Mode::Inspect,
            dark: true,
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

        (app, Task::none())
    }

    pub fn title(&self) -> String {
        "Filament — Claude Code Config".to_string()
    }

    pub fn theme(&self) -> Theme {
        if self.dark {
            Theme::TokyoNight
        } else {
            Theme::Light
        }
    }

    /// Watch the config locations so external edits refresh the UI live.
    pub fn subscription(&self) -> Subscription<Message> {
        watcher::subscription(self.watch_roots())
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
            Message::ToggleTheme => self.dark = !self.dark,
            Message::Rescan | Message::FsChanged => self.rescan(),

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

    pub fn view(&self) -> Element<'_, Message> {
        let theme = self.theme();

        let detail: Element<Message> = match &self.mode {
            Mode::Inspect => self.detail(),
            Mode::EditAgent(st) => scrollable(container(st.view(&theme)).padding(24))
                .height(Fill)
                .into(),
            Mode::EditSource(st) => container(st.view(&theme)).padding(24).height(Fill).into(),
            Mode::Wizard(w) => scrollable(container(w.view(&theme)).padding(24))
                .height(Fill)
                .into(),
        };

        let body = row![
            container(sidebar::view(
                &self.workspace.catalog,
                self.selection.as_ref(),
                &theme,
                &self.search,
                self.kind_filter,
            ))
            .width(Length::Fixed(320.0))
            .height(Fill),
            rule::vertical(1),
            container(detail).width(Fill).height(Fill),
        ]
        .height(Fill);

        column![self.header(&theme), rule::horizontal(1), body]
            .height(Fill)
            .into()
    }

    fn header<'a>(&'a self, theme: &Theme) -> Element<'a, Message> {
        let muted = th::muted(theme);
        let workspace = self
            .workspace
            .options
            .workspace
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();

        let mut right = row![].spacing(8).align_y(Center);
        match &self.mode {
            Mode::Inspect => {
                let invalid = self.workspace.catalog.invalid_count();
                if invalid > 0 {
                    right = right.push(widgets::pill(
                        format!("{invalid} invalid"),
                        th::danger(),
                        th::with_alpha(th::danger(), 0.15),
                    ));
                }
                right = right.push(widgets::secondary_button("+ New", Message::NewItem, theme));
                right = right.push(widgets::secondary_button("Rescan", Message::Rescan, theme));
                right = right.push(widgets::secondary_button(
                    if self.dark { "Light" } else { "Dark" },
                    Message::ToggleTheme,
                    theme,
                ));
            }
            Mode::EditAgent(st) => {
                let on_save = st.is_valid().then_some(Message::SaveEdit);
                right = right.push(widgets::primary_button("Save", on_save, theme));
                right = right.push(widgets::secondary_button(
                    "Cancel",
                    Message::CancelEdit,
                    theme,
                ));
            }
            Mode::EditSource(_) => {
                right = right.push(widgets::primary_button(
                    "Save",
                    Some(Message::SaveEdit),
                    theme,
                ));
                right = right.push(widgets::secondary_button(
                    "Cancel",
                    Message::CancelEdit,
                    theme,
                ));
            }
            Mode::Wizard(_) => {
                right = right.push(widgets::primary_button(
                    "Create",
                    Some(Message::WizardCreate),
                    theme,
                ));
                right = right.push(widgets::secondary_button(
                    "Cancel",
                    Message::CancelEdit,
                    theme,
                ));
            }
        }

        row![
            text("Filament").size(18),
            space().width(12.0),
            text(workspace)
                .size(12)
                .style(move |_| text::Style { color: Some(muted) }),
            space().width(Fill),
            right,
        ]
        .align_y(Center)
        .padding(Padding {
            top: 12.0,
            right: 16.0,
            bottom: 12.0,
            left: 16.0,
        })
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
                .size(15)
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
