//! Plugin tool execution: built-in tools, WASM sandbox, and bridge HTTP dispatch.

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

/// WASM component model bindings generated from the helpcore:plugin WIT world.
mod wasm_bindings {
    #![allow(missing_docs)]
    wasmtime::component::bindgen!({
        inline: r#"
            package helpcore:plugin;

            interface host {
                http-request: func(request-json: string) -> result<string, string>;
                http-request-binary: func(request-json: string) -> result<string, string>;
                data-read: func(path: string) -> result<string, string>;
                data-write: func(path: string, content: string) -> result<_, string>;
                config-read: func(key: string) -> result<string, string>;
                secret-read: func(key: string) -> result<string, string>;
            }

            world plugin {
                import host;
                export call: func(tool: string, input-json: string) -> result<string, string>;
            }
        "#,
    });
}
use wasm_bindings::*;

const WASM_MEMORY_LIMIT: usize = 64 * 1024 * 1024;
const WASM_FUEL_LIMIT: u64 = 1_000_000_000;
const WASM_TIMEOUT: Duration = Duration::from_secs(30);
const BRIDGE_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_TOOL_RESULT_BYTES: usize = 4 * 1024 * 1024;
const SANDBOX_ERROR_OUTPUT_CHARS: usize = 2_000;

#[derive(Clone)]
struct RuntimeTool {
    plugin: InstalledPlugin,
}

/// Loads and dispatches tool calls to built-in handlers or WASM/bridge plugins.
pub struct ToolCatalog {
    definitions: Vec<ToolDefinition>,
    tools: HashMap<String, RuntimeTool>,
}

