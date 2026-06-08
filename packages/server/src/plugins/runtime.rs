use anyhow::Context;
use base64::Engine as _;
use reqwest::header::{AUTHORIZATION, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    net::IpAddr,
    path::{Component as PathComponent, Path, PathBuf},
    time::Duration,
};
use wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, HasData, Linker},
};

use crate::{
    conversation::memory,
    plugins::{
        package,
        registry::{InstalledPlugin, Manifest},
        secrets,
    },
    providers::types::{ToolCall, ToolDefinition},
    state::AppState,
};

wasmtime::component::bindgen!({
    inline: r#"
        package helpcore:plugin;

        interface host {
            http-request: func(request-json: string) -> result<string, string>;
            http-request-binary: func(request-json: string) -> result<string, string>;
            data-read: func(path: string) -> result<string, string>;
            data-write: func(path: string, content: string) -> result<_, string>;
            config-read: func(key: string) -> result<string, string>;
        }

        world plugin {
            import host;
            export call: func(tool: string, input-json: string) -> result<string, string>;
        }
    "#,
});

const WASM_MEMORY_LIMIT: usize = 64 * 1024 * 1024;
const WASM_FUEL_LIMIT: u64 = 1_000_000_000;
const WASM_TIMEOUT: Duration = Duration::from_secs(30);
const BRIDGE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_TOOL_RESULT_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
struct RuntimeTool {
    plugin: InstalledPlugin,
}

pub struct ToolCatalog {
    definitions: Vec<ToolDefinition>,
    tools: HashMap<String, RuntimeTool>,
}

impl ToolCatalog {
    pub async fn load(state: &AppState, user_id: &str) -> anyhow::Result<Self> {
        let user_id = user_id.to_string();
        let plugins = state
            .db
            .call(move |conn| crate::plugins::registry::list_enabled(conn, &user_id))
            .await?;
        let mut definitions = builtin_tool_definitions();
        let mut tools = HashMap::new();
        for plugin in plugins.into_iter().filter(|plugin| plugin.enabled) {
            for tool in &plugin.tools.tools {
                if tools.contains_key(&tool.name) || is_builtin_tool(&tool.name) {
                    anyhow::bail!("enabled plugins expose duplicate tool {}", tool.name);
                }
                definitions.push(ToolDefinition {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    input_schema: tool.input_schema.clone(),
                });
                tools.insert(
                    tool.name.clone(),
                    RuntimeTool {
                        plugin: plugin.clone(),
                    },
                );
            }
        }
        Ok(Self { definitions, tools })
    }

    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    pub async fn execute(
        &self,
        state: &AppState,
        user_id: &str,
        conversation_id: Option<&str>,
        call: &ToolCall,
    ) -> anyhow::Result<String> {
        if is_builtin_tool(&call.name) {
            return execute_builtin(state, user_id, conversation_id, call).await;
        }
        let runtime = self
            .tools
            .get(&call.name)
            .with_context(|| format!("model requested unknown tool {}", call.name))?;
        match runtime.plugin.tier.as_str() {
            "wasm" => execute_wasm(state, user_id, &runtime.plugin, call).await,
            "bridge" => execute_bridge(state, user_id, &runtime.plugin, call).await,
            tier => anyhow::bail!("unsupported plugin tier {tier}"),
        }
    }
}

// ── Built-in tools ────────────────────────────────────────────────────────────
//
// Memory and personality management are core capabilities, not plugin
// concerns — every user gets them regardless of which plugins are enabled.
// They run directly against the conversation::memory module rather than
// through the WASM/bridge plugin runtimes.

/// Upper bound on the `limit` a model can request from `memory_search`,
/// mirroring how the wasm/bridge tiers cap `MAX_TOOL_RESULT_BYTES` — without
/// it a single tool call could dump a large slice of memory back into context.
const MEMORY_SEARCH_MAX_LIMIT: u64 = 25;

const BUILTIN_TOOL_NAMES: &[&str] = &[
    "memory_list",
    "memory_read",
    "memory_write",
    "memory_append",
    "memory_move",
    "memory_delete",
    "memory_search",
    "personality_write",
    "personality_append",
    "conversation_rename",
    "chart_generate",
    "mermaid_render",
    "skill_read",
];

