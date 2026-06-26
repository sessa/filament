//! Per-file parsers. Each returns a fully-formed [`Entry`]; a malformed file
//! yields an entry with a [`Payload::Invalid`] rather than an error, so the scan
//! never aborts and the UI can still list the file with an error badge.

use std::path::Path;

use serde_norway::Value as Yaml;

use crate::error::ParseError;
use crate::frontmatter::split_frontmatter;
use crate::model::{
    AgentFrontmatter, CommandFrontmatter, Entry, ItemId, ItemKind, McpServer, Payload, RawDoc,
    Settings, SkillFrontmatter,
};
use crate::scope::Scope;

/// Read a file and split its frontmatter, capturing read failures as a
/// [`ParseError`].
fn read_raw(path: &Path) -> Result<RawDoc, ParseError> {
    let raw_text = std::fs::read_to_string(path).map_err(|e| ParseError::Unreadable {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    let split = split_frontmatter(&raw_text);
    Ok(RawDoc {
        source_path: path.to_path_buf(),
        raw_text,
        frontmatter: split.frontmatter,
        body: split.body,
        has_frontmatter: split.has_frontmatter,
    })
}

#[allow(clippy::too_many_arguments)]
fn entry(
    id: ItemId,
    kind: ItemKind,
    name: String,
    scope: Scope,
    precedence: u32,
    path: &Path,
    raw: Option<RawDoc>,
    payload: Payload,
) -> Entry {
    Entry {
        id,
        kind,
        name,
        scope,
        precedence,
        source_path: path.to_path_buf(),
        raw,
        payload,
        winning: true,
        shadowed_by: None,
        shadows: Vec::new(),
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("(unnamed)")
        .to_string()
}

fn skill_name_fallback(path: &Path) -> String {
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| file_stem(path))
}

// ---- Agents -----------------------------------------------------------------

pub fn parse_agent(path: &Path, scope: Scope, precedence: u32) -> Entry {
    let id = ItemId::for_path(ItemKind::Agent, path);
    let raw = match read_raw(path) {
        Ok(r) => r,
        Err(e) => {
            return entry(
                id,
                ItemKind::Agent,
                file_stem(path),
                scope,
                precedence,
                path,
                None,
                Payload::Invalid(e),
            )
        }
    };
    if !raw.has_frontmatter {
        let err = ParseError::MissingFrontmatter {
            path: path.to_path_buf(),
        };
        return entry(
            id,
            ItemKind::Agent,
            file_stem(path),
            scope,
            precedence,
            path,
            Some(raw),
            Payload::Invalid(err),
        );
    }
    match serde_norway::from_str::<AgentFrontmatter>(raw.frontmatter_str()) {
        Ok(fm) => {
            let name = fm.name.clone();
            entry(
                id,
                ItemKind::Agent,
                name,
                scope,
                precedence,
                path,
                Some(raw),
                Payload::Agent(fm),
            )
        }
        Err(e) => {
            let err = ParseError::Yaml {
                path: path.to_path_buf(),
                message: e.to_string(),
            };
            entry(
                id,
                ItemKind::Agent,
                file_stem(path),
                scope,
                precedence,
                path,
                Some(raw),
                Payload::Invalid(err),
            )
        }
    }
}

// ---- Skills -----------------------------------------------------------------

pub fn parse_skill(path: &Path, scope: Scope, precedence: u32) -> Entry {
    let id = ItemId::for_path(ItemKind::Skill, path);
    let fallback = skill_name_fallback(path);
    let raw = match read_raw(path) {
        Ok(r) => r,
        Err(e) => {
            return entry(
                id,
                ItemKind::Skill,
                fallback,
                scope,
                precedence,
                path,
                None,
                Payload::Invalid(e),
            )
        }
    };
    if !raw.has_frontmatter {
        let err = ParseError::MissingFrontmatter {
            path: path.to_path_buf(),
        };
        return entry(
            id,
            ItemKind::Skill,
            fallback,
            scope,
            precedence,
            path,
            Some(raw),
            Payload::Invalid(err),
        );
    }
    match serde_norway::from_str::<SkillFrontmatter>(raw.frontmatter_str()) {
        Ok(fm) => {
            let name = fm.name.clone();
            entry(
                id,
                ItemKind::Skill,
                name,
                scope,
                precedence,
                path,
                Some(raw),
                Payload::Skill(fm),
            )
        }
        Err(e) => {
            let err = ParseError::Yaml {
                path: path.to_path_buf(),
                message: e.to_string(),
            };
            entry(
                id,
                ItemKind::Skill,
                fallback,
                scope,
                precedence,
                path,
                Some(raw),
                Payload::Invalid(err),
            )
        }
    }
}

// ---- Commands ---------------------------------------------------------------

pub fn parse_command(path: &Path, scope: Scope, precedence: u32) -> Entry {
    let id = ItemId::for_path(ItemKind::Command, path);
    let fallback = file_stem(path);
    let raw = match read_raw(path) {
        Ok(r) => r,
        Err(e) => {
            return entry(
                id,
                ItemKind::Command,
                fallback,
                scope,
                precedence,
                path,
                None,
                Payload::Invalid(e),
            )
        }
    };
    // Commands frequently have no frontmatter; that's valid.
    if !raw.has_frontmatter {
        return entry(
            id,
            ItemKind::Command,
            fallback,
            scope,
            precedence,
            path,
            Some(raw),
            Payload::Command(CommandFrontmatter::default()),
        );
    }
    match serde_norway::from_str::<CommandFrontmatter>(raw.frontmatter_str()) {
        Ok(fm) => {
            let name = fm.name.clone().unwrap_or(fallback);
            entry(
                id,
                ItemKind::Command,
                name,
                scope,
                precedence,
                path,
                Some(raw),
                Payload::Command(fm),
            )
        }
        Err(e) => {
            let err = ParseError::Yaml {
                path: path.to_path_buf(),
                message: e.to_string(),
            };
            entry(
                id,
                ItemKind::Command,
                fallback,
                scope,
                precedence,
                path,
                Some(raw),
                Payload::Invalid(err),
            )
        }
    }
}

// ---- Settings ---------------------------------------------------------------

pub fn parse_settings(path: &Path, scope: Scope, precedence: u32) -> Entry {
    let id = ItemId::for_path(ItemKind::Settings, path);
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("settings.json")
        .to_string();
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            let err = ParseError::Unreadable {
                path: path.to_path_buf(),
                message: e.to_string(),
            };
            return entry(
                id,
                ItemKind::Settings,
                name,
                scope,
                precedence,
                path,
                None,
                Payload::Invalid(err),
            );
        }
    };
    match serde_json::from_str::<Settings>(&text) {
        Ok(s) => entry(
            id,
            ItemKind::Settings,
            name,
            scope,
            precedence,
            path,
            None,
            Payload::Settings(s),
        ),
        Err(e) => {
            let err = ParseError::Json {
                path: path.to_path_buf(),
                message: e.to_string(),
            };
            entry(
                id,
                ItemKind::Settings,
                name,
                scope,
                precedence,
                path,
                None,
                Payload::Invalid(err),
            )
        }
    }
}

