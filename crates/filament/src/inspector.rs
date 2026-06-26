//! The right-hand detail pane. Each config kind gets a tailored view:
//! agents, skills, and commands render metadata + the markdown body; MCP servers
//! show transport/endpoint/env; settings show permissions, hooks, and env.

use iced::widget::{column, markdown, row, scrollable, space, text};
use iced::{Center, Color, Element, Fill, Font, Length, Theme};

use filament_core::{
    AgentFrontmatter, CommandFrontmatter, Entry, HookEventGroup, McpServer, McpTransport,
    ParseError, Payload, Permissions, Settings, SkillFrontmatter, ToolList,
};

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
        Payload::Skill(fm) => skill_view(entry, fm, preview, theme),
        Payload::Command(fm) => command_view(entry, fm, preview, theme),
        Payload::Mcp(server) => mcp_view(entry, server, theme),
        Payload::Settings(settings) => settings_view(entry, settings, theme),
        Payload::Invalid(err) => invalid_view(entry, err, theme),
    };
    scrollable(iced::widget::container(body).padding(24))
        .height(Fill)
        .into()
}

// ---- shared pieces ----------------------------------------------------------

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

    let mut right = row![].spacing(6).align_y(Center);
    if matches!(entry.payload, Payload::Agent(_)) {
        right = right.push(widgets::secondary_button(
            "Edit",
            Message::EnterEditAgent,
            theme,
        ));
    }
    right = right.push(widgets::secondary_button(
        "Source",
        Message::EnterEditSource,
        theme,
    ));
    if entry.shadowed_by.is_some() {
        right = right.push(widgets::pill(
            "shadowed",
            muted,
            th::with_alpha(theme.palette().text, 0.08),
        ));
    }
    right = right.push(widgets::scope_pill(entry.scope));

    row![left, space().width(Fill), right]
        .align_y(Center)
        .into()
}