fn is_builtin_tool(name: &str) -> bool {
    BUILTIN_TOOL_NAMES.contains(&name)
}

fn builtin_tool_definitions() -> Vec<ToolDefinition> {
    let path_property = serde_json::json!({
        "type": "string",
        "description": "Relative path, e.g. 'alice-chen.md' or 'home/devices.md'"
    });
    let content_property = serde_json::json!({
        "type": "string",
        "description": "Full Markdown content"
    });
    vec![
        ToolDefinition {
            name: "memory_list".into(),
            description: "List all of your memory files (path and last-updated time, no \
                content). Use this to see what already exists before deciding where to \
                save something new."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "memory_read".into(),
            description: "Read the full content of one memory file by its relative path.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "path": path_property },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "memory_write".into(),
            description: "Create a new memory file, or completely replace the content of \
                an existing one. Use memory_append instead if you want to add to a file \
                without losing what's already in it."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "path": path_property, "content": content_property },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "memory_append".into(),
            description: "Append content to the end of a memory file on its own line, \
                creating the file if it doesn't exist yet."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "path": path_property, "content": content_property },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "memory_move".into(),
            description: "Rename or move a memory file to a new path. Always ask the user \
                before moving or renaming a file."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Current relative path" },
                    "to": { "type": "string", "description": "New relative path" }
                },
                "required": ["from", "to"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "memory_delete".into(),
            description: "Permanently delete a memory file. This cannot be undone — \
                always confirm with the user first."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "path": path_property },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "memory_search".into(),
            description: "Full-text search across all memory files by keyword. Relevant \
                files are already injected into context automatically each turn — use \
                this when you need to look for something specific that wasn't surfaced."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search keywords" },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 25,
                        "description": "Maximum number of results (default 10, capped at 25)"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "personality_write".into(),
            description: "Replace the content of one of your own personality files: \
                'soul' (your tone and personality), 'identity' (your name and \
                background), or 'user' (facts about the person you're talking to). This \
                replaces the whole file — use personality_append for small additions \
                that don't need to rewrite everything."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "enum": ["soul", "identity", "user"],
                        "description": "Which personality file to replace"
                    },
                    "content": content_property
                },
                "required": ["name", "content"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "personality_append".into(),
            description: "Append content to the end of one of your own personality \
                files ('soul', 'identity', or 'user'). Use this to add a single fact \
                or preference without rewriting the whole file. Creates the file if \
                it doesn't exist yet."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "enum": ["soul", "identity", "user"],
                        "description": "Which personality file to append to"
                    },
                    "content": content_property
                },
                "required": ["name", "content"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "conversation_rename".into(),
            description: "Rename the current conversation. Use this when the \
                conversation's main topic has clearly shifted from its current title. \
                Don't rename for minor tangents — only when the central subject has \
                changed significantly."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "The new title for the conversation"
                    }
                },
                "required": ["title"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "chart_generate".into(),
            description: "Generate a chart (pie, bar, line, or donut) from raw data. \
                Returns JSON with an 'action' of 'chart', the 'chart_type', an optional \
                'title', and a 'data_uri' (base64 SVG). To display the chart, extract \
                the data_uri and emit it in markdown as ![title](data_uri). Use this \
                when the user needs to visualize numbers, trends, or proportions."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["pie", "bar", "line", "donut"],
                        "description": "The type of chart to generate"
                    },
                    "labels": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Category labels for the data points"
                    },
                    "values": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Numerical values corresponding to each label"
                    },
                    "title": {
                        "type": "string",
                        "description": "Optional title for the chart"
                    },
                    "colors": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of CSS colors for the data series"
                    },
                    "width": {
                        "type": "integer",
                        "default": 600,
                        "description": "Chart width in pixels"
                    },
                    "height": {
                        "type": "integer",
                        "default": 400,
                        "description": "Chart height in pixels"
                    }
                },
                "required": ["type", "labels", "values"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "mermaid_render".into(),
            description: "Render a Mermaid diagram definition. Returns JSON with an \
                'action' of 'mermaid', the 'theme', and a 'data_uri' (base64 PNG). To \
                display the diagram, extract the data_uri and emit it in markdown as \
                ![diagram](data_uri). Supports flowcharts, sequence diagrams, gantt \
                charts, and more. Use this when the user needs to visualize processes, \
                architectures, or relationships."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "definition": {
                        "type": "string",
                        "description": "The raw Mermaid diagram definition string (e.g. 'graph TD...')"
                    },
                    "theme": {
                        "type": "string",
                        "enum": ["default", "dark", "neutral"],
                        "default": "default",
                        "description": "The visual theme for the diagram"
                    },
                    "width": {
                        "type": "integer",
                        "default": 800,
                        "description": "Target width in pixels"
                    },
                    "height": {
                        "type": "integer",
                        "default": 600,
                        "description": "Target height in pixels"
                    }
                },
                "required": ["definition"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "skill_read".into(),
            description: "Read the full instructions for an enabled plugin skill by name. \
                Use this when you need detailed guidance on how to use a plugin's tools \
                — the skill index above gives you a one-line summary for each plugin; \
                call skill_read to get the complete instructions before invoking a \
                plugin's tools for the first time or when you need detailed parameter \
                information."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The plugin name as shown in the skill index (e.g. 'Weather', 'Calculator', 'Home Assistant')"
                    }
                },
                "required": ["name"],
                "additionalProperties": false
            }),
        },
    ]
}

