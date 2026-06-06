use anyhow::Context;
use reqwest::header::{AUTHORIZATION, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component as PathComponent, Path, PathBuf},
    time::Duration,
};
use wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, Linker},
};

use crate::{
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
            data-read: func(path: string) -> result<string, string>;
            data-write: func(path: string, content: string) -> result<_, string>;
        }

        world plugin {
            import host;
            export call: func(tool: string, input-json: string) -> result<string, string>;
        }
    "#,
});

const WASM_MEMORY_LIMIT: usize = 64 * 1024 * 1024;
const WASM_FUEL_LIMIT: u64 = 20_000_000;
const WASM_TIMEOUT: Duration = Duration::from_secs(5);
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
        let mut definitions = Vec::new();
        let mut tools = HashMap::new();
        for plugin in plugins.into_iter().filter(|plugin| plugin.enabled) {
            for tool in &plugin.tools.tools {
                if tools.contains_key(&tool.name) {
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
        call: &ToolCall,
    ) -> anyhow::Result<String> {
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

struct WasmState {
    limits: StoreLimits,
    permissions: HashSet<String>,
    workspace: PathBuf,
    allowed_hosts: Vec<String>,
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
    tokio::task::spawn_blocking(move || {
        run_wasm_component(
            &wasm_path,
            &tool,
            &input,
            workspace,
            permissions,
            allowed_hosts,
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
) -> anyhow::Result<String> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.consume_fuel(true);
    config.epoch_interruption(true);
    let engine = Engine::new(&config)?;
    let component = Component::from_file(&engine, wasm_path)
        .with_context(|| format!("failed to load {}", wasm_path.display()))?;
    let mut linker = Linker::new(&engine);
    Plugin::add_to_linker(&mut linker, |state: &mut WasmState| state)?;
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
        .context("WASM tool call trapped")?
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
}

#[derive(Serialize)]
struct WasmHttpResponse {
    status: u16,
    headers: HashMap<String, String>,
    body: String,
}

impl helpcore::plugin::host::Host for WasmState {
    fn http_request(&mut self, request_json: String) -> Result<String, String> {
        self.require("outbound_http")?;
        let request: WasmHttpRequest =
            serde_json::from_str(&request_json).map_err(|error| error.to_string())?;
        let url = reqwest::Url::parse(&request.url).map_err(|error| error.to_string())?;
        ensure_host_allowed(&self.allowed_hosts, &url).map_err(|error| error.to_string())?;
        let client = reqwest::blocking::Client::builder()
            .timeout(BRIDGE_TIMEOUT)
            .build()
            .map_err(|error| error.to_string())?;
        let method = reqwest::Method::from_bytes(request.method.as_bytes())
            .map_err(|error| error.to_string())?;
        let mut builder = client.request(method, url);
        for (name, value) in request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let response = builder.send().map_err(|error| error.to_string())?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_TOOL_RESULT_BYTES as u64)
        {
            return Err("HTTP response exceeds the 1 MiB limit".into());
        }
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

    fn data_read(&mut self, path: String) -> Result<String, String> {
        self.require("user_data_read")?;
        let path = resolve_workspace_path(&self.workspace, &path, false)
            .map_err(|error| error.to_string())?;
        fs::read_to_string(path).map_err(|error| error.to_string())
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

fn ensure_host_allowed(allowed_hosts: &[String], url: &reqwest::Url) -> anyhow::Result<()> {
    let host = url.host_str().context("bridge endpoint has no host")?;
    if allowed_hosts.iter().any(|allowed| {
        allowed == host
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
            min_core_version: None,
            bridge: None,
            allowed_hosts: hosts.iter().map(|host| host.to_string()).collect(),
            config_schema: Vec::new(),
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
