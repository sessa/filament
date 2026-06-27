//! The **Settings** section — app preferences (appearance, terminal, sessions).
//!
//! This edits Filament's own preferences (persisted via [`crate::prefs`]), which
//! are distinct from the Claude Code `settings.json` shown in the Config section.

use iced::widget::{button, column, container, pick_list, row, space, text, text_input, toggler};
use iced::{border, Background, Border, Center, Color, Element, Fill, Padding, Shadow, Theme};

use crate::app::Message;
use crate::icon;
use crate::prefs::{
    AccentChoice, Density, PrefMsg, Prefs, ThemeMode, TERM_FONT_MAX, TERM_FONT_MIN,
};
use crate::theme as th;

/// The left pane for Settings: a static legend of the categories.
pub fn sidebar<'a>(theme: &Theme) -> Element<'a, Message> {
    let cats = [
        (icon::PALETTE, "Appearance", "Theme, accent & density"),
        (icon::TERMINAL, "Terminal", "Font size & shell"),
        (icon::SESSIONS, "Sessions", "Board & repositories"),
        (icon::INFO, "About", "Version & data files"),
    ];
    let mut col = column![text("SETTINGS")
        .size(th::TEXT_LABEL)
        .style(legend_title(theme))]
    .spacing(4)
    .padding(Padding {
        top: 14.0,
        right: 10.0,
        bottom: 8.0,
        left: 12.0,
    });
    for (glyph, title, sub) in cats {
        col = col.push(legend_row(glyph, title, sub, theme));
    }
    container(col).height(Fill).into()
}

fn legend_title(theme: &Theme) -> impl Fn(&Theme) -> text::Style {
    let muted = th::muted(theme);
    move |_| text::Style { color: Some(muted) }
}

fn legend_row<'a>(
    glyph: char,
    title: &'a str,
    sub: &'a str,
    theme: &Theme,
) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let accent = theme.palette().primary;
    let fg = theme.palette().text;
    container(
        row![
            icon::icon(glyph)
                .size(15)
                .style(move |_: &Theme| text::Style {
                    color: Some(accent)
                }),
            column![
                text(title)
                    .size(th::TEXT_UI)
                    .style(move |_| text::Style { color: Some(fg) }),
                text(sub)
                    .size(th::TEXT_LABEL)
                    .style(move |_| text::Style { color: Some(muted) }),
            ]
            .spacing(1),
        ]
        .spacing(10)
        .align_y(Center),
    )
    .padding(Padding {
        top: 7.0,
        right: 8.0,
        bottom: 7.0,
        left: 10.0,
    })
    .into()
}

/// The Settings detail pane.
pub fn view<'a>(
    prefs: &'a Prefs,
    theme: &Theme,
    active_repo: Option<&'a std::path::Path>,
) -> Element<'a, Message> {
    let content = column![
        text("Settings").size(th::TEXT_H2),
        appearance_card(prefs, theme),
        terminal_card(prefs, theme),
        sessions_card(prefs, active_repo, theme),
        about_card(prefs, theme),
    ]
    .spacing(th::GAP_SECTION)
    .width(Fill);

    crate::widgets::scroll(container(content).padding(th::PAD_PANE), theme).into()
}

// ---- cards ------------------------------------------------------------------

fn appearance_card<'a>(prefs: &'a Prefs, theme: &Theme) -> Element<'a, Message> {
    let mut theme_seg = row![].spacing(0);
    for mode in ThemeMode::ALL {
        theme_seg = theme_seg.push(segment(
            mode.label(),
            prefs.theme == mode,
            PrefMsg::SetTheme(mode),
            theme,
        ));
    }

    let mut swatches = row![].spacing(8).align_y(Center);
    for accent in AccentChoice::ALL {
        swatches = swatches.push(accent_swatch(
            accent,
            prefs.accent == accent,
            prefs.theme,
            theme,
        ));
    }

    let density = pick_list(Density::ALL.to_vec(), Some(prefs.density), |d| {
        Message::Pref(PrefMsg::SetDensity(d))
    })
    .text_size(th::TEXT_UI)
    .padding(7)
    .width(Fill);

    card(
        icon::PALETTE,
        "Appearance",
        column![
            setting_row("Theme", segmented(theme_seg, theme), theme),
            setting_row("Accent", swatches.into(), theme),
            setting_row("Density", density.into(), theme),
        ]
        .spacing(14)
        .into(),
        theme,
    )
}