// ---- MCP servers ------------------------------------------------------------

/// One `.mcp.json` can declare many servers, so this returns several entries.
pub fn parse_mcp_file(path: &Path, scope: Scope, precedence: u32) -> Vec<Entry> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            let id = ItemId::for_mcp(path, "(file)");
            let err = ParseError::Unreadable {
                path: path.to_path_buf(),
                message: e.to_string(),
            };
            let name = mcp_file_label(path);
            return vec![entry(
                id,
                ItemKind::McpServer,
                name,
                scope,
                precedence,
                path,
                None,
                Payload::Invalid(err),
            )];
        }
    };
    let json: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            let id = ItemId::for_mcp(path, "(file)");
            let err = ParseError::Json {
                path: path.to_path_buf(),
                message: e.to_string(),
            };
            let name = mcp_file_label(path);
            return vec![entry(
                id,
                ItemKind::McpServer,
                name,
                scope,
                precedence,
                path,
                None,
                Payload::Invalid(err),
            )];
        }
    };

    let Some(servers) = json.get("mcpServers").and_then(|v| v.as_object()) else {
        return Vec::new();
    };

    servers
        .iter()
        .map(|(name, value)| {
            let id = ItemId::for_mcp(path, name);
            match McpServer::from_json(name, value) {
                Ok(server) => entry(
                    id,
                    ItemKind::McpServer,
                    name.clone(),
                    scope,
                    precedence,
                    path,
                    None,
                    Payload::Mcp(server),
                ),
                Err(message) => {
                    let err = ParseError::Other {
                        path: path.to_path_buf(),
                        message,
                    };
                    entry(
                        id,
                        ItemKind::McpServer,
                        name.clone(),
                        scope,
                        precedence,
                        path,
                        None,
                        Payload::Invalid(err),
                    )
                }
            }
        })
        .collect()
}

fn mcp_file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(".mcp.json")
        .to_string()
}

/// Convenience: parse an inline `mcpServers` block from agent/skill frontmatter
/// (kept opaque elsewhere) into displayable servers. Best-effort; unknown shapes
/// are skipped.
pub fn parse_inline_mcp(value: &Yaml) -> Vec<McpServer> {
    let Some(map) = value.as_mapping() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (k, v) in map {
        let Some(name) = k.as_str() else { continue };
        // Round-trip YAML -> JSON to reuse the JSON inference.
        if let Ok(json) = serde_norway::from_value::<serde_json::Value>(v.clone()) {
            if let Ok(server) = McpServer::from_json(name, &json) {
                out.push(server);
            }
        }
    }
    out
}
