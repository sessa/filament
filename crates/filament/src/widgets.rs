//! Small reusable building blocks: pills, swatches, cards, buttons.

use iced::widget::{button, column, container, text};
use iced::{border, Background, Border, Color, Element, Length, Padding, Shadow, Theme};

use filament_core::Scope;

use crate::app::Message;
use crate::theme as th;

/// A rounded, tinted label chip.
pub fn pill<'a>(label: impl text::IntoFragment<'a>, fg: Color, bg: Color) -> Element<'a, Message> {
    container(text(label).size(11))
        .padding(Padding {
            top: 2.0,
            right: 8.0,
            bottom: 2.0,
            left: 8.0,
        })
        .style(move |_theme| container::Style {
            background: Some(Background::Color(bg)),
            text_color: Some(fg),
            border: border::rounded(6),
            ..container::Style::default()
        })
        .into()
}

/// A `key value` chip, where the key is muted.
pub fn kv_pill<'a>(key: &'a str, value: &'a str, theme: &Theme) -> Element<'a, Message> {
    // Precompute all colors so the style closures capture owned values only
    // (never a borrow of `theme`).
    let muted = th::muted(theme);
    let fg = theme.palette().text;
    let bg = th::surface(theme);
    let bdr = th::hairline(theme);
    container(
        iced::widget::row![
            text(key)
                .size(11)
                .style(move |_| text::Style { color: Some(muted) }),
            text(value)
                .size(11)
                .style(move |_| text::Style { color: Some(fg) }),
        ]
        .spacing(5),
    )
    .padding(Padding {
        top: 2.0,
        right: 8.0,
        bottom: 2.0,
        left: 8.0,
    })
    .style(move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: bdr,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// A small colored square.
pub fn swatch<'a>(color: Color, size: f32) -> Element<'a, Message> {
    container(text(""))
        .width(size)
        .height(size)
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            border: border::rounded(3),
            ..container::Style::default()
        })
        .into()
}

pub fn scope_pill<'a>(scope: Scope) -> Element<'a, Message> {
    let accent = th::scope_accent(scope);
    pill(scope.label(), accent, th::with_alpha(accent, 0.15))
}

pub fn error_badge<'a>() -> Element<'a, Message> {
    pill("invalid", th::danger(), th::with_alpha(th::danger(), 0.15))
}

/// A titled surface card wrapping arbitrary body content.
pub fn card<'a>(title: &'a str, body: Element<'a, Message>, theme: &Theme) -> Element<'a, Message> {
    let surface = th::surface(theme);
    let bdr = th::hairline(theme);
    let muted = th::muted(theme);
    container(
        column![
            text(title.to_uppercase())
                .size(11)
                .style(move |_| text::Style { color: Some(muted) }),
            body,
        ]
        .spacing(10),
    )
    .padding(14)
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(surface)),
        border: Border {
            color: bdr,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// A secondary/ghost button for toolbar actions.
pub fn secondary_button<'a>(label: &'a str, msg: Message, theme: &Theme) -> Element<'a, Message> {
    let surface = th::surface(theme);
    let txt = theme.palette().text;
    let bdr = th::hairline(theme);
    button(text(label).size(13))
        .padding(Padding {
            top: 6.0,
            right: 12.0,
            bottom: 6.0,
            left: 12.0,
        })
        .on_press(msg)
        .style(move |_theme, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let bg = if hovered {
                th::with_alpha(txt, 0.10)
            } else {
                surface
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: txt,
                border: Border {
                    color: bdr,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .into()
}

/// Lay out chips across multiple rows so they don't overflow horizontally.
pub fn wrapped(items: Vec<Element<'_, Message>>, per_row: usize) -> Element<'_, Message> {
    let per_row = per_row.max(1);
    let mut col = column![].spacing(6);
    let mut current = iced::widget::row![].spacing(6);
    let mut n = 0usize;
    for item in items {
        current = current.push(item);
        n += 1;
        if n.is_multiple_of(per_row) {
            col = col.push(current);
            current = iced::widget::row![].spacing(6);
        }
    }
    if !n.is_multiple_of(per_row) {
        col = col.push(current);
    }
    col.into()
}
