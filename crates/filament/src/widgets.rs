//! Small reusable building blocks: pills, swatches, glass cards, buttons.

use iced::widget::{button, column, container, row, text};
use iced::{border, Background, Border, Center, Color, Element, Length, Padding, Shadow, Theme};

use filament_core::Scope;

use crate::app::Message;
use crate::icon;
use crate::theme as th;

const PAD_PILL: Padding = Padding {
    top: 2.0,
    right: 8.0,
    bottom: 2.0,
    left: 8.0,
};

/// A rounded, tinted label chip.
pub fn pill<'a>(label: impl text::IntoFragment<'a>, fg: Color, bg: Color) -> Element<'a, Message> {
    container(text(label).size(11))
        .padding(PAD_PILL)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(bg)),
            text_color: Some(fg),
            border: border::rounded(7),
            ..container::Style::default()
        })
        .into()
}

/// A `key value` chip, where the key is muted.
pub fn kv_pill<'a>(key: &'a str, value: &'a str, theme: &Theme) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let fg = theme.palette().text;
    let bg = th::surface_strong(theme);
    let bdr = th::hairline(theme);
    container(
        row![
            text(key)
                .size(11)
                .style(move |_| text::Style { color: Some(muted) }),
            text(value)
                .size(11)
                .style(move |_| text::Style { color: Some(fg) }),
        ]
        .spacing(5),
    )
    .padding(PAD_PILL)
    .style(move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: bdr,
            width: 1.0,
            radius: 7.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// A small colored, slightly glowing dot.
pub fn swatch<'a>(color: Color, size: f32) -> Element<'a, Message> {
    container(text(""))
        .width(size)
        .height(size)
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            border: border::rounded(size / 2.0),
            shadow: Shadow {
                color: th::with_alpha(color, 0.6),
                offset: iced::Vector::new(0.0, 0.0),
                blur_radius: 6.0,
            },
            ..container::Style::default()
        })
        .into()
}

pub fn scope_pill<'a>(scope: Scope) -> Element<'a, Message> {
    let accent = th::scope_accent(scope);
    let label = row![
        icon::icon(icon::scope(scope)).size(10),
        text(scope.label()).size(11)
    ]
    .spacing(4);
    container(label)
        .padding(PAD_PILL)
        .style(move |_| container::Style {
            background: Some(Background::Color(th::with_alpha(accent, 0.15))),
            text_color: Some(accent),
            border: border::rounded(7),
            ..container::Style::default()
        })
        .into()
}

pub fn error_badge<'a>() -> Element<'a, Message> {
    let red = th::danger();
    container(row![icon::icon(icon::WARNING).size(10), text("invalid").size(11)].spacing(4))
        .padding(PAD_PILL)
        .style(move |_| container::Style {
            background: Some(Background::Color(th::with_alpha(red, 0.15))),
            text_color: Some(red),
            border: border::rounded(7),
            ..container::Style::default()
        })
        .into()
}

/// A reusable glass-panel container style (raised surface + hairline + shadow).
pub fn panel(theme: &Theme) -> impl Fn(&Theme) -> container::Style {
    let bg = th::surface_strong(theme);
    let bdr = th::hairline(theme);
    let shadow = th::card_shadow();
    move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: bdr,
            width: 1.0,
            radius: 16.0.into(),
        },
        shadow,
        ..container::Style::default()
    }
}

/// A glass card with a title.
pub fn card<'a>(title: &'a str, body: Element<'a, Message>, theme: &Theme) -> Element<'a, Message> {
    let surface = th::surface(theme);
    let bdr = th::hairline(theme);
    let muted = th::muted(theme);
    let shadow = th::card_shadow();
    container(
        column![
            text(title.to_uppercase())
                .size(11)
                .style(move |_| text::Style { color: Some(muted) }),
            body,
        ]
        .spacing(10),
    )
    .padding(16)
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(surface)),
        border: Border {
            color: bdr,
            width: 1.0,
            radius: 14.0.into(),
        },
        shadow,
        ..container::Style::default()
    })
    .into()
}

/// A glass card with no title, wrapping arbitrary content.
pub fn card_titleless<'a>(body: Element<'a, Message>, theme: &Theme) -> Element<'a, Message> {
    let surface = th::surface(theme);
    let bdr = th::hairline(theme);
    let shadow = th::card_shadow();
    container(body)
        .padding(18)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                color: bdr,
                width: 1.0,
                radius: 14.0.into(),
            },
            shadow,
            ..container::Style::default()
        })
        .into()
}

// ---- buttons ----------------------------------------------------------------

const PAD_BTN: Padding = Padding {
    top: 6.0,
    right: 12.0,
    bottom: 6.0,
    left: 12.0,
};

/// The shared "ghost" (subtle translucent) button style.
fn ghost(theme: &Theme) -> impl Fn(&Theme, button::Status) -> button::Style {
    let surface = th::surface_strong(theme);
    let txt = theme.palette().text;
    let bdr = th::hairline(theme);
    move |_t, status| {
        let bg = match status {
            button::Status::Hovered | button::Status::Pressed => th::with_alpha(txt, 0.13),
            _ => surface,
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: txt,
            border: Border {
                color: bdr,
                width: 1.0,
                radius: 9.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

/// A ghost button with a leading icon.
pub fn icon_button<'a>(
    glyph: char,
    label: &'a str,
    msg: Message,
    theme: &Theme,
) -> Element<'a, Message> {
    button(
        row![icon::icon(glyph).size(14), text(label).size(13)]
            .spacing(7)
            .align_y(Center),
    )
    .padding(PAD_BTN)
    .on_press(msg)
    .style(ghost(theme))
    .into()
}

/// A compact icon-only ghost button.
pub fn icon_only<'a>(glyph: char, msg: Message, theme: &Theme) -> Element<'a, Message> {
    button(icon::icon(glyph).size(15))
        .padding(Padding {
            top: 6.0,
            right: 8.0,
            bottom: 6.0,
            left: 8.0,
        })
        .on_press(msg)
        .style(ghost(theme))
        .into()
}

/// A primary (accent) button with a leading icon. `on_press == None` disables it.
pub fn primary_button<'a>(
    glyph: char,
    label: &'a str,
    on_press: Option<Message>,
    theme: &Theme,
) -> Element<'a, Message> {
    let primary = theme.palette().primary;
    let shadow = th::soft_shadow();
    button(
        row![icon::icon(glyph).size(14), text(label).size(13)]
            .spacing(7)
            .align_y(Center),
    )
    .padding(PAD_BTN)
    .on_press_maybe(on_press)
    .style(move |_theme, status| {
        let bg = match status {
            button::Status::Disabled => th::with_alpha(primary, 0.30),
            button::Status::Hovered | button::Status::Pressed => th::with_alpha(primary, 0.85),
            button::Status::Active => primary,
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: Color::WHITE,
            border: border::rounded(10),
            shadow: if matches!(status, button::Status::Disabled) {
                Shadow::default()
            } else {
                shadow
            },
            snap: true,
        }
    })
    .into()
}

/// Lay out chips across multiple rows so they don't overflow horizontally.
pub fn wrapped(items: Vec<Element<'_, Message>>, per_row: usize) -> Element<'_, Message> {
    let per_row = per_row.max(1);
    let mut col = column![].spacing(6);
    let mut current = row![].spacing(6);
    let mut n = 0usize;
    for item in items {
        current = current.push(item);
        n += 1;
        if n.is_multiple_of(per_row) {
            col = col.push(current);
            current = row![].spacing(6);
        }
    }
    if !n.is_multiple_of(per_row) {
        col = col.push(current);
    }
    col.into()
}