impl ToolCatalog {
    /// Loads all enabled plugins for a user and builds the tool catalog.
    pub async fn load(state: &AppState, user_id: &str) -> anyhow::Result<Self> {
        let user_id = user_id.to_string();
        let plugins = state
            .db
            .call(move |conn| crate::plugins::registry::list_enabled(conn, &user_id))
            .await?;
        let mut definitions = builtin_tool_definitions();
        // Don't offer the sandbox tools when the sandbox is disabled or
        // Docker is unreachable — advertising tools that can only fail wastes
        // context and invites doomed calls. The names stay reserved (plugins
        // still can't shadow them) and the handlers keep their own guard.
        if state.sandbox.is_none() {
            definitions.retain(|definition| !definition.name.starts_with("sandbox_"));
        }
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

    /// Returns the tool definitions visible to the model.
    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    /// Dispatches a tool call to the appropriate runtime (built-in, WASM, or bridge).
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
    "skill_read",
    "tool_result_read",
    "request_user_input",
    "request_plugin_action",
    "sandbox_list",
    "sandbox_read",
    "sandbox_write",
    "sandbox_edit",
    "sandbox_search",
    "sandbox_exec",
    "sandbox_ps",
    "sandbox_logs",
    "sandbox_kill",
    "git_commit_push",
];

/// Directory inside the sandbox container holding background process logs and
/// metadata. Lives on the container's own filesystem (not /workspace) so its
/// lifetime matches the processes themselves.
const SANDBOX_PROC_DIR: &str = "/tmp/helpcore-proc";

pub(crate) fn is_builtin_tool(name: &str) -> bool {
    BUILTIN_TOOL_NAMES.contains(&name)
}

/// Returns true for tools that pause generation for a user response.
pub(crate) fn is_interactive_tool(name: &str) -> bool {
    matches!(name, "request_user_input" | "request_plugin_action")
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
        ToolDefinition {
            name: "tool_result_read".into(),
            description: "Continue reading a tool result that HelpCore paginated because it was \
                too large for one model-context response. Use the continuation_token returned \
                by that tool result, then follow next_start_line and next_start_column while \
                truncated is true. The token is valid only during the current assistant turn."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "continuation_token": {
                        "type": "string",
                        "description": "Opaque token from a truncated tool result"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "First line to read (1-based, default 1)",
                        "minimum": 1
                    },
                    "start_column": {
                        "type": "integer",
                        "description": "Character column within start_line (1-based, default 1). Use the returned next_start_column when continuing within one long line.",
                        "minimum": 1
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Last line to read (1-based, inclusive). Defaults to a bounded window and is capped at 200 lines.",
                        "minimum": 1
                    }
                },
                "required": ["continuation_token"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "request_user_input".into(),
            description: "Pause and ask the user one to three decision-relevant questions. \
                Each question must declare `single_select`, `multi_select`, or `text`. \
                Select questions require two or three meaningful options, and every option \
                must include a description that explains its effect or tradeoff. The interface \
                also permits a custom typed response to select questions. \
                Call this tool alone in its tool round. Do not use it for plugin installation \
                or enablement; use request_plugin_action for those."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "questions": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 3,
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "header": { "type": "string" },
                                "question": { "type": "string" },
                                "question_type": {
                                    "type": "string",
                                    "enum": ["single_select", "multi_select", "text"]
                                },
                                "options": {
                                    "type": "array",
                                    "minItems": 2,
                                    "maxItems": 3,
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "id": { "type": "string" },
                                            "label": { "type": "string" },
                                            "description": { "type": "string" }
                                        },
                                        "required": ["id", "label", "description"],
                                        "additionalProperties": false
                                    }
                                }
                            },
                            "required": ["id", "header", "question", "question_type"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["questions"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "request_plugin_action".into(),
            description: "Propose installing, configuring, or enabling one materially useful \
                plugin from the plugin catalog. The server determines the required lifecycle \
                steps and asks for explicit user approval. Call this tool alone in its tool \
                round. Never call it for enabled, blocked, unavailable, or server-managed plugins."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "plugin_id": {
                        "type": "string",
                        "description": "Exact plugin id from the plugin catalog"
                    },
                    "rationale": {
                        "type": "string",
                        "description": "Concise explanation of how this plugin helps the current task"
                    }
                },
                "required": ["plugin_id", "rationale"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "sandbox_list".into(),
            description: "List files and directories in the sandbox workspace. \
                          Returns one path per line, sorted. \
                          Use the optional depth to control how deep to recurse (default 2)."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory or file path relative to /workspace, e.g. 'src/' or '.'"
                    },
                    "depth": {
                        "type": "integer",
                        "description": "Maximum recursion depth (default 2, max 5)",
                        "minimum": 1,
                        "maximum": 5
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "sandbox_read".into(),
            description: "Read a file from the sandbox workspace. \
                          Each line is prefixed with its line number (like cat -n), \
                          and the file's total line count is returned alongside. \
                          Reads up to 2000 lines from start_line by default; when the \
                          file is longer, the response includes a hint with the \
                          start_line to continue from. Lines longer than 2000 \
                          characters are clipped. \
                          If the path does not exist, the error suggests similarly \
                          named files in the workspace."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to /workspace, e.g. 'src/main.rs'"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "First line to read (1-based, default 1)",
                        "minimum": 1
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Last line to read (1-based, inclusive). Omit or set high to read to EOF.",
                        "minimum": 1
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "sandbox_write".into(),
            description: "Write content to a file in the sandbox workspace. \
                          Creates parent directories automatically. \
                          Replaces the file if it already exists."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to /workspace, e.g. 'src/main.rs'"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full file content to write"
                    }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "sandbox_edit".into(),
            description: "Perform a surgical find-and-replace in a file in the sandbox workspace. \
                          'old' must identify ONE place in the file: if it matches several, \
                          the call fails with the matching line numbers — add surrounding \
                          lines to 'old' to pinpoint one, or set replace_all to change every \
                          occurrence. Matching tolerates small whitespace/indentation \
                          differences (the response reports which match_strategy was used; \
                          'exact' means a verbatim match). \
                          Prefer this over sandbox_write for small changes. \
                          Returns the line number and post-edit context so you can verify \
                          the change. If the pattern is not found, the error lists the \
                          closest near-matches so you can correct it."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to /workspace, e.g. 'src/main.rs'"
                    },
                    "old": {
                        "type": "string",
                        "description": "Exact text to find and replace"
                    },
                    "new": {
                        "type": "string",
                        "description": "Replacement text"
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "Replace all occurrences instead of just the first (default false)"
                    }
                },
                "required": ["path", "old", "new"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "sandbox_search".into(),
            description: "Search for a regex pattern in the sandbox workspace \
                          (ripgrep, falling back to grep; .gitignore'd files are \
                          skipped when ripgrep is available). \
                          Returns matching lines in <file>:<line>:<text> format. \
                          Use an optional path or glob filter to narrow the search scope. \
                          Supports context lines, case-insensitive search, and a result cap."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional directory or file to limit search scope, e.g. 'src/'"
                    },
                    "glob": {
                        "type": "string",
                        "description": "Optional filename glob to filter results, e.g. '*.rs'"
                    },
                    "context": {
                        "type": "integer",
                        "description": "Number of context lines around each match (default 0, max 10)",
                        "minimum": 0,
                        "maximum": 10
                    },
                    "case_insensitive": {
                        "type": "boolean",
                        "description": "Case-insensitive search (default false)"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum result lines (default 200, max 500)",
                        "minimum": 1,
                        "maximum": 500
                    }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "sandbox_exec".into(),
            description: "Execute a shell command in the sandbox: a persistent Linux \
                          development workspace. Network access is intended for git \
                          and package-manager operations, not general web searches or \
                          business lookups. /workspace is on a persistent \
                          volume and survives across commands; toolchain caches \
                          (cargo, pip, npm) persist under /workspace/.cache, so \
                          repeated builds are incremental. Use this for builds, \
                          tests, local service checks, git operations, and installs. \
                          Set background=true \
                          to start a long-running process (e.g. a dev server) and \
                          manage it with sandbox_ps / sandbox_logs / sandbox_kill."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to run in the sandbox"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Max execution time in seconds (default 120, max 600)",
                        "minimum": 1,
                        "maximum": 600
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Working directory relative to /workspace, e.g. 'myrepo' (default: /workspace)"
                    },
                    "env": {
                        "type": "object",
                        "description": "Extra environment variables for this command, e.g. {\"RUST_BACKTRACE\": \"1\"}",
                        "additionalProperties": { "type": "string" }
                    },
                    "background": {
                        "type": "boolean",
                        "description": "Start the command as a background process and return immediately with a process_id (default false)"
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "sandbox_ps".into(),
            description: "List background processes started with sandbox_exec \
                          (background=true): process id, pid, running/exited status, \
                          start time, and command."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "sandbox_logs".into(),
            description: "Show the captured output (stdout and stderr combined) of a \
                          background process started with sandbox_exec, plus whether \
                          it is still running. Returns the last N lines."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "process_id": {
                        "type": "string",
                        "description": "Process id returned by sandbox_exec with background=true"
                    },
                    "lines": {
                        "type": "integer",
                        "description": "Number of trailing log lines to return (default 100, max 2000)",
                        "minimum": 1,
                        "maximum": 2000
                    }
                },
                "required": ["process_id"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "sandbox_kill".into(),
            description: "Stop a background process started with sandbox_exec. Sends \
                          SIGTERM to the process group, waits a few seconds, then \
                          SIGKILLs if it is still alive. The process log remains \
                          readable with sandbox_logs afterwards."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "process_id": {
                        "type": "string",
                        "description": "Process id returned by sandbox_exec with background=true"
                    }
                },
                "required": ["process_id"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: "git_commit_push".into(),
            description: "Stage all changes, commit with a message, and push to the \\\\
                          remote repository. The repo path defaults to the current \\\\
                          workspace root. Returns the commit output."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "Commit message"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional path to the git repository directory relative to /workspace (defaults to .)"
                    }
                },
                "required": ["message"],
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