fn require_str_arg<'a>(arguments: &'a serde_json::Value, key: &str) -> anyhow::Result<&'a str> {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_str)
        .with_context(|| format!("tool call is missing required string argument '{key}'"))
}

async fn execute_builtin(
    state: &AppState,
    user_id: &str,
    conversation_id: Option<&str>,
    call: &ToolCall,
) -> anyhow::Result<String> {
    let uid = user_id.to_string();
    match call.name.as_str() {
        "memory_list" => {
            let entries = state
                .db
                .call(move |conn| memory::list_memory(conn, &uid))
                .await?;
            let payload: Vec<_> = entries
                .into_iter()
                .map(|entry| serde_json::json!({ "path": entry.path, "updated_at": entry.updated_at }))
                .collect();
            Ok(serde_json::to_string(&payload)?)
        }
        "memory_read" => {
            let path = memory::sanitize_path(require_str_arg(&call.arguments, "path")?)?;
            let path_clone = path.clone();
            let content = state
                .db
                .call(move |conn| memory::read_memory(conn, &uid, &path_clone))
                .await?;
            Ok(serde_json::json!({
                "action": "read",
                "path": path,
                "content": content
            })
            .to_string())
        }
        "memory_write" => {
            let path = memory::sanitize_path(require_str_arg(&call.arguments, "path")?)?;
            let content = require_str_arg(&call.arguments, "content")?.to_string();
            let path_for_db = path.clone();
            let content_for_db = content.clone();
            let old_content = state
                .db
                .call(move |conn| memory::write_memory(conn, &uid, &path_for_db, &content_for_db))
                .await?;
            Ok(serde_json::json!({
                "action": "write",
                "path": path,
                "content": content,
                "old_content": old_content
            })
            .to_string())
        }
        "memory_append" => {
            let path = memory::sanitize_path(require_str_arg(&call.arguments, "path")?)?;
            let content = require_str_arg(&call.arguments, "content")?.to_string();
            let path_for_db = path.clone();
            let content_for_db = content.clone();
            let (old_content, combined) = state
                .db
                .call(move |conn| memory::append_memory(conn, &uid, &path_for_db, &content_for_db))
                .await?;
            Ok(serde_json::json!({
                "action": "append",
                "path": path,
                "content": combined,
                "old_content": old_content
            })
            .to_string())
        }
        "memory_move" => {
            let from = memory::sanitize_path(require_str_arg(&call.arguments, "from")?)?;
            let to = memory::sanitize_path(require_str_arg(&call.arguments, "to")?)?;
            let from_clone = from.clone();
            let to_clone = to.clone();
            let moved = state
                .db
                .call(move |conn| memory::move_memory(conn, &uid, &from_clone, &to_clone))
                .await?;
            Ok(serde_json::json!({
                "action": "move",
                "from": from,
                "to": to,
                "status": if moved { "moved" } else { "not_found" }
            })
            .to_string())
        }
        "memory_delete" => {
            let path = memory::sanitize_path(require_str_arg(&call.arguments, "path")?)?;
            let path_clone = path.clone();
            let old_content = state
                .db
                .call(move |conn| memory::delete_memory(conn, &uid, &path_clone))
                .await?;
            Ok(serde_json::json!({
                "action": "delete",
                "path": path,
                "old_content": old_content
            })
            .to_string())
        }
        "memory_search" => {
            let query = require_str_arg(&call.arguments, "query")?.to_string();
            // Clamp like the wasm/bridge tiers clamp result size: an
            // unbounded model-supplied limit could pull a large slice of the
            // user's memory back into its own context in one round trip.
            let limit = call
                .arguments
                .get("limit")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(10)
                .clamp(1, MEMORY_SEARCH_MAX_LIMIT) as usize;
            let results = state
                .db
                .call(move |conn| memory::search_memory(conn, &uid, &query, limit))
                .await?;
            let payload: Vec<_> = results
                .into_iter()
                .map(|result| serde_json::json!({ "path": result.path, "content": result.content }))
                .collect();
            Ok(serde_json::to_string(&payload)?)
        }
        "personality_write" => {
            let name = require_str_arg(&call.arguments, "name")?.to_string();
            if !["soul", "identity", "user"].contains(&name.as_str()) {
                anyhow::bail!("personality name must be one of 'soul', 'identity', 'user'");
            }
            let content = require_str_arg(&call.arguments, "content")?.to_string();
            let name_for_db = name.clone();
            let content_for_db = content.clone();
            let old_content = state
                .db
                .call(move |conn| {
                    memory::set_personality(conn, &uid, &name_for_db, &content_for_db)
                })
                .await?;
            Ok(serde_json::json!({
                "action": "personality_write",
                "name": name,
                "content": content,
                "old_content": old_content
            })
            .to_string())
        }
        "personality_append" => {
            let name = require_str_arg(&call.arguments, "name")?.to_string();
            if !["soul", "identity", "user"].contains(&name.as_str()) {
                anyhow::bail!("personality name must be one of 'soul', 'identity', 'user'");
            }
            let content = require_str_arg(&call.arguments, "content")?.to_string();
            let name_for_db = name.clone();
            let content_for_db = content.clone();
            let (old_content, combined) = state
                .db
                .call(move |conn| {
                    memory::append_personality(conn, &uid, &name_for_db, &content_for_db)
                })
                .await?;
            Ok(serde_json::json!({
                "action": "personality_append",
                "name": name,
                "content": combined,
                "old_content": old_content
            })
            .to_string())
        }
        "conversation_rename" => {
            let cid = conversation_id
                .context("conversation_rename can only be called from within a conversation")?
                .to_string();
            let title = require_str_arg(&call.arguments, "title")?.to_string();
            state
                .db
                .call(move |conn| {
                    crate::conversation::history::update_conversation_title(
                        conn, &cid, &uid, &title,
                    )
                })
                .await?;
            Ok("renamed".to_string())
        }
        "chart_generate" => {
            let chart_type = require_str_arg(&call.arguments, "type")?;
            let labels: Vec<String> = call
                .arguments
                .get("labels")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect()
                })
                .unwrap_or_default();
            let values: Vec<f64> = call
                .arguments
                .get("values")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect())
                .unwrap_or_default();
            let title = call.arguments.get("title").and_then(|v| v.as_str());
            let colors = call
                .arguments
                .get("colors")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect()
                });
            let width = call
                .arguments
                .get("width")
                .and_then(|v| v.as_u64())
                .unwrap_or(600) as u32;
            let height = call
                .arguments
                .get("height")
                .and_then(|v| v.as_u64())
                .unwrap_or(400) as u32;

            let svg = crate::plugins::chart::generate_svg(
                chart_type, &labels, &values, title, colors, width, height,
            );
            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, svg);
            let data_uri = format!("data:image/svg+xml;base64,{}", b64);
            Ok(serde_json::json!({
                "action": "chart",
                "chart_type": chart_type,
                "title": title,
                "data_uri": data_uri,
                "width": width,
                "height": height
            })
            .to_string())
        }
        "mermaid_render" => {
            let definition = require_str_arg(&call.arguments, "definition")?;
            let theme = call
                .arguments
                .get("theme")
                .and_then(|v| v.as_str())
                .unwrap_or("default");

            // Build mermaid.ink URL
            let json = serde_json::json!({
                "code": definition,
                "mermaid": { "theme": theme }
            });
            let bytes = serde_json::to_vec(&json)?;
            let encoded =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD_NO_PAD, bytes);
            let url = format!("https://mermaid.ink/img/{}", encoded);

            let client = reqwest::Client::builder().timeout(BRIDGE_TIMEOUT).build()?;
            let resp = client.get(url).send().await?;
            if !resp.status().is_success() {
                anyhow::bail!("failed to render mermaid diagram: {}", resp.status());
            }
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            let bytes = resp.bytes().await?;
            if !content_type.starts_with("image/") {
                let preview = String::from_utf8_lossy(&bytes);
                anyhow::bail!(
                    "mermaid.ink returned non-image response ({}): {}",
                    content_type,
                    preview.chars().take(200).collect::<String>()
                );
            }
            if bytes.len() > MAX_TOOL_RESULT_BYTES {
                anyhow::bail!("mermaid render result exceeds the 1 MiB limit");
            }
            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
            Ok(serde_json::json!({
                "action": "mermaid",
                "theme": theme,
                "data_uri": format!("data:image/png;base64,{}", b64)
            })
            .to_string())
        }
        "skill_read" => {
            let name = require_str_arg(&call.arguments, "name")?.to_string();
            let name_for_error = name.clone();
            let content = state
                .db
                .call(move |conn| crate::plugins::registry::read_plugin_skill(conn, &uid, &name))
                .await?;
            match content {
                Some(skill_md) => Ok(skill_md),
                None => Ok(format!(
                    "No enabled plugin named '{name_for_error}' is installed. \
                     Check the skill index above for exact plugin names."
                )),
            }
        }
        other => anyhow::bail!("unknown built-in tool {other}"),
    }
}

