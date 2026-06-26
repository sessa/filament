//! Application state, messages, and the root view/update.

use std::collections::HashMap;

use iced::widget::{column, container, markdown, row, rule, space, text};
use iced::{Center, Element, Fill, Length, Padding, Task, Theme};

use filament_core::{Entry, ItemId, ItemKind, Workspace};

use crate::cli::Cli;
use crate::theme as th;
use crate::{inspector, sidebar, widgets};

pub struct App {
    workspace: Workspace,
    selection: Option<ItemId>,
    /// Parsed markdown bodies, cached per item so we don't re-parse every frame.
    previews: HashMap<ItemId, markdown::Content>,
    dark: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Select(ItemId),
    LinkClicked(markdown::Uri),
    ToggleTheme,
    Rescan,
}

impl App {
    pub fn new() -> (App, Task<Message>) {
        let cli = Cli::from_env();
        let workspace = Workspace::load(cli.options());

        let mut app = App {
            workspace,
            selection: None,
            previews: HashMap::new(),
            dark: true,
        };
        app.selection = app
            .workspace
            .catalog
            .by_kind(ItemKind::Agent)
            .find(|e| e.is_valid())
            .map(|e| e.id.clone())
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

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Select(id) => {
                self.selection = Some(id);
                self.ensure_preview();
            }
            Message::LinkClicked(_uri) => {
                // Opening links in the browser arrives in a later milestone.
            }
            Message::ToggleTheme => self.dark = !self.dark,
            Message::Rescan => {
                self.workspace.rescan();
                self.previews.clear();
                // Drop a selection that no longer exists.
                if let Some(id) = &self.selection {
                    if self.workspace.catalog.get(id).is_none() {
                        self.selection = None;
                    }
                }
                self.ensure_preview();
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let theme = self.theme();
        let body = row![
            container(sidebar::view(
                &self.workspace.catalog,
                self.selection.as_ref(),
                &theme
            ))
            .width(Length::Fixed(320.0))
            .height(Fill),
            rule::vertical(1),
            container(self.detail()).width(Fill).height(Fill),
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
        let invalid = self.workspace.catalog.invalid_count();
        if invalid > 0 {
            right = right.push(widgets::pill(
                format!("{invalid} invalid"),
                th::danger(),
                th::with_alpha(th::danger(), 0.15),
            ));
        }
        right = right.push(widgets::secondary_button("Rescan", Message::Rescan, theme));
        right = right.push(widgets::secondary_button(
            if self.dark { "Light" } else { "Dark" },
            Message::ToggleTheme,
            theme,
        ));

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