fn terminal_card<'a>(prefs: &'a Prefs, theme: &Theme) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let can_dec = prefs.terminal_font_size > TERM_FONT_MIN;
    let can_inc = prefs.terminal_font_size < TERM_FONT_MAX;
    let font_row = row![
        step_button(
            "A−",
            can_dec.then_some(Message::Pref(PrefMsg::TermFontDelta(-1.0))),
            theme
        ),
        text(format!("{:.0} pt", prefs.terminal_font_size))
            .size(th::TEXT_UI)
            .width(iced::Length::Fixed(56.0))
            .align_x(Center),
        step_button(
            "A+",
            can_inc.then_some(Message::Pref(PrefMsg::TermFontDelta(1.0))),
            theme
        ),
    ]
    .spacing(8)
    .align_y(Center);

    let shell = text_input("$SHELL (e.g. /bin/zsh)", &prefs.shell)
        .on_input(|s| Message::Pref(PrefMsg::ShellChanged(s)))
        .size(th::TEXT_UI)
        .padding(7)
        .width(Fill);

    card(
        icon::TERMINAL,
        "Terminal",
        column![
            setting_row("Font size", font_row.into(), theme),
            setting_row("Shell", shell.into(), theme),
            text("Applies to terminals opened from now on.")
                .size(th::TEXT_LABEL)
                .style(move |_| text::Style { color: Some(muted) }),
        ]
        .spacing(12)
        .into(),
        theme,
    )
}

fn sessions_card<'a>(
    prefs: &'a Prefs,
    active_repo: Option<&'a std::path::Path>,
    theme: &Theme,
) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let toggle = toggler(prefs.show_all_sessions)
        .on_toggle(|v| Message::Pref(PrefMsg::ToggleShowAll(v)))
        .size(20);

    let repo_label = match active_repo {
        Some(p) => p.display().to_string(),
        None => "None — open one in the Sessions section".to_string(),
    };

    card(
        icon::SESSIONS,
        "Sessions",
        column![
            setting_row("Show all repositories", toggle.into(), theme),
            text("When on, the board lists every session you've run, not just the active repo's.")
                .size(th::TEXT_LABEL)
                .style(move |_| text::Style { color: Some(muted) }),
            space().height(2.0),
            setting_row(
                "Active repository",
                text(repo_label)
                    .size(th::TEXT_META)
                    .style(move |_| text::Style { color: Some(muted) })
                    .into(),
                theme,
            ),
        ]
        .spacing(10)
        .into(),
        theme,
    )
}

fn about_card<'a>(prefs: &'a Prefs, theme: &Theme) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let mut col = column![
        row![
            text("Filament").size(th::TEXT_BODY),
            text(format!("v{}", env!("CARGO_PKG_VERSION")))
                .size(th::TEXT_META)
                .style(move |_| text::Style { color: Some(muted) }),
        ]
        .spacing(8)
        .align_y(Center),
        text("A desktop dashboard for Claude Code — agents, skills, commands, MCP, settings & worktree sessions.")
            .size(th::TEXT_META)
            .style(move |_| text::Style { color: Some(muted) }),
    ]
    .spacing(8);

    if let Some(path) = &prefs.path {
        col = col.push(
            text(format!("Preferences: {}", path.display()))
                .size(th::TEXT_LABEL)
                .style(move |_| text::Style { color: Some(muted) }),
        );
    }

    card(icon::INFO, "About", col.into(), theme)
}

// ---- building blocks --------------------------------------------------------

