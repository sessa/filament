//! The right-hand detail pane. M2 renders agents richly (metadata cards +
//! rendered system prompt) and gives other kinds a basic view; M3 enriches the
//! rest.

use iced::widget::{column, markdown, row, scrollable, space, text};
use iced::{Center, Color, Element, Fill, Theme};

use filament_core::{AgentFrontmatter, Entry, ParseError, Payload, ToolList};

use crate::app::Message;
use crate::theme as th;
use crate::widgets;

pub fn view<'a>(
    entry: &'a Entry,
    preview: Option<&'a markdown::Content>,
    theme: &Theme,
) -> Element<'a, Message> {
    let body = match &entry.payload {
        Payload::Agent(fm) => agent_view(entry, fm, preview, theme),
        Payload::Invalid(err) => invalid_view(entry, err, theme),
        _ => generic_view(entry, preview, theme),
    };
    scrollable(iced::widget::container(body).padding(24))
        .height(Fill)
        .into()
}

fn header<'a>(entry: &'a Entry, theme: &Theme) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let mut left = row![].spacing(10).align_y(Center);
    if let Some(c) = entry.color() {
        left = left.push(widgets::swatch(th::agent_color(c), 16.0));
    }
    left = left.push(text(entry.name.clone()).size(24));
    left = left.push(
        text(entry.kind.label())
            .size(13)
            .style(move |_| text::Style { color: Some(muted) }),
    );

    row![left, space().width(Fill), widgets::scope_pill(entry.scope)]
        .align_y(Center)
        .into()
}

fn agent_view<'a>(
    entry: &'a Entry,
    fm: &'a AgentFrontmatter,
    preview: Option<&'a markdown::Content>,
    theme: &Theme,
) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let mut content = column![].spacing(16).width(Fill);

    content = content.push(header(entry, theme));
    content = content.push(
        text(fm.description.clone())
            .size(14)
            .style(move |_| text::Style { color: Some(muted) }),
    );

    // Quick badges.
    let mut badges: Vec<Element<Message>> = vec![widgets::kv_pill("model", model_label(fm), theme)];
    if let Some(e) = fm.effort {
        badges.push(widgets::kv_pill("effort", e.as_str(), theme));
    }
    if let Some(p) = fm.permission_mode {
        badges.push(widgets::kv_pill("permission", p.as_str(), theme));
    }
    if let Some(m) = fm.memory {
        badges.push(widgets::kv_pill("memory", m.as_str(), theme));
    }
    if fm.background == Some(true) {
        badges.push(widgets::kv_pill("background", "on", theme));
    }
    if let Some(i) = fm.isolation {
        badges.push(widgets::kv_pill("isolation", i.as_str(), theme));
    }
    content = content.push(widgets::wrapped(badges, 5));

    if let Some(tools) = &fm.tools {
        content = content.push(widgets::card("Tools — allowed", tool_chips(tools), theme));
    }
    if let Some(denied) = &fm.disallowed_tools {
        content = content.push(widgets::card("Tools — denied", tool_chips(denied), theme));
    }
    if !fm.skills.is_empty() {
        let chips = fm
            .skills
            .iter()
            .map(|s| {
                widgets::pill(
                    s.clone(),
                    th::agent_color(filament_core::AgentColor::Cyan),
                    th::with_alpha(th::agent_color(filament_core::AgentColor::Cyan), 0.15),
                )
            })
            .collect();
        content = content.push(widgets::card(
            "Preloaded skills",
            widgets::wrapped(chips, 6),
            theme,
        ));
    }

    content = content.push(source_line(entry, theme));

    if let Some(md) = preview {
        content = content.push(widgets::card(
            "System prompt",
            render_markdown(md, theme),
            theme,
        ));
    }

    content.into()
}

fn generic_view<'a>(
    entry: &'a Entry,
    preview: Option<&'a markdown::Content>,
    theme: &Theme,
) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let mut content = column![].spacing(16).width(Fill);
    content = content.push(header(entry, theme));
    if let Some(desc) = entry.description() {
        content = content.push(
            text(desc.to_string())
                .size(14)
                .style(move |_| text::Style { color: Some(muted) }),
        );
    }
    content = content.push(source_line(entry, theme));
    if let Some(md) = preview {
        content = content.push(widgets::card("Contents", render_markdown(md, theme), theme));
    }
    content.into()
}

fn invalid_view<'a>(entry: &'a Entry, err: &'a ParseError, theme: &Theme) -> Element<'a, Message> {
    let danger = th::danger();
    let mut content = column![].spacing(16).width(Fill);
    content = content.push(header(entry, theme));
    content = content.push(widgets::card(
        "Parse error",
        text(err.to_string())
            .size(13)
            .style(move |_| text::Style {
                color: Some(danger),
            })
            .into(),
        theme,
    ));
    content = content.push(source_line(entry, theme));
    content.into()
}

fn source_line<'a>(entry: &'a Entry, theme: &Theme) -> Element<'a, Message> {
    let muted = th::muted(theme);
    text(entry.source_path.display().to_string())
        .size(11)
        .style(move |_| text::Style { color: Some(muted) })
        .into()
}

fn render_markdown<'a>(content: &'a markdown::Content, theme: &Theme) -> Element<'a, Message> {
    let settings = markdown::Settings::with_style(markdown::Style::from_palette(theme.palette()));
    markdown::view(content.items(), settings).map(Message::LinkClicked)
}

fn tool_chips(list: &ToolList) -> Element<'_, Message> {
    let chips = list
        .iter()
        .map(|t| {
            let color = category_color(t.category());
            widgets::pill(t.to_token(), color, th::with_alpha(color, 0.15))
        })
        .collect();
    widgets::wrapped(chips, 4)
}

fn category_color(category: &str) -> Color {
    match category {
        "builtin" => Color::from_rgb8(0x3F, 0xB9, 0x50),
        "mcp" => Color::from_rgb8(0x4C, 0x8B, 0xF5),
        "agent" => Color::from_rgb8(0x9B, 0x59, 0xD6),
        "skill" => Color::from_rgb8(0x3F, 0xC1, 0xC9),
        _ => Color::from_rgb8(0xE8, 0x8B, 0x3C),
    }
}

fn model_label(fm: &AgentFrontmatter) -> &str {
    // Borrow the owned string inside Full; otherwise a static alias.
    match &fm.model {
        filament_core::ModelChoice::Full(s) => s.as_str(),
        other => other.as_str(),
    }
}
