//! The left navigation pane: every config item, grouped by kind, with scope
//! chips, color swatches, and error/shadow indicators.

use iced::widget::{button, column, row, scrollable, space, text};
use iced::{border, Background, Center, Color, Element, Fill, Padding, Shadow, Theme};

use filament_core::{Catalog, Entry, ItemId, ItemKind};

use crate::app::Message;
use crate::theme as th;
use crate::widgets;

pub fn view<'a>(
    catalog: &'a Catalog,
    selected: Option<&'a ItemId>,
    theme: &Theme,
) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let mut col = column![].spacing(2).padding(Padding {
        top: 10.0,
        right: 8.0,
        bottom: 10.0,
        left: 8.0,
    });

    let mut any = false;
    for kind in ItemKind::ALL {
        let entries: Vec<&Entry> = catalog.by_kind(kind).collect();
        if entries.is_empty() {
            continue;
        }
        any = true;
        col = col.push(group_header(kind, entries.len(), muted));
        for entry in entries {
            col = col.push(entry_row(entry, selected == Some(&entry.id), theme));
        }
        col = col.push(space().height(10.0));
    }

    if !any {
        col = col.push(
            text("No configuration found in this workspace.")
                .size(13)
                .style(move |_| text::Style { color: Some(muted) }),
        );
    }

    scrollable(col).height(Fill).width(Fill).into()
}

fn group_header<'a>(kind: ItemKind, count: usize, muted: Color) -> Element<'a, Message> {
    row![
        text(format!(
            "{}  {}",
            th::kind_glyph(kind),
            kind.plural().to_uppercase()
        ))
        .size(11)
        .style(move |_| text::Style { color: Some(muted) }),
        space().width(Fill),
        text(count.to_string())
            .size(11)
            .style(move |_| text::Style { color: Some(muted) }),
    ]
    .align_y(Center)
    .padding(Padding {
        top: 8.0,
        right: 6.0,
        bottom: 2.0,
        left: 6.0,
    })
    .into()
}

fn entry_row<'a>(entry: &'a Entry, selected: bool, theme: &Theme) -> Element<'a, Message> {
    let shadowed = entry.shadowed_by.is_some();
    let base_text = theme.palette().text;
    let name_color = if shadowed {
        th::muted(theme)
    } else {
        base_text
    };
    let selected_bg = th::with_alpha(theme.palette().primary, 0.20);
    let hover_bg = th::with_alpha(base_text, 0.06);

    let mut line = row![].spacing(8).align_y(Center).width(Fill);
    if let Some(c) = entry.color() {
        line = line.push(widgets::swatch(th::agent_color(c), 9.0));
    }
    line = line.push(
        text(entry.name.clone())
            .size(14)
            .width(Fill)
            .style(move |_| text::Style {
                color: Some(name_color),
            }),
    );
    if shadowed {
        line = line.push(widgets::pill(
            "shadowed",
            th::muted(theme),
            th::with_alpha(base_text, 0.08),
        ));
    }
    line = line.push(widgets::scope_pill(entry.scope));
    if !entry.is_valid() {
        line = line.push(widgets::error_badge());
    }

    button(line)
        .width(Fill)
        .padding(Padding {
            top: 7.0,
            right: 8.0,
            bottom: 7.0,
            left: 8.0,
        })
        .on_press(Message::Select(entry.id.clone()))
        .style(move |_theme, status| {
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
                text_color: name_color,
                border: border::rounded(7),
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .into()
}