struct WasmState {
    limits: StoreLimits,
    permissions: HashSet<String>,
    workspace: PathBuf,
    allowed_hosts: Vec<String>,
    config: serde_json::Value,
}

impl HasData for WasmState {
    type Data<'a> = &'a mut WasmState;
}

async fn execute_wasm(
    state: &AppState,
    user_id: &str,
    plugin: &InstalledPlugin,
    call: &ToolCall,
) -> anyhow::Result<String> {
    let wasm_path = package::plugin_root(&state.data_dir, user_id, &plugin.plugin_id)
        .join("versions")
        .join(&plugin.version)
        .join("plugin.wasm");
    let tool = call.name.clone();
    let input = serde_json::to_string(&call.arguments)?;
    let workspace = state.data_dir.join("users").join(user_id).join("workspace");
    let permissions = plugin.permissions.iter().cloned().collect();
    let allowed_hosts = plugin.manifest.allowed_hosts.clone();

    // Build a merged config map: non-secret stored values + decrypted secrets.
    let mut plugin_config = plugin.config.clone();
    if let Some(obj) = plugin_config.as_object_mut() {
        obj.remove("_secret_keys");
    }
    if let Some(encrypted) = plugin.secrets.as_deref() {
        let data_dir = state.data_dir.clone();
        let uid = user_id.to_string();
        let pid = plugin.plugin_id.clone();
        let encrypted = encrypted.to_string();
        let decrypted = tokio::task::spawn_blocking(move || {
            secrets::decrypt(&data_dir, &uid, &pid, &encrypted)
        })
        .await
        .map_err(|error| anyhow::anyhow!("WASM secret decrypt task failed: {error}"))??;
        if let (Some(cfg), Some(sec)) = (plugin_config.as_object_mut(), decrypted.as_object()) {
            for (key, value) in sec {
                cfg.insert(key.clone(), value.clone());
            }
        }
    }

    tokio::task::spawn_blocking(move || {
        run_wasm_component(
            &wasm_path,
            &tool,
            &input,
            workspace,
            permissions,
            allowed_hosts,
            plugin_config,
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("WASM plugin task failed: {error}"))?
}

fn run_wasm_component(
    wasm_path: &Path,
    tool: &str,
    input: &str,
    workspace: PathBuf,
    permissions: HashSet<String>,
    allowed_hosts: Vec<String>,
    plugin_config: serde_json::Value,
) -> anyhow::Result<String> {
    let mut engine_config = Config::new();
    engine_config.wasm_component_model(true);
    engine_config.consume_fuel(true);
    engine_config.epoch_interruption(true);
    let engine = Engine::new(&engine_config)?;
    let component = Component::from_file(&engine, wasm_path)
        .with_context(|| format!("failed to load {}", wasm_path.display()))?;
    let mut linker = Linker::new(&engine);
    Plugin::add_to_linker::<WasmState, WasmState>(&mut linker, |state: &mut WasmState| state)?;
    fs::create_dir_all(&workspace)?;
    set_private_directory(&workspace)?;
    let limits = StoreLimitsBuilder::new()
        .memory_size(WASM_MEMORY_LIMIT)
        .instances(4)
        .memories(4)
        .tables(4)
        .trap_on_grow_failure(true)
        .build();
    let mut store = Store::new(
        &engine,
        WasmState {
            limits,
            permissions,
            workspace,
            allowed_hosts,
            config: plugin_config,
        },
    );
    store.limiter(|state| &mut state.limits);
    store.set_fuel(WASM_FUEL_LIMIT)?;
    store.set_epoch_deadline(1);
    store.epoch_deadline_trap();

    let deadline_engine = engine.clone();
    std::thread::spawn(move || {
        std::thread::sleep(WASM_TIMEOUT);
        deadline_engine.increment_epoch();
    });

    let bindings = Plugin::instantiate(&mut store, &component, &linker)
        .context("plugin does not implement the helpcore Component Model ABI")?;
    let result = bindings
        .call_call(&mut store, tool, input)
        .map_err(|error| anyhow::anyhow!("WASM tool call trapped: {error:#}"))?
        .map_err(|error| anyhow::anyhow!("WASM tool returned an error: {error}"))?;
    if result.len() > MAX_TOOL_RESULT_BYTES {
        anyhow::bail!("WASM tool result exceeds the 1 MiB limit");
    }
    Ok(result)
}

#[derive(Deserialize)]
struct WasmHttpRequest {
    method: String,
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    body: Option<String>,
    #[serde(default)]
    body_base64: Option<String>,
}

#[derive(Serialize)]
struct WasmHttpResponse {
    status: u16,
    headers: HashMap<String, String>,
    body: String,
}

impl helpcore::plugin::host::Host for WasmState {
    fn http_request(&mut self, request_json: String) -> Result<String, String> {
        let response = self.send_http_request(&request_json)?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        let body = response.text().map_err(|error| error.to_string())?;
        if body.len() > MAX_TOOL_RESULT_BYTES {
            return Err("HTTP response exceeds the 1 MiB limit".into());
        }
        serde_json::to_string(&WasmHttpResponse {
            status,
            headers,
            body,
        })
        .map_err(|error| error.to_string())
    }

    fn http_request_binary(&mut self, request_json: String) -> Result<String, String> {
        let response = self.send_http_request(&request_json)?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        let bytes = response.bytes().map_err(|error| error.to_string())?;
        if bytes.len() > MAX_TOOL_RESULT_BYTES {
            return Err("HTTP response exceeds the 1 MiB limit".into());
        }
        let body = base64::engine::general_purpose::STANDARD.encode(&bytes);
        serde_json::to_string(&WasmHttpResponse {
            status,
            headers,
            body,
        })
        .map_err(|error| error.to_string())
    }

    fn data_read(&mut self, path: String) -> Result<String, String> {
        self.require("user_data_read")?;
        let path = resolve_workspace_path(&self.workspace, &path, false)
            .map_err(|error| error.to_string())?;
        fs::read_to_string(path).map_err(|error| error.to_string())
    }

    fn config_read(&mut self, key: String) -> Result<String, String> {
        match self.config.get(&key) {
            Some(serde_json::Value::String(s)) => Ok(s.clone()),
            Some(v) => serde_json::to_string(v).map_err(|e| e.to_string()),
            None => Err(format!("config key '{key}' not found")),
        }
    }

    fn data_write(&mut self, path: String, content: String) -> Result<(), String> {
        self.require("user_data_write")?;
        if content.len() > MAX_TOOL_RESULT_BYTES {
            return Err("workspace write exceeds the 1 MiB limit".into());
        }
        let path = resolve_workspace_path(&self.workspace, &path, true)
            .map_err(|error| error.to_string())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            reject_symlink_path(&self.workspace, parent).map_err(|error| error.to_string())?;
            set_private_directory(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&path, content).map_err(|error| error.to_string())?;
        set_private_file(&path).map_err(|error| error.to_string())
    }
}

impl WasmState {
    fn require(&self, permission: &str) -> Result<(), String> {
        if self.permissions.contains(permission) {
            Ok(())
        } else {
            Err(format!("plugin was not approved for {permission}"))
        }
    }

    fn send_http_request(&self, request_json: &str) -> Result<reqwest::blocking::Response, String> {
        let request: WasmHttpRequest =
            serde_json::from_str(request_json).map_err(|error| error.to_string())?;
        let url = reqwest::Url::parse(&request.url).map_err(|error| error.to_string())?;
        ensure_host_allowed(&self.allowed_hosts, &url).map_err(|error| error.to_string())?;
        let mut client_builder = reqwest::blocking::Client::builder().timeout(BRIDGE_TIMEOUT);
        // Allow self-signed certs for private IPs, .local hostnames, and localhost
        if is_private_or_local(&url) {
            client_builder = client_builder.danger_accept_invalid_certs(true);
        }
        let client = client_builder.build().map_err(|error| error.to_string())?;
        let method = reqwest::Method::from_bytes(request.method.as_bytes())
            .map_err(|error| error.to_string())?;
        let mut builder = client.request(method, url);
        for (name, value) in request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body_b64) = request.body_base64 {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&body_b64)
                .map_err(|error| format!("invalid base64 in body_base64: {error}"))?;
            builder = builder.body(bytes);
        } else if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let response = builder.send().map_err(|error| error.to_string())?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_TOOL_RESULT_BYTES as u64)
        {
            return Err("HTTP response exceeds the 1 MiB limit".into());
        }
        Ok(response)
    }
}