/// A card with an icon + title header.
fn card<'a>(
    glyph: char,
    title: &'a str,
    body: Element<'a, Message>,
    theme: &Theme,
) -> Element<'a, Message> {
    let surface = th::surface(theme);
    let bdr = th::hairline(theme);
    let muted = th::muted(theme);
    let accent = theme.palette().primary;
    let head = row![
        icon::icon(glyph)
            .size(13)
            .style(move |_: &Theme| text::Style {
                color: Some(accent)
            }),
        text(title.to_uppercase())
            .size(th::TEXT_LABEL)
            .style(move |_| text::Style { color: Some(muted) }),
    ]
    .spacing(7)
    .align_y(Center);
    container(column![head, body].spacing(14))
        .padding(th::PAD_CARD)
        .width(Fill)
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

/// A labeled setting: muted label on the left, control on the right.
fn setting_row<'a>(
    label: &'a str,
    control: Element<'a, Message>,
    theme: &Theme,
) -> Element<'a, Message> {
    let fg = theme.palette().text;
    row![
        text(label)
            .size(th::TEXT_UI)
            .width(iced::Length::Fixed(168.0))
            .style(move |_| text::Style { color: Some(fg) }),
        container(control).width(Fill),
    ]
    .spacing(12)
    .align_y(Center)
    .into()
}

/// Wrap a row of segments in a pill background.
fn segmented<'a>(inner: iced::widget::Row<'a, Message>, theme: &Theme) -> Element<'a, Message> {
    let surface = th::surface_strong(theme);
    container(inner)
        .padding(2)
        .style(move |_| container::Style {
            background: Some(Background::Color(surface)),
            border: border::rounded(10),
            ..container::Style::default()
        })
        .into()
}

fn segment<'a>(label: &'a str, active: bool, msg: PrefMsg, theme: &Theme) -> Element<'a, Message> {
    let primary = theme.palette().primary;
    let txt = theme.palette().text;
    button(text(label).size(th::TEXT_UI))
        .padding(Padding {
            top: 5.0,
            right: 16.0,
            bottom: 5.0,
            left: 16.0,
        })
        .on_press(Message::Pref(msg))
        .style(move |_t, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let (bg, fg) = if active {
                (th::with_alpha(primary, 0.22), txt)
            } else if hovered {
                (th::with_alpha(txt, 0.10), txt)
            } else {
                (Color::TRANSPARENT, th::with_alpha(txt, 0.7))
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: fg,
                border: border::rounded(8),
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .into()
}

fn accent_swatch<'a>(
    accent: AccentChoice,
    selected: bool,
    mode: ThemeMode,
    theme: &Theme,
) -> Element<'a, Message> {
    let color = th::accent_color(accent, mode.is_dark());
    let ring = theme.palette().text;
    button(space().width(20.0).height(20.0))
        .on_press(Message::Pref(PrefMsg::SetAccent(accent)))
        .padding(0)
        .style(move |_t, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let border = if selected {
                Border {
                    color: ring,
                    width: 2.0,
                    radius: 11.0.into(),
                }
            } else {
                Border {
                    color: th::with_alpha(ring, if hovered { 0.5 } else { 0.0 }),
                    width: 2.0,
                    radius: 11.0.into(),
                }
            };
            button::Style {
                background: Some(Background::Color(color)),
                text_color: Color::WHITE,
                border,
                shadow: Shadow::default(),
                snap: true,
            }
        })
        .into()
}

fn step_button<'a>(label: &'a str, msg: Option<Message>, theme: &Theme) -> Element<'a, Message> {
    let surface = th::surface_strong(theme);
    let txt = theme.palette().text;
    let bdr = th::hairline(theme);
    button(text(label).size(th::TEXT_UI))
        .padding(Padding {
            top: 5.0,
            right: 12.0,
            bottom: 5.0,
            left: 12.0,
        })
        .on_press_maybe(msg)
        .style(move |_t, status| {
            let bg = match status {
                button::Status::Disabled => th::with_alpha(surface, 0.4),
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
        })
        .into()
}
