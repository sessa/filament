//! MCP server definitions from `.mcp.json`.
//!
//! `.mcp.json` is `{ "mcpServers": { "<name>": { … } } }`. A server entry may
//! declare an explicit `type` (`stdio`/`http`/`sse`/`ws`); when omitted we infer
//! `stdio` if it has a `command` and `http` if it has a `url`, matching Claude
//! Code's behaviour.

use std::collections::BTreeMap;

use serde_json::Value as Json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub name: String,
    pub transport: McpTransport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTransport {
    Stdio {
        command: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
    Http {
        url: String,
        headers: BTreeMap<String, String>,
    },
    Sse {
        url: String,
        headers: BTreeMap<String, String>,
    },
    Ws {
        url: String,
    },
}

impl McpTransport {
    pub fn kind(&self) -> &'static str {
        match self {
            McpTransport::Stdio { .. } => "stdio",
            McpTransport::Http { .. } => "http",
            McpTransport::Sse { .. } => "sse",
            McpTransport::Ws { .. } => "ws",
        }
    }

    /// A one-line summary of where/how the server runs.
    pub fn endpoint(&self) -> String {
        match self {
            McpTransport::Stdio { command, args, .. } => {
                if args.is_empty() {
                    command.clone()
                } else {
                    format!("{command} {}", args.join(" "))
                }
            }
            McpTransport::Http { url, .. }
            | McpTransport::Sse { url, .. }
            | McpTransport::Ws { url } => url.clone(),
        }
    }
}

impl McpServer {
    /// Build a server from its JSON object, inferring the transport when `type`
    /// is absent.
    pub fn from_json(name: &str, v: &Json) -> Result<McpServer, String> {
        let obj = v
            .as_object()
            .ok_or_else(|| format!("server `{name}` must be an object"))?;

        let ty = obj
            .get("type")
            .and_then(Json::as_str)
            .map(str::to_ascii_lowercase);

        let transport = match ty.as_deref() {
            Some("stdio") => stdio(obj)?,
            Some("http") => http(obj)?,
            Some("sse") => sse(obj)?,
            Some("ws") | Some("websocket") => ws(obj)?,
            Some(other) => return Err(format!("unknown transport type `{other}`")),
            None if obj.contains_key("command") => stdio(obj)?,
            None if obj.contains_key("url") => http(obj)?,
            None => return Err("server has neither `command` nor `url`".to_string()),
        };

        Ok(McpServer {
            name: name.to_string(),
            transport,
        })
    }
}

type Obj = serde_json::Map<String, Json>;

fn stdio(obj: &Obj) -> Result<McpTransport, String> {
    Ok(McpTransport::Stdio {
        command: req_str(obj, "command")?,
        args: string_array(obj, "args"),
        env: string_map(obj, "env"),
    })
}

fn http(obj: &Obj) -> Result<McpTransport, String> {
    Ok(McpTransport::Http {
        url: req_str(obj, "url")?,
        headers: string_map(obj, "headers"),
    })
}

fn sse(obj: &Obj) -> Result<McpTransport, String> {
    Ok(McpTransport::Sse {
        url: req_str(obj, "url")?,
        headers: string_map(obj, "headers"),
    })
}

fn ws(obj: &Obj) -> Result<McpTransport, String> {
    Ok(McpTransport::Ws {
        url: req_str(obj, "url")?,
    })
}

fn req_str(obj: &Obj, key: &str) -> Result<String, String> {
    obj.get(key)
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("missing string field `{key}`"))
}

fn string_array(obj: &Obj, key: &str) -> Vec<String> {
    obj.get(key)
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn string_map(obj: &Obj, key: &str) -> BTreeMap<String, String> {
    obj.get(key)
        .and_then(Json::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_stdio() {
        let v: Json = serde_json::json!({"command": "npx", "args": ["-y", "pkg"]});
        let s = McpServer::from_json("x", &v).unwrap();
        assert_eq!(s.transport.kind(), "stdio");
        assert_eq!(s.transport.endpoint(), "npx -y pkg");
    }

    #[test]
    fn infers_http_from_url() {
        let v: Json = serde_json::json!({"url": "https://example.com/mcp"});
        let s = McpServer::from_json("x", &v).unwrap();
        assert_eq!(s.transport.kind(), "http");
    }

    #[test]
    fn explicit_sse() {
        let v: Json = serde_json::json!({"type": "sse", "url": "https://e/sse"});
        assert_eq!(
            McpServer::from_json("x", &v).unwrap().transport.kind(),
            "sse"
        );
    }

    #[test]
    fn rejects_empty() {
        assert!(McpServer::from_json("x", &serde_json::json!({})).is_err());
    }
}