fn description<'a>(text_value: String, theme: &Theme) -> Element<'a, Message> {
    let muted = th::muted(theme);
    text(text_value)
        .size(14)
        .style(move |_| text::Style { color: Some(muted) })
        .into()
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

fn mono<'a>(value: String, color: Color) -> Element<'a, Message> {
    text(value)
        .size(12)
        .font(Font::MONOSPACE)
        .style(move |_| text::Style { color: Some(color) })
        .into()
}

// ---- agents -----------------------------------------------------------------

fn agent_view<'a>(
    entry: &'a Entry,
    fm: &'a AgentFrontmatter,
    preview: Option<&'a markdown::Content>,
    theme: &Theme,
) -> Element<'a, Message> {
    let mut content = column![].spacing(16).width(Fill);
    content = content.push(header(entry, theme));
    content = content.push(description(fm.description.clone(), theme));

    let mut badges: Vec<Element<Message>> =
        vec![widgets::kv_pill("model", fm.model.as_str(), theme)];
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
    if let Some(n) = fm.max_turns {
        badges.push(widgets::pill(
            format!("max turns: {n}"),
            theme.palette().text,
            th::surface(theme),
        ));
    }
    content = content.push(widgets::wrapped(badges, 5));

    if let Some(w) = &fm.when_to_use {
        if !w.trim().is_empty() {
            content = content.push(widgets::card(
                "When to use",
                description(w.clone(), theme),
                theme,
            ));
        }
    }
    if let Some(tools) = &fm.tools {
        content = content.push(widgets::card("Tools — allowed", tool_chips(tools), theme));
    }
    if let Some(denied) = &fm.disallowed_tools {
        content = content.push(widgets::card("Tools — denied", tool_chips(denied), theme));
    }
    if !fm.skills.is_empty() {
        content = content.push(widgets::card(
            "Preloaded skills",
            string_chips(&fm.skills, th::agent_color(filament_core::AgentColor::Cyan)),
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

// ---- skills -----------------------------------------------------------------

fn skill_view<'a>(
    entry: &'a Entry,
    fm: &'a SkillFrontmatter,
    preview: Option<&'a markdown::Content>,
    theme: &Theme,
) -> Element<'a, Message> {
    let mut content = column![].spacing(16).width(Fill);
    content = content.push(header(entry, theme));
    content = content.push(description(fm.description.clone(), theme));

    let mut badges: Vec<Element<Message>> =
        vec![widgets::kv_pill("model", fm.model.as_str(), theme)];
    if let Some(e) = fm.effort {
        badges.push(widgets::kv_pill("effort", e.as_str(), theme));
    }
    if fm.disable_model_invocation == Some(true) {
        badges.push(widgets::kv_pill("invocation", "manual only", theme));
    }
    if let Some(c) = &fm.context {
        badges.push(widgets::kv_pill("context", c, theme));
    }
    if let Some(a) = &fm.agent {
        badges.push(widgets::kv_pill("agent", a, theme));
    }
    if let Some(h) = &fm.argument_hint {
        badges.push(widgets::kv_pill("args", h, theme));
    }
    content = content.push(widgets::wrapped(badges, 5));

    if let Some(w) = &fm.when_to_use {
        if !w.trim().is_empty() {
            content = content.push(widgets::card(
                "When to use",
                description(w.clone(), theme),
                theme,
            ));
        }
    }
    if let Some(tools) = &fm.allowed_tools {
        content = content.push(widgets::card("Tools — allowed", tool_chips(tools), theme));
    }
    if let Some(tools) = &fm.disallowed_tools {
        content = content.push(widgets::card("Tools — denied", tool_chips(tools), theme));
    }

    content = content.push(source_line(entry, theme));
    if let Some(md) = preview {
        content = content.push(widgets::card("Skill", render_markdown(md, theme), theme));
    }
    content.into()
}

// ---- commands ---------------------------------------------------------------

fn command_view<'a>(
    entry: &'a Entry,
    fm: &'a CommandFrontmatter,
    preview: Option<&'a markdown::Content>,
    theme: &Theme,
) -> Element<'a, Message> {
    let mut content = column![].spacing(16).width(Fill);
    content = content.push(header(entry, theme));
    if let Some(desc) = &fm.description {
        content = content.push(description(desc.clone(), theme));
    }

    let mut badges: Vec<Element<Message>> =
        vec![widgets::kv_pill("model", fm.model.as_str(), theme)];
    if let Some(h) = &fm.argument_hint {
        badges.push(widgets::kv_pill("args", h, theme));
    }
    content = content.push(widgets::wrapped(badges, 5));

    if let Some(tools) = &fm.allowed_tools {
        content = content.push(widgets::card("Tools — allowed", tool_chips(tools), theme));
    }

    content = content.push(source_line(entry, theme));
    if let Some(md) = preview {
        content = content.push(widgets::card(
            "Command prompt",
            render_markdown(md, theme),
            theme,
        ));
    }
    content.into()
}

// ---- MCP servers ------------------------------------------------------------

fn mcp_view<'a>(entry: &'a Entry, server: &'a McpServer, theme: &Theme) -> Element<'a, Message> {
    let fg = theme.palette().text;
    let mut content = column![].spacing(16).width(Fill);
    content = content.push(header(entry, theme));

    let badges = vec![widgets::kv_pill(
        "transport",
        server.transport.kind(),
        theme,
    )];
    content = content.push(widgets::wrapped(badges, 5));

    content = content.push(widgets::card(
        "Endpoint",
        mono(server.transport.endpoint(), fg),
        theme,
    ));

    match &server.transport {
        McpTransport::Stdio { env, .. } if !env.is_empty() => {
            content = content.push(widgets::card(
                "Environment",
                kv_rows(env.iter(), theme),
                theme,
            ));
        }
        McpTransport::Http { headers, .. } | McpTransport::Sse { headers, .. }
            if !headers.is_empty() =>
        {
            content = content.push(widgets::card(
                "Headers",
                kv_rows(headers.iter(), theme),
                theme,
            ));
        }
        _ => {}
    }

    content = content.push(source_line(entry, theme));
    content.into()
}