use crate::sandbox::shell_escape;

/// Validates that a path does not traverse outside `/workspace` and
/// returns a normalized absolute workspace path.
/// Uses [`Path::components`] to normalize `.`, `//`, and detect `..`.
fn validate_workspace_path(path: &str) -> anyhow::Result<String> {
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Ok("/workspace".to_string());
    }
    let mut result = String::from("/workspace");
    for component in std::path::Path::new(trimmed).components() {
        match component {
            std::path::Component::Normal(seg) => {
                result.push('/');
                result.push_str(&seg.to_string_lossy());
            }
            std::path::Component::ParentDir => {
                anyhow::bail!("path must not contain '..': {path}");
            }
            std::path::Component::CurDir => {
                // skip `.`
            }
            _ => {
                // RootDir, Prefix — should not appear in a relative path
            }
        }
    }
    Ok(result)
}

/// Parses the optional `env` object argument into KEY=VALUE pairs, rejecting
/// names that are not valid environment variable identifiers.
fn parse_env_arg(arguments: &serde_json::Value) -> anyhow::Result<Vec<String>> {
    let Some(env) = arguments.get("env") else {
        return Ok(Vec::new());
    };
    let env = env
        .as_object()
        .context("'env' must be an object of string values")?;
    let mut pairs = Vec::with_capacity(env.len());
    for (key, value) in env {
        let valid = !key.is_empty()
            && !key.starts_with(|c: char| c.is_ascii_digit())
            && key.chars().all(|c| c == '_' || c.is_ascii_alphanumeric());
        if !valid {
            anyhow::bail!("invalid environment variable name '{key}'");
        }
        let value = value
            .as_str()
            .with_context(|| format!("env value for '{key}' must be a string"))?;
        pairs.push(format!("{key}={value}"));
    }
    Ok(pairs)
}

fn sandbox_exec_result(result: &crate::sandbox::SandboxResult) -> anyhow::Result<String> {
    if result.exit_code != 0 {
        let stdout = result
            .stdout
            .trim()
            .chars()
            .take(SANDBOX_ERROR_OUTPUT_CHARS)
            .collect::<String>();
        let stderr = result
            .stderr
            .trim()
            .chars()
            .take(SANDBOX_ERROR_OUTPUT_CHARS)
            .collect::<String>();
        anyhow::bail!(
            "command exited with status {}\nstdout:\n{}\nstderr:\n{}",
            result.exit_code,
            stdout,
            stderr
        );
    }
    Ok(serde_json::to_string(result)?)
}

/// Validates a background process id (as returned by `sandbox_exec` with
/// `background=true`) so it can be safely interpolated into shell commands.
fn validate_process_id(id: &str) -> anyhow::Result<&str> {
    if id.is_empty() || id.len() > 16 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        anyhow::bail!("invalid process id '{id}'");
    }
    Ok(id)
}

/// Maximum file size sandbox_edit will read and rewrite. Larger files should
/// be modified with sandbox_exec instead.
const EDIT_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Lines returned by sandbox_read when no end_line is given. The response
/// carries an explicit continuation hint when the file is longer.
const READ_DEFAULT_LINES: u64 = 2000;

