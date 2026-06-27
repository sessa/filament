//! Small reusable building blocks: pills, swatches, glass cards, buttons.

use iced::widget::{button, column, container, row, scrollable, text};
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
    container(text(label).size(th::TEXT_LABEL))
        .padding(PAD_PILL)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(bg)),
            text_color: Some(fg),
            border: border::rounded(th::RADIUS_CHIP),
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
                .size(th::TEXT_LABEL)
                .style(move |_| text::Style { color: Some(muted) }),
            text(value)
                .size(th::TEXT_LABEL)
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
            radius: th::RADIUS_CHIP.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// A small colored dot.
pub fn swatch<'a>(color: Color, size: f32) -> Element<'a, Message> {
    container(text(""))
        .width(size)
        .height(size)
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            border: border::rounded(size / 2.0),
            ..container::Style::default()
        })
        .into()
}

// ---- scrolling --------------------------------------------------------------

/// A vertically-scrolling region with Filament's thin, translucent overlay
/// scrollbar. The scroller floats in the content's right padding (no track, no
/// reserved gutter), brightens on hover, and tints to the accent while dragged.
pub fn scroll<'a>(
    content: impl Into<Element<'a, Message>>,
    theme: &Theme,
) -> iced::widget::Scrollable<'a, Message> {
    scrollable(content)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(6.0).scroller_width(6.0),
        ))
        .height(Length::Fill)
        .style(scroll_style(theme))
}

fn scroll_style(theme: &Theme) -> impl Fn(&Theme, scrollable::Status) -> scrollable::Style {
    let ink = theme.palette().text;
    let accent = theme.palette().primary;
    move |base_theme, status| {
        let scroller = match status {
            scrollable::Status::Dragged { .. } => th::with_alpha(accent, 0.75),
            scrollable::Status::Hovered {
                is_vertical_scrollbar_hovered: true,
                ..
            } => th::with_alpha(ink, 0.40),
            scrollable::Status::Hovered { .. } => th::with_alpha(ink, 0.22),
            scrollable::Status::Active { .. } => th::with_alpha(ink, 0.18),
        };
        let rail = scrollable::Rail {
            background: None,
            border: border::rounded(3),
            scroller: scrollable::Scroller {
                background: Background::Color(scroller),
                border: border::rounded(3),
            },
        };
        scrollable::Style {
            container: container::Style::default(),
            vertical_rail: rail,
            horizontal_rail: rail,
            gap: None,
            ..scrollable::default(base_theme, status)
        }
    }
}

pub fn scope_pill<'a>(scope: Scope) -> Element<'a, Message> {
    let accent = th::scope_accent(scope);
    let label = row![
        icon::icon(icon::scope(scope)).size(10),
        text(scope.label()).size(th::TEXT_LABEL)
    ]
    .spacing(4);
    container(label)
        .padding(PAD_PILL)
        .style(move |_| container::Style {
            background: Some(Background::Color(th::with_alpha(accent, 0.15))),
            text_color: Some(accent),
            border: border::rounded(th::RADIUS_CHIP),
            ..container::Style::default()
        })
        .into()
}

pub fn error_badge<'a>() -> Element<'a, Message> {
    let red = th::danger();
    container(
        row![
            icon::icon(icon::WARNING).size(10),
            text("invalid").size(th::TEXT_LABEL)
        ]
        .spacing(4),
    )
    .padding(PAD_PILL)
    .style(move |_| container::Style {
        background: Some(Background::Color(th::with_alpha(red, 0.15))),
        text_color: Some(red),
        border: border::rounded(th::RADIUS_CHIP),
        ..container::Style::default()
    })
    .into()
}

/// A reusable glass-panel container style (raised surface + hairline + a single
/// soft shadow that lifts it off the blurred desktop).
pub fn panel(theme: &Theme) -> impl Fn(&Theme) -> container::Style {
    let bg = th::surface_strong(theme);
    let bdr = th::hairline(theme);
    let shadow = th::panel_shadow();
    move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: bdr,
            width: 1.0,
            radius: th::RADIUS_PANEL.into(),
        },
        shadow,
        ..container::Style::default()
    }
}

/// A glass card with a title. Flat (no shadow): it sits inside a panel already.
pub fn card<'a>(title: &'a str, body: Element<'a, Message>, theme: &Theme) -> Element<'a, Message> {
    let surface = th::surface(theme);
    let bdr = th::hairline(theme);
    let muted = th::muted(theme);
    container(
        column![
            text(title.to_uppercase())
                .size(th::TEXT_LABEL)
                .style(move |_| text::Style { color: Some(muted) }),
            body,
        ]
        .spacing(10),
    )
    .padding(th::PAD_CARD)
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(surface)),
        border: Border {
            color: bdr,
            width: 1.0,
            radius: th::RADIUS_CARD.into(),
        },
        ..container::Style::default()
    })
    .into()
}

/// A glass card with no title, wrapping arbitrary content. Flat, like [`card`].
pub fn card_titleless<'a>(body: Element<'a, Message>, theme: &Theme) -> Element<'a, Message> {
    let surface = th::surface(theme);
    let bdr = th::hairline(theme);
    container(body)
        .padding(th::PAD_CARD)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                color: bdr,
                width: 1.0,
                radius: th::RADIUS_CARD.into(),
            },
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
                radius: th::RADIUS_CONTROL.into(),
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
        row![icon::icon(glyph).size(13), text(label).size(th::TEXT_UI)]
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
    button(icon::icon(glyph).size(14))
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
        row![icon::icon(glyph).size(13), text(label).size(th::TEXT_UI)]
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
            border: border::rounded(th::RADIUS_CONTROL),
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