// ---- settings ---------------------------------------------------------------

fn settings_view<'a>(
    entry: &'a Entry,
    settings: &'a Settings,
    theme: &Theme,
) -> Element<'a, Message> {
    let mut content = column![].spacing(16).width(Fill);
    content = content.push(header(entry, theme));

    if let Some(agent) = &settings.agent {
        content = content.push(widgets::wrapped(
            vec![widgets::kv_pill("default agent", agent, theme)],
            5,
        ));
    }

    if !settings.permissions.is_empty() {
        content = content.push(widgets::card(
            "Permissions",
            permissions_view(&settings.permissions),
            theme,
        ));
    }

    let hooks = settings.hook_groups();
    if !hooks.is_empty() {
        content = content.push(widgets::card("Hooks", hooks_view(hooks, theme), theme));
    }

    if !settings.env.is_empty() {
        content = content.push(widgets::card(
            "Environment",
            kv_rows(settings.env.iter(), theme),
            theme,
        ));
    }

    content = content.push(source_line(entry, theme));
    content.into()
}

fn permissions_view<'a>(perms: &'a Permissions) -> Element<'a, Message> {
    let green = Color::from_rgb8(0x3F, 0xB9, 0x50);
    let red = Color::from_rgb8(0xE5, 0x48, 0x4D);
    let amber = Color::from_rgb8(0xE8, 0xC3, 0x4A);

    let mut col = column![].spacing(10);
    if !perms.allow.is_empty() {
        col = col.push(perm_group("allow", &perms.allow, green));
    }
    if !perms.ask.is_empty() {
        col = col.push(perm_group("ask", &perms.ask, amber));
    }
    if !perms.deny.is_empty() {
        col = col.push(perm_group("deny", &perms.deny, red));
    }
    col.into()
}

fn perm_group<'a>(label: &'a str, items: &'a [String], color: Color) -> Element<'a, Message> {
    let chips = items
        .iter()
        .map(|s| widgets::pill(s.clone(), color, th::with_alpha(color, 0.15)))
        .collect();
    column![
        text(label)
            .size(11)
            .style(move |_| text::Style { color: Some(color) }),
        widgets::wrapped(chips, 3),
    ]
    .spacing(6)
    .into()
}

fn hooks_view<'a>(groups: Vec<HookEventGroup>, theme: &Theme) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let fg = theme.palette().text;
    let mut col = column![].spacing(12);
    for group in groups {
        let mut block = column![].spacing(4);
        block = block.push(text(group.event).size(13));
        for matcher in group.matchers {
            if let Some(m) = matcher.matcher {
                block = block.push(
                    text(format!("matcher: {m}"))
                        .size(12)
                        .style(move |_| text::Style { color: Some(muted) }),
                );
            }
            for cmd in matcher.commands {
                block = block.push(mono(format!("$ {}", cmd.command), fg));
            }
        }
        col = col.push(block);
    }
    col.into()
}

// ---- invalid ----------------------------------------------------------------

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

// ---- helpers ----------------------------------------------------------------

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

fn string_chips(items: &[String], color: Color) -> Element<'_, Message> {
    let chips = items
        .iter()
        .map(|s| widgets::pill(s.clone(), color, th::with_alpha(color, 0.15)))
        .collect();
    widgets::wrapped(chips, 6)
}

fn kv_rows<'a>(
    pairs: impl Iterator<Item = (&'a String, &'a String)>,
    theme: &Theme,
) -> Element<'a, Message> {
    let muted = th::muted(theme);
    let fg = theme.palette().text;
    let mut col = column![].spacing(4);
    for (k, v) in pairs {
        col = col.push(
            row![
                text(k.clone())
                    .size(12)
                    .style(move |_| text::Style { color: Some(muted) })
                    .width(Length::Fixed(170.0)),
                mono(v.clone(), fg),
            ]
            .spacing(8),
        );
    }
    col.into()
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