async fn execute_bridge(
    state: &AppState,
    user_id: &str,
    plugin: &InstalledPlugin,
    call: &ToolCall,
) -> anyhow::Result<String> {
    require_permission(plugin, "outbound_http")?;
    let endpoint = plugin
        .config
        .get("endpoint")
        .and_then(serde_json::Value::as_str)
        .context("bridge plugin has no configured endpoint")?;
    let base = reqwest::Url::parse(endpoint).context("bridge endpoint is invalid")?;
    ensure_allowed_host(&plugin.manifest, &base)?;
    let url = base
        .join(&format!("tools/{}", call.name))
        .context("bridge tool URL is invalid")?;

    let client = reqwest::Client::builder().timeout(BRIDGE_TIMEOUT).build()?;
    let mut request = client.post(url).json(&serde_json::json!({
        "arguments": call.arguments,
        "settings": plugin.config.get("settings").cloned().unwrap_or_default(),
    }));
    if let Some(encrypted) = plugin.secrets.as_deref() {
        let data_dir = state.data_dir.clone();
        let user_id = user_id.to_string();
        let plugin_id = plugin.plugin_id.clone();
        let encrypted = encrypted.to_string();
        let credentials = tokio::task::spawn_blocking(move || {
            secrets::decrypt(&data_dir, &user_id, &plugin_id, &encrypted)
        })
        .await
        .map_err(|error| anyhow::anyhow!("bridge credential task failed: {error}"))??;
        if let Some(token) = credentials.get("token").and_then(serde_json::Value::as_str) {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        if let Some(headers) = credentials
            .get("headers")
            .and_then(serde_json::Value::as_object)
        {
            for (name, value) in headers {
                let Some(value) = value.as_str() else {
                    anyhow::bail!("bridge secret headers must contain string values");
                };
                request = request.header(
                    HeaderName::from_bytes(name.as_bytes())
                        .context("invalid bridge header name")?,
                    HeaderValue::from_str(value).context("invalid bridge header value")?,
                );
            }
        }
    }
    let response = request.send().await?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_TOOL_RESULT_BYTES as u64)
    {
        anyhow::bail!("bridge tool result exceeds the 1 MiB limit");
    }
    let bytes = response.bytes().await?;
    if bytes.len() > MAX_TOOL_RESULT_BYTES {
        anyhow::bail!("bridge tool result exceeds the 1 MiB limit");
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        serde_json::Value::String(String::from_utf8_lossy(&bytes).into_owned())
    });
    let result = value.get("result").cloned().unwrap_or(value);
    match result {
        serde_json::Value::String(value) => Ok(value),
        value => Ok(serde_json::to_string(&value)?),
    }
}