/// Builds up to three numbered context blocks around lines that nearly match
/// the first meaningful line of `pattern`. Used to enrich "pattern not found"
/// errors so the caller can correct its pattern in one step.
fn near_match_report(content: &str, pattern: &str) -> Option<String> {
    let probe = pattern.lines().map(str::trim).find(|l| !l.is_empty())?;
    let lines: Vec<&str> = content.split('\n').collect();
    let mut hits: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains(probe))
        .map(|(i, _)| i)
        .take(3)
        .collect();
    if hits.is_empty() {
        // Retry with collapsed whitespace, the most common near-miss.
        let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        let probe = normalize(probe);
        if !probe.is_empty() {
            hits = lines
                .iter()
                .enumerate()
                .filter(|(_, line)| normalize(line).contains(&probe))
                .map(|(i, _)| i)
                .take(3)
                .collect();
        }
    }
    if hits.is_empty() {
        return None;
    }
    let blocks: Vec<String> = hits
        .iter()
        .map(|&i| {
            let start = i.saturating_sub(2);
            let end = (i + 3).min(lines.len());
            (start..end)
                .map(|j| format!("{}: {}", j + 1, lines[j]))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect();
    Some(blocks.join("\n---\n"))
}

/// Returns the 1-based line number of byte offset `idx` in `content`, plus a
/// numbered context block of the surrounding lines.
fn edit_context(content: &str, idx: usize) -> (usize, String) {
    let line = content[..idx].matches('\n').count() + 1;
    let lines: Vec<&str> = content.split('\n').collect();
    let start = line.saturating_sub(3);
    let end = (line + 3).min(lines.len());
    let context = (start..end)
        .map(|i| format!("{}: {}", i + 1, lines[i]))
        .collect::<Vec<_>>()
        .join("\n");
    (line, context)
}

/// Starts a command as a detached background process in the sandbox and
/// returns a JSON payload with its process id.
async fn start_background_process(
    sandbox: &crate::sandbox::SandboxState,
    command: &str,
    cwd: Option<String>,
    env: Vec<String>,
) -> anyhow::Result<String> {
    let id = format!("{:08x}", rand::random::<u32>());
    let escaped_cmd = shell_escape(command);
    let cmd_b64 = base64::engine::general_purpose::STANDARD.encode(command);
    // setsid detaches the process into its own session (and process group, so
    // sandbox_kill can signal the whole tree); the meta file records pid,
    // start time, and the command for sandbox_ps. mkdir must be separated
    // with ';' — with '&&' it would become part of the backgrounded list and
    // race the foreground meta-file write.
    let launcher = format!(
        "mkdir -p {dir} || exit 1; \
         setsid /bin/sh -c {escaped_cmd} > {dir}/{id}.log 2>&1 < /dev/null & \
         PID=$!; \
         printf '%s\\n%s\\n%s\\n' \"$PID\" \"$(date -Iseconds)\" '{cmd_b64}' > {dir}/{id}.meta; \
         echo \"$PID\"",
        dir = SANDBOX_PROC_DIR,
    );
    let result = crate::sandbox::exec_with(
        sandbox,
        &launcher,
        crate::sandbox::ExecOptions {
            timeout_secs: Some(30),
            cwd,
            env,
            ..Default::default()
        },
    )
    .await?;
    if result.exit_code != 0 {
        anyhow::bail!(
            "failed to start background process: {}",
            result.stderr.trim()
        );
    }
    let pid = result.stdout.trim().to_string();
    Ok(serde_json::json!({
        "action": "background_start",
        "process_id": id,
        "pid": pid,
        "message": format!(
            "Process started in the background. Check output with sandbox_logs \
             (process_id '{id}'), list processes with sandbox_ps, stop it with sandbox_kill."
        )
    })
    .to_string())
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
        "tool_result_read" => {
            anyhow::bail!("tool_result_read is only available during an active tool-use turn")
        }
        "request_user_input" | "request_plugin_action" => {
            anyhow::bail!("interactive tools must be handled by the conversation loop")
        }
        "sandbox_list" => {
            let sandbox = state
                .sandbox
                .as_ref()
                .context("sandbox is not enabled (set sandbox.enabled = true in config.toml) or Docker is unavailable")?;
            let path = call
                .arguments
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(".");
            let depth = call
                .arguments
                .get("depth")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(2)
                .clamp(1, 5);
            let workspace_path = validate_workspace_path(path)?;
            let escaped = shell_escape(&workspace_path);
            let cmd = format!(
                "find {escaped} -maxdepth {depth} -not -path '*/.git/*' 2>/dev/null | sort | head -500"
            );
            let result = crate::sandbox::exec(sandbox, &cmd, None).await?;
            Ok(serde_json::json!({ "files": result.stdout }).to_string())
        }
        "sandbox_read" => {
            let sandbox = state
                .sandbox
                .as_ref()
                .context("sandbox is not enabled (set sandbox.enabled = true in config.toml) or Docker is unavailable")?;
            let path = require_str_arg(&call.arguments, "path")?;
            let workspace_path = validate_workspace_path(path)?;
            let escaped = shell_escape(&workspace_path);

            let start_line = call
                .arguments
                .get("start_line")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1)
                .max(1);
            // Without an explicit end_line, read a capped window rather than
            // the whole file: an unbounded read of a big file would blow the
            // output cap and silently drop the middle.
            let end_line = call
                .arguments
                .get("end_line")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(start_line + READ_DEFAULT_LINES - 1)
                .max(start_line);

            // First output line is the file's total line count; the rest is
            // the requested range with `<number>\t<line>` formatting. Very
            // long lines are clipped so one minified file can't eat the
            // whole output budget.
            let cmd = format!(
                "test -f {escaped} || {{ echo 'not a regular file' >&2; exit 1; }}; \
                 wc -l < {escaped}; \
                 awk -v s={start_line} -v e={end_line} 'NR>=s && NR<=e {{ \
                     line=$0; \
                     if (length(line) > 2000) line = substr(line, 1, 2000) \"... (line truncated)\"; \
                     printf \"%d\\t%s\\n\", NR, line \
                 }}' {escaped}"
            );

            let result = crate::sandbox::exec(sandbox, &cmd, None).await?;
            if result.exit_code != 0 {
                // Suggest similarly named files before giving up — a wrong
                // directory prefix is the most common mistake.
                let basename = std::path::Path::new(&workspace_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let mut suggestion = String::new();
                if !basename.is_empty() {
                    let find_cmd = format!(
                        "find /workspace -maxdepth 8 -iname {} -not -path '*/.git/*' 2>/dev/null \
                         | sed 's|^/workspace/||' | head -5",
                        shell_escape(&basename)
                    );
                    if let Ok(found) = crate::sandbox::exec(sandbox, &find_cmd, Some(30)).await {
                        let hits = found.stdout.trim();
                        if !hits.is_empty() {
                            suggestion = format!("\nDid you mean one of these?\n{hits}");
                        }
                    }
                }
                anyhow::bail!("file not found: {path}{suggestion}");
            }
            let (total, content) = result
                .stdout
                .split_once('\n')
                .unwrap_or((result.stdout.trim(), ""));
            let total_lines = total.trim().parse::<u64>().unwrap_or(0);
            let shown_end = end_line.min(total_lines);
            let mut response = serde_json::json!({
                "content": content,
                "total_lines": total_lines,
                "truncated": result.truncated
            });
            if total_lines > shown_end {
                response["hint"] = serde_json::Value::String(format!(
                    "Showing lines {start_line}-{shown_end} of {total_lines}. \
                     Continue with start_line={}.",
                    shown_end + 1
                ));
            }
            Ok(response.to_string())
        }
        "sandbox_write" => {
            let sandbox = state
                .sandbox
                .as_ref()
                .context("sandbox is not enabled (set sandbox.enabled = true in config.toml) or Docker is unavailable")?;
            let path = require_str_arg(&call.arguments, "path")?;
            let content = require_str_arg(&call.arguments, "content")?;
            let workspace_path = validate_workspace_path(path)?;
            let escaped = shell_escape(&workspace_path);
            let parent = std::path::Path::new(&workspace_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "/workspace".to_string());
            let escaped_dir = shell_escape(&parent);
            // Content travels via stdin: embedding it in the command string
            // would hit the kernel's per-argument size limit on larger files.
            let cmd = format!(
                "if test -f {escaped}; then echo 'EXISTS'; else echo 'NEW'; fi && mkdir -p {escaped_dir} && cat > {escaped}"
            );
            let result = crate::sandbox::exec_with(
                sandbox,
                &cmd,
                crate::sandbox::ExecOptions {
                    stdin: Some(content.as_bytes().to_vec()),
                    ..Default::default()
                },
            )
            .await?;
            if result.exit_code != 0 {
                anyhow::bail!("failed to write {path}: {}", result.stderr.trim());
            }
            let action = if result.stdout.starts_with("EXISTS") {
                "overwritten"
            } else {
                "created"
            };
            Ok(serde_json::json!({ action: path }).to_string())
        }
        "sandbox_edit" => {
            let sandbox = state
                .sandbox
                .as_ref()
                .context("sandbox is not enabled (set sandbox.enabled = true in config.toml) or Docker is unavailable")?;
            let path = require_str_arg(&call.arguments, "path")?;
            let old = require_str_arg(&call.arguments, "old")?;
            let new = require_str_arg(&call.arguments, "new")?;
            let replace_all = call
                .arguments
                .get("replace_all")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let workspace_path = validate_workspace_path(path)?;
            let escaped = shell_escape(&workspace_path);

            // Read the file, do the replacement server-side, write it back
            // via stdin. Files are treated as UTF-8 text.
            let read = crate::sandbox::exec_with(
                sandbox,
                &format!("cat {escaped}"),
                crate::sandbox::ExecOptions {
                    output_limit: Some(EDIT_MAX_BYTES),
                    ..Default::default()
                },
            )
            .await?;
            if read.exit_code != 0 {
                anyhow::bail!("file not found: {path}\n{}", read.stderr.trim());
            }
            if read.truncated {
                anyhow::bail!(
                    "{path} is larger than {EDIT_MAX_BYTES} bytes — edit it with sandbox_exec instead"
                );
            }
            let content = read.stdout;

            let Some(matches) = crate::plugins::edit::find_matches(&content, old) else {
                match near_match_report(&content, old) {
                    Some(report) => {
                        anyhow::bail!("pattern not found in {path}; closest matches:\n{report}")
                    }
                    None => anyhow::bail!("pattern not found in {path}"),
                }
            };
            // Never guess between occurrences: editing the wrong one corrupts
            // the file silently.
            if !replace_all && matches.ranges.len() > 1 {
                let lines: Vec<String> = matches
                    .ranges
                    .iter()
                    .take(5)
                    .map(|range| (content[..range.start].matches('\n').count() + 1).to_string())
                    .collect();
                anyhow::bail!(
                    "pattern matches {} locations in {path} (lines {}). Add more \
                     surrounding context to 'old' to pinpoint one occurrence, or \
                     set replace_all to change every occurrence.",
                    matches.ranges.len(),
                    lines.join(", ")
                );
            }
            let ranges = if replace_all {
                matches.ranges.as_slice()
            } else {
                &matches.ranges[..1]
            };
            let new_content = crate::plugins::edit::apply_replacements(&content, ranges, new);

            let write = crate::sandbox::exec_with(
                sandbox,
                &format!("cat > {escaped}"),
                crate::sandbox::ExecOptions {
                    stdin: Some(new_content.clone().into_bytes()),
                    ..Default::default()
                },
            )
            .await?;
            if write.exit_code != 0 {
                anyhow::bail!("failed to write {path}: {}", write.stderr.trim());
            }

            // Show the post-edit state so the result doubles as verification.
            let (line, context) = edit_context(&new_content, ranges[0].start);
            Ok(serde_json::json!({
                "replaced": path,
                "line": line,
                "replacements": ranges.len(),
                "match_strategy": matches.strategy,
                "context": context
            })
            .to_string())
        }
        "sandbox_search" => {
            let sandbox = state
                .sandbox
                .as_ref()
                .context("sandbox is not enabled (set sandbox.enabled = true in config.toml) or Docker is unavailable")?;
            let pattern = require_str_arg(&call.arguments, "pattern")?;
            let escaped_pattern = shell_escape(pattern);
            let case_insensitive = call
                .arguments
                .get("case_insensitive")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let context = call
                .arguments
                .get("context")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
                .clamp(0, 10);
            let max_results = call
                .arguments
                .get("max_results")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(200)
                .clamp(1, 500);
            let search_path = call
                .arguments
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(".");
            let workspace_search = validate_workspace_path(search_path)?;
            let escaped_search = shell_escape(&workspace_search);
            let glob = call
                .arguments
                .get("glob")
                .and_then(serde_json::Value::as_str);

            let case_flag = if case_insensitive { "-i " } else { "" };
            let context_flag = if context > 0 {
                format!("-C {context} ")
            } else {
                String::new()
            };
            let rg_glob = glob
                .map(|g| format!("-g {} ", shell_escape(g)))
                .unwrap_or_default();
            let grep_glob = glob
                .map(|g| format!("--include={} ", shell_escape(g)))
                .unwrap_or_default();
            // Prefer ripgrep (faster, skips .gitignore'd files like target/);
            // fall back to grep for images without it.
            let cmd = format!(
                "if command -v rg >/dev/null 2>&1; then \
                     OUT=$(rg -n --no-heading -H {case_flag}{context_flag}{rg_glob}{escaped_pattern} {escaped_search} 2>/dev/null | head -n {max_results}); \
                 else \
                     OUT=$(grep -rn {case_flag}{context_flag}{grep_glob}{escaped_pattern} {escaped_search} 2>/dev/null | head -n {max_results}); \
                 fi; \
                 if [ -n \"$OUT\" ]; then echo \"$OUT\"; else echo 'no matches'; fi"
            );
            let result = crate::sandbox::exec(sandbox, &cmd, None).await?;
            Ok(serde_json::json!({ "matches": result.stdout }).to_string())
        }
        "sandbox_exec" => {
            let sandbox = state
                .sandbox
                .as_ref()
                .context("sandbox is not enabled (set sandbox.enabled = true in config.toml) or Docker is unavailable")?;
            let command = require_str_arg(&call.arguments, "command")?;
            let timeout = call.arguments.get("timeout").and_then(|v| v.as_u64());
            let background = call
                .arguments
                .get("background")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let cwd = call
                .arguments
                .get("cwd")
                .and_then(serde_json::Value::as_str)
                .map(validate_workspace_path)
                .transpose()?;
            let env = parse_env_arg(&call.arguments)?;

            if background {
                return start_background_process(sandbox, command, cwd, env).await;
            }

            let result = crate::sandbox::exec_with(
                sandbox,
                command,
                crate::sandbox::ExecOptions {
                    timeout_secs: timeout,
                    cwd,
                    env,
                    ..Default::default()
                },
            )
            .await?;
            sandbox_exec_result(&result)
        }
        "sandbox_ps" => {
            let sandbox = state
                .sandbox
                .as_ref()
                .context("sandbox is not enabled (set sandbox.enabled = true in config.toml) or Docker is unavailable")?;
            let cmd = format!(
                "if [ -d {dir} ]; then \
                     for m in {dir}/*.meta; do \
                         [ -e \"$m\" ] || continue; \
                         id=$(basename \"$m\" .meta); \
                         pid=$(sed -n 1p \"$m\"); \
                         started=$(sed -n 2p \"$m\"); \
                         cmd=$(sed -n 3p \"$m\" | base64 -d | tr '\\n' ' ' | head -c 200); \
                         if kill -0 \"$pid\" 2>/dev/null; then st=running; else st=exited; fi; \
                         printf '%s pid=%s status=%s started=%s cmd=%s\\n' \"$id\" \"$pid\" \"$st\" \"$started\" \"$cmd\"; \
                     done; \
                 fi; \
                 true",
                dir = SANDBOX_PROC_DIR,
            );
            let result = crate::sandbox::exec(sandbox, &cmd, Some(30)).await?;
            let listing = if result.stdout.trim().is_empty() {
                "no background processes".to_string()
            } else {
                result.stdout
            };
            Ok(serde_json::json!({ "processes": listing }).to_string())
        }
        "sandbox_logs" => {
            let sandbox = state
                .sandbox
                .as_ref()
                .context("sandbox is not enabled (set sandbox.enabled = true in config.toml) or Docker is unavailable")?;
            let id = validate_process_id(require_str_arg(&call.arguments, "process_id")?)?;
            let lines = call
                .arguments
                .get("lines")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(100)
                .clamp(1, 2000);
            let cmd = format!(
                "L={dir}/{id}.log; M={dir}/{id}.meta; \
                 if [ ! -f \"$L\" ]; then echo 'no such process: {id}' >&2; exit 1; fi; \
                 PID=$(sed -n 1p \"$M\" 2>/dev/null); \
                 if [ -n \"$PID\" ] && kill -0 \"$PID\" 2>/dev/null; \
                     then echo \"status: running (pid $PID)\"; \
                     else echo 'status: exited'; \
                 fi; \
                 tail -n {lines} \"$L\"",
                dir = SANDBOX_PROC_DIR,
            );
            let result = crate::sandbox::exec(sandbox, &cmd, Some(30)).await?;
            if result.exit_code != 0 {
                anyhow::bail!("{}", result.stderr.trim());
            }
            Ok(
                serde_json::json!({ "logs": result.stdout, "truncated": result.truncated })
                    .to_string(),
            )
        }
        "sandbox_kill" => {
            let sandbox = state
                .sandbox
                .as_ref()
                .context("sandbox is not enabled (set sandbox.enabled = true in config.toml) or Docker is unavailable")?;
            let id = validate_process_id(require_str_arg(&call.arguments, "process_id")?)?;
            // TERM the whole process group (setsid made the process a group
            // leader), then escalate to KILL if it survives the grace period.
            let cmd = format!(
                "M={dir}/{id}.meta; \
                 if [ ! -f \"$M\" ]; then echo 'no such process: {id}' >&2; exit 1; fi; \
                 PID=$(sed -n 1p \"$M\"); \
                 if ! kill -0 \"$PID\" 2>/dev/null; then echo 'already exited'; exit 0; fi; \
                 kill -TERM -- \"-$PID\" 2>/dev/null || kill -TERM \"$PID\" 2>/dev/null; \
                 for i in 1 2 3 4 5; do \
                     sleep 1; \
                     kill -0 \"$PID\" 2>/dev/null || {{ echo terminated; exit 0; }}; \
                 done; \
                 kill -KILL -- \"-$PID\" 2>/dev/null || kill -KILL \"$PID\" 2>/dev/null; \
                 echo killed",
                dir = SANDBOX_PROC_DIR,
            );
            let result = crate::sandbox::exec(sandbox, &cmd, Some(30)).await?;
            if result.exit_code != 0 {
                anyhow::bail!("{}", result.stderr.trim());
            }
            Ok(serde_json::json!({
                "process_id": id,
                "result": result.stdout.trim()
            })
            .to_string())
        }
        "git_commit_push" => {
            let sandbox = state
                .sandbox
                .as_ref()
                .context("sandbox is not enabled (set sandbox.enabled = true in config.toml) or Docker is unavailable")?;
            let message = require_str_arg(&call.arguments, "message")?;
            let repo_path = call
                .arguments
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(".");
            let workspace_repo = validate_workspace_path(repo_path)?;
            let escaped_repo = shell_escape(&workspace_repo);
            let msg_b64 = base64::engine::general_purpose::STANDARD.encode(message);
            let cmd = format!(
                "cd {escaped_repo} && \
                 git add -A 2>&1 && \
                 git diff --cached --quiet 2>&1 && echo 'nothing to commit' && exit 0; \
                 git commit -m \"$(echo '{}' | base64 -d)\" 2>&1 && \
                 git push 2>&1",
                msg_b64
            );
            let result = crate::sandbox::exec(sandbox, &cmd, Some(60)).await?;
            if result.exit_code != 0 {
                anyhow::bail!("git commit/push failed: {}", result.stderr.trim());
            }
            Ok(result.stdout.to_string())
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
    secrets: serde_json::Value,
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
    let mut permissions: HashSet<String> = plugin.permissions.iter().cloned().collect();
    permissions.extend(plugin.manifest.permissions.iter().cloned());
    let allowed_hosts = plugin.manifest.allowed_hosts.clone();

    // Non-secret stored config values, exposed via `config-read` without a permission check.
    let mut plugin_config = plugin.config.clone();
    if let Some(obj) = plugin_config.as_object_mut() {
        obj.remove("_secret_keys");
    }
    // Decrypted secrets are kept separate from `plugin_config` and only reachable through
    // `secret-read`, which is gated on the `read_secrets` permission.
    let mut plugin_secrets = serde_json::Value::Object(serde_json::Map::new());
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
        plugin_secrets = decrypted;
    }

    tokio::task::spawn_blocking(move || {
        run_wasm_component(
            &wasm_path,
            &tool,
            &input,
            WasmSandboxConfig {
                workspace,
                permissions,
                allowed_hosts,
                config: plugin_config,
                secrets: plugin_secrets,
            },
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("WASM plugin task failed: {error}"))?
}

/// Everything needed to build the sandboxed [`WasmState`] for a single tool call,
/// besides the resource `limits` (which are constructed fresh per run).
struct WasmSandboxConfig {
    workspace: PathBuf,
    permissions: HashSet<String>,
    allowed_hosts: Vec<String>,
    config: serde_json::Value,
    secrets: serde_json::Value,
}

fn run_wasm_component(
    wasm_path: &Path,
    tool: &str,
    input: &str,
    sandbox: WasmSandboxConfig,
) -> anyhow::Result<String> {
    let WasmSandboxConfig {
        workspace,
        permissions,
        allowed_hosts,
        config,
        secrets,
    } = sandbox;
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
            config,
            secrets,
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

/// Truncate a JSON response body to fit within `MAX_TOOL_RESULT_BYTES`.
/// For arrays, drops trailing elements until the serialized size fits.
/// For other types, serializes compactly or falls back to raw truncation.
fn truncate_json_body(body: String) -> String {
    if body.len() <= MAX_TOOL_RESULT_BYTES {
        return body;
    }
    // Try compact re-serialization first (removes whitespace, ~20-30% savings)
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) {
        let compact = serde_json::to_string(&value).unwrap_or_default();
        if compact.len() <= MAX_TOOL_RESULT_BYTES {
            return compact;
        }
        // Still too large — if it's an array, drop elements binary-search style
        if let serde_json::Value::Array(arr) = &value {
            let mut lo = 0;
            let mut hi = arr.len();
            while lo + 1 < hi {
                let mid = (lo + hi) / 2;
                let subset = serde_json::Value::Array(arr[..mid].to_vec());
                let serialized = serde_json::to_string(&subset).unwrap_or_default();
                if serialized.len() <= MAX_TOOL_RESULT_BYTES {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let subset = serde_json::Value::Array(arr[..lo].to_vec());
            return serde_json::to_string(&subset).unwrap_or_default();
        }
    }
    // Not JSON or not an array — raw truncation
    body.chars()
        .take(MAX_TOOL_RESULT_BYTES / 4)
        .collect::<String>()
        + "…[TRUNCATED]"
}

impl helpcore::plugin::host::Host for WasmState {
    fn http_request(&mut self, request_json: String) -> Result<String, String> {
        self.require("outbound_http")?;
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
        let body = truncate_json_body(body);
        serde_json::to_string(&WasmHttpResponse {
            status,
            headers,
            body,
        })
        .map_err(|error| error.to_string())
    }

    fn http_request_binary(&mut self, request_json: String) -> Result<String, String> {
        self.require("outbound_http")?;
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
        let body = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let body = if body.len() > MAX_TOOL_RESULT_BYTES {
            body.chars()
                .take(MAX_TOOL_RESULT_BYTES / 4)
                .collect::<String>()
                + "…[TRUNCATED]"
        } else {
            body
        };
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

    fn secret_read(&mut self, key: String) -> Result<String, String> {
        self.require("read_secrets")?;
        match self.secrets.get(&key) {
            Some(serde_json::Value::String(s)) => Ok(s.clone()),
            Some(v) => serde_json::to_string(v).map_err(|e| e.to_string()),
            None => Err(format!("secret '{key}' not found")),
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
    use super::helpcore::plugin::host::Host as _;
    use super::*;

    fn wasm_state(
        permissions: &[&str],
        config: serde_json::Value,
        secrets: serde_json::Value,
    ) -> WasmState {
        WasmState {
            limits: StoreLimitsBuilder::new().build(),
            permissions: permissions.iter().map(|p| p.to_string()).collect(),
            workspace: PathBuf::new(),
            allowed_hosts: Vec::new(),
            config,
            secrets,
        }
    }

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

    #[test]
    fn http_request_requires_outbound_http_permission() {
        let request =
            serde_json::json!({"method": "GET", "url": "https://example.com"}).to_string();

        let mut state = wasm_state(&[], serde_json::json!({}), serde_json::json!({}));
        let error = state.http_request(request.clone()).unwrap_err();
        assert!(error.contains("outbound_http"), "unexpected error: {error}");
        let error = state.http_request_binary(request.clone()).unwrap_err();
        assert!(error.contains("outbound_http"), "unexpected error: {error}");

        // Declaring the permission clears the gate (the request itself may still fail,
        // e.g. because the host is not in `allowed_hosts`, but not on a permission error).
        let mut state = wasm_state(
            &["outbound_http"],
            serde_json::json!({}),
            serde_json::json!({}),
        );
        let error = state.http_request(request).unwrap_err();
        assert!(
            !error.contains("outbound_http"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn config_read_does_not_require_a_permission() {
        let mut state = wasm_state(
            &[],
            serde_json::json!({"endpoint": "https://api.example.com"}),
            serde_json::json!({"api_key": "sekret"}),
        );
        assert_eq!(
            state.config_read("endpoint".into()).unwrap(),
            "https://api.example.com"
        );
        // Secrets are not reachable through `config-read`, regardless of permissions.
        assert!(state.config_read("api_key".into()).is_err());
    }

    #[test]
    fn secret_read_requires_read_secrets_permission() {
        let mut state = wasm_state(
            &[],
            serde_json::json!({}),
            serde_json::json!({"api_key": "sekret"}),
        );
        let error = state.secret_read("api_key".into()).unwrap_err();
        assert!(error.contains("read_secrets"), "unexpected error: {error}");

        let mut state = wasm_state(
            &["read_secrets"],
            serde_json::json!({}),
            serde_json::json!({"api_key": "sekret"}),
        );
        assert_eq!(state.secret_read("api_key".into()).unwrap(), "sekret");
    }

    #[test]
    fn data_read_and_write_require_user_data_permissions() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        fs::create_dir(&workspace).unwrap();

        let mut state = WasmState {
            limits: StoreLimitsBuilder::new().build(),
            permissions: HashSet::new(),
            workspace,
            allowed_hosts: Vec::new(),
            config: serde_json::json!({}),
            secrets: serde_json::json!({}),
        };
        let error = state.data_read("file.txt".into()).unwrap_err();
        assert!(
            error.contains("user_data_read"),
            "unexpected error: {error}"
        );
        let error = state
            .data_write("file.txt".into(), "content".into())
            .unwrap_err();
        assert!(
            error.contains("user_data_write"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn near_match_report_finds_whitespace_variants() {
        let content = "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";
        // Exact substring of a line.
        let report = near_match_report(content, "let x = 1;\nlet y = 2;").unwrap();
        assert!(report.contains("2:     let x = 1;"), "report: {report}");
        // Whitespace-collapsed match.
        let report = near_match_report(content, "let  x  =  1;").unwrap();
        assert!(report.contains("let x = 1;"), "report: {report}");
        // No plausible match.
        assert!(near_match_report(content, "completely absent").is_none());
    }

    #[test]
    fn edit_context_reports_line_and_neighbours() {
        let content = "one\ntwo\nthree\nfour\nfive\nsix\nseven";
        let idx = content.find("four").unwrap();
        let (line, context) = edit_context(content, idx);
        assert_eq!(line, 4);
        assert!(context.contains("4: four"));
        assert!(context.contains("2: two"));
        assert!(context.contains("6: six"));
    }

    #[test]
    fn parse_env_arg_validates_names() {
        let args = serde_json::json!({"env": {"RUST_BACKTRACE": "1", "FOO_2": "bar"}});
        let mut env = parse_env_arg(&args).unwrap();
        env.sort();
        assert_eq!(env, vec!["FOO_2=bar", "RUST_BACKTRACE=1"]);

        assert!(parse_env_arg(&serde_json::json!({"env": {"2BAD": "x"}})).is_err());
        assert!(parse_env_arg(&serde_json::json!({"env": {"A=B": "x"}})).is_err());
        assert!(parse_env_arg(&serde_json::json!({"env": {"OK": 1}})).is_err());
        assert!(parse_env_arg(&serde_json::json!({})).unwrap().is_empty());
    }

    #[test]
    fn process_ids_are_validated() {
        assert!(validate_process_id("a1b2c3d4").is_ok());
        assert!(validate_process_id("").is_err());
        assert!(validate_process_id("../etc").is_err());
        assert!(validate_process_id("abc; rm -rf /").is_err());
        assert!(validate_process_id("aaaaaaaaaaaaaaaaa").is_err());
    }

    #[test]
    fn request_user_input_schema_requires_option_help_and_limits_questions() {
        let definition = builtin_tool_definitions()
            .into_iter()
            .find(|definition| definition.name == "request_user_input")
            .unwrap();
        let questions = &definition.input_schema["properties"]["questions"];
        assert_eq!(questions["minItems"], 1);
        assert_eq!(questions["maxItems"], 3);
        let required = questions["items"]["properties"]["options"]["items"]["required"]
            .as_array()
            .unwrap();
        assert!(required.iter().any(|value| value == "description"));
        assert_eq!(
            questions["items"]["properties"]["question_type"]["enum"],
            serde_json::json!(["single_select", "multi_select", "text"])
        );
    }

    #[test]
    fn tool_result_read_schema_exposes_continuation_coordinates() {
        let definition = builtin_tool_definitions()
            .into_iter()
            .find(|definition| definition.name == "tool_result_read")
            .unwrap();
        let properties = &definition.input_schema["properties"];
        assert_eq!(
            definition.input_schema["required"],
            serde_json::json!(["continuation_token"])
        );
        assert_eq!(properties["start_line"]["minimum"], 1);
        assert_eq!(properties["start_column"]["minimum"], 1);
        assert_eq!(properties["end_line"]["minimum"], 1);
    }

    #[test]
    fn sandbox_exec_nonzero_exit_is_a_tool_failure() {
        let result = crate::sandbox::SandboxResult {
            stdout: "partial output".into(),
            stderr: "curl failed".into(),
            exit_code: 22,
            duration_ms: 10,
            truncated: false,
        };
        let error = sandbox_exec_result(&result).unwrap_err().to_string();
        assert!(error.contains("command exited with status 22"));
        assert!(error.contains("curl failed"));
    }

    #[test]
    fn sandbox_exec_zero_exit_is_serialized() {
        let result = crate::sandbox::SandboxResult {
            stdout: "ok".into(),
            stderr: String::new(),
            exit_code: 0,
            duration_ms: 10,
            truncated: false,
        };
        let output = sandbox_exec_result(&result).unwrap();
        assert!(output.contains("\"exit_code\":0"));
    }
}
