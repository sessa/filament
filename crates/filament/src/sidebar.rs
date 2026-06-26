//! The left navigation pane: a search box, kind filter chips, and every config
//! item grouped by kind with icons, scope chips, color swatches, and
//! error/shadow indicators.

use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{border, Background, Center, Color, Element, Fill, Padding, Shadow, Theme};

use filament_core::{Catalog, Entry, ItemId, ItemKind};

use crate::app::Message;
use crate::icon;
use crate::search;
use crate::theme as th;
use crate::widgets;

pub fn view<'a>(
    catalog: &'a Catalog,
    selected: Option<&'a ItemId>,
    theme: &Theme,
    query: &'a str,
    kind_filter: Option<ItemKind>,
) -> Element<'a, Message> {
    let top = column![search_bar(query, theme), filters(kind_filter, theme)]
        .spacing(10)
        .padding(Padding {
            top: 12.0,
            right: 10.0,
            bottom: 8.0,
            left: 10.0,
        });

    let list = scrollable(
        container(build_list(catalog, selected, theme, query, kind_filter)).padding(Padding {
            top: 0.0,
            right: 8.0,
            bottom: 12.0,
            left: 8.0,
        }),
    )
    .height(Fill);

    column![top, list].height(Fill).into()
}

fn search_bar<'a>(query: &'a str, theme: &Theme) -> Element<'a, Message> {
    let muted = th::muted(theme);
    row![
        icon::icon(icon::SEARCH)
            .size(13)
            .style(move |_: &Theme| text::Style { color: Some(muted) }),
        text_input("Search…", query)
            .on_input(Message::SearchChanged)
            .size(13)
            .padding(7)
            .width(Fill),
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

fn filters<'a>(active: Option<ItemKind>, theme: &Theme) -> Element<'a, Message> {
    let mut chips: Vec<Element<Message>> = vec![filter_chip(
        "All",
        active.is_none(),
        Message::SetKindFilter(None),
        theme,
    )];
    for kind in ItemKind::ALL {
        chips.push(filter_chip(
            short_label(kind),
            active == Some(kind),
            Message::SetKindFilter(Some(kind)),
            theme,
        ));
    }
    widgets::wrapped(chips, 3)
}

fn short_label(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Agent => "Agents",
        ItemKind::Skill => "Skills",
        ItemKind::Command => "Commands",
        ItemKind::McpServer => "MCP",
        ItemKind::Settings => "Settings",
    }
}

fn filter_chip<'a>(
    label: &'a str,
    active: bool,
    msg: Message,
    theme: &Theme,
) -> Element<'a, Message> {
    let primary = theme.palette().primary;
    let txt = theme.palette().text;
    let surface = th::surface(theme);
    button(text(label).size(12))
        .padding(Padding {
            top: 4.0,
            right: 11.0,
            bottom: 4.0,
            left: 11.0,
        })
        .on_press(msg)
        .style(move |_t, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let (bg, fg) = if active {
                (th::with_alpha(primary, 0.28), txt)
            } else if hovered {
                (th::with_alpha(txt, 0.10), txt)
            } else {
                (surface, th::with_alpha(txt, 0.7))
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: fg,
                border: border::rounded(20),
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .into()
}

fn build_list<'a>(
    catalog: &'a Catalog,
    selected: Option<&'a ItemId>,
    theme: &Theme,
    query: &str,
    kind_filter: Option<ItemKind>,
) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let q_active = !query.trim().is_empty();
    let mut col = column![].spacing(2);
    let mut any = false;

    for kind in ItemKind::ALL {
        if let Some(f) = kind_filter {
            if f != kind {
                continue;
            }
        }
        let mut scored: Vec<(i32, &Entry)> = catalog
            .by_kind(kind)
            .filter_map(|e| search::entry_match(e, query).map(|s| (s, e)))
            .collect();
        if scored.is_empty() {
            continue;
        }
        if q_active {
            scored.sort_by(|a, b| b.0.cmp(&a.0));
        }
        any = true;
        col = col.push(group_header(kind, scored.len(), muted));
        for (_, entry) in scored {
            col = col.push(entry_row(entry, selected == Some(&entry.id), theme));
        }
        col = col.push(space().height(10.0));
    }

    if !any {
        let message = if q_active {
            "No matches."
        } else {
            "No configuration found in this workspace."
        };
        col = col.push(
            text(message)
                .size(13)
                .style(move |_| text::Style { color: Some(muted) }),
        );
    }

    col.into()
}

fn group_header<'a>(kind: ItemKind, count: usize, muted: Color) -> Element<'a, Message> {
    row![
        icon::icon(icon::kind(kind))
            .size(12)
            .style(move |_: &Theme| text::Style { color: Some(muted) }),
        text(kind.plural().to_uppercase())
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

fn entry_row<'a>(entry: &'a Entry, selected: bool, theme: &Theme) -> Element<'a, Message> {
    let shadowed = entry.shadowed_by.is_some();
    let base_text = theme.palette().text;
    let name_color = if shadowed {
        th::muted(theme)
    } else {
        base_text
    };
    let accent = theme.palette().primary;
    let indicator = if selected { accent } else { Color::TRANSPARENT };
    let selected_bg = th::with_alpha(accent, 0.18);
    let hover_bg = th::with_alpha(base_text, 0.06);

    let mut line = row![container(space().width(3.0).height(18.0)).style(move |_| {
        container::Style {
            background: Some(Background::Color(indicator)),
            border: border::rounded(2),
            ..container::Style::default()
        }
    })]
    .spacing(8)
    .align_y(Center)
    .width(Fill);

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
            left: 5.0,
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
                border: border::rounded(8),
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .into()
}