fn require_permission(plugin: &InstalledPlugin, permission: &str) -> anyhow::Result<()> {
    if plugin
        .permissions
        .iter()
        .any(|approved| approved == permission)
    {
        Ok(())
    } else {
        anyhow::bail!(
            "plugin {} was not approved for {permission}",
            plugin.plugin_id
        )
    }
}

fn ensure_allowed_host(manifest: &Manifest, url: &reqwest::Url) -> anyhow::Result<()> {
    ensure_host_allowed(&manifest.allowed_hosts, url)
}

/// Whether a URL points to a private address where self-signed certs are expected.
fn is_private_or_local(url: &reqwest::Url) -> bool {
    url.host_str().is_some_and(|host| {
        host == "localhost"
            || host.ends_with(".local")
            || host
                .parse::<IpAddr>()
                .map(|addr| match addr {
                    IpAddr::V4(v4) => v4.is_private() || v4.is_loopback(),
                    IpAddr::V6(v6) => v6.is_loopback(),
                })
                .unwrap_or(false)
    })
}

fn ensure_host_allowed(allowed_hosts: &[String], url: &reqwest::Url) -> anyhow::Result<()> {
    let host = url.host_str().context("bridge endpoint has no host")?;
    if allowed_hosts.iter().any(|allowed| {
        allowed == "*"
            || allowed == host
            || allowed
                .strip_prefix("*.")
                .is_some_and(|suffix| host.ends_with(&format!(".{suffix}")))
    }) {
        Ok(())
    } else {
        anyhow::bail!("bridge endpoint host is not declared by the plugin")
    }
}

fn resolve_workspace_path(
    root: &Path,
    relative: &str,
    allow_missing: bool,
) -> anyhow::Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, PathComponent::Normal(_) | PathComponent::CurDir))
    {
        anyhow::bail!("workspace path must be relative and cannot contain '..'");
    }
    let root = root
        .canonicalize()
        .context("failed to resolve user workspace")?;
    let candidate = root.join(path);
    if allow_missing {
        reject_symlink_path(&root, candidate.parent().unwrap_or(&root))?;
        return Ok(candidate);
    }
    let resolved = candidate
        .canonicalize()
        .context("workspace path does not exist")?;
    if !resolved.starts_with(&root) {
        anyhow::bail!("workspace path escapes the user's workspace");
    }
    Ok(resolved)
}

fn reject_symlink_path(root: &Path, path: &Path) -> anyhow::Result<()> {
    let relative = path
        .strip_prefix(root)
        .context("workspace path escapes its root")?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if current.exists() && fs::symlink_metadata(&current)?.file_type().is_symlink() {
            anyhow::bail!("workspace paths may not traverse symlinks");
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(hosts: &[&str]) -> Manifest {
        Manifest {
            id: "test".into(),
            name: "Test".into(),
            version: "1.0.0".into(),
            description: String::new(),
            tier: "bridge".into(),
            permissions: vec!["outbound_http".into()],
            provides: Vec::new(),
            min_core_version: None,
            bridge: None,
            allowed_hosts: hosts.iter().map(|host| host.to_string()).collect(),
            config_schema: Vec::new(),
            brief: "Use when testing.".to_string(),
        }
    }

    #[test]
    fn bridge_hosts_are_constrained() {
        assert!(
            ensure_allowed_host(
                &manifest(&["api.example.com"]),
                &reqwest::Url::parse("https://api.example.com/tools/a").unwrap(),
            )
            .is_ok()
        );
        assert!(
            ensure_allowed_host(
                &manifest(&["*.example.com"]),
                &reqwest::Url::parse("https://api.example.com/tools/a").unwrap(),
            )
            .is_ok()
        );
        assert!(
            ensure_allowed_host(
                &manifest(&["api.example.com"]),
                &reqwest::Url::parse("https://evil.example/tools/a").unwrap(),
            )
            .is_err()
        );
    }

    #[test]
    fn workspace_paths_cannot_escape_or_follow_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        assert!(resolve_workspace_path(&workspace, "../secret", true).is_err());
        assert!(resolve_workspace_path(&workspace, "nested/file.txt", true).is_ok());
    }
}
