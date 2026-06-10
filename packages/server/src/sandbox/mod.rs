//! Docker sandbox for safe command execution.
//!
//! Commands run inside a single long-lived "session" container via the Docker
//! exec API instead of a fresh container per command, which keeps per-command
//! latency low and lets caches (cargo registry, pip, npm) survive between
//! calls. The container is created lazily on first use, torn down after an
//! idle period, and recreated when its configuration (image, resources)
//! changes. All state lives on a persistent named volume at /workspace.

use bollard::{
    Docker,
    container::{Config, CreateContainerOptions, InspectContainerOptions, RemoveContainerOptions},
    exec::{CreateExecOptions, StartExecOptions, StartExecResults},
    image::CreateImageOptions,
    models::{HostConfig, Mount, MountTypeEnum},
};
use futures_util::StreamExt;
use serde::Serialize;

use crate::config::SandboxGitConfig;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::time::timeout;

const WORKSPACE_VOLUME: &str = "helpcore-sandbox-workspace";
const SESSION_CONTAINER: &str = "helpcore-sandbox-session";

/// Default per-stream output cap. Output is split 40/60 between a verbatim
/// head and a rolling tail, so build/test failures (which appear at the end
/// of output) survive truncation.
const DEFAULT_OUTPUT_BYTES: usize = 100_000;

/// How long a session container may sit unused before the reaper removes it.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// How often the reaper checks for an idle session.
const REAP_INTERVAL: Duration = Duration::from_secs(60);
/// Host-side grace period added to the in-container timeout as a backstop.
const BACKSTOP_GRACE: Duration = Duration::from_secs(15);
/// Hard ceiling on per-command timeouts.
const MAX_TIMEOUT_SECS: u64 = 600;

/// Default environment for the session container. Caches live under the
/// persistent /workspace volume so toolchains stay warm across commands and
/// container recreations.
const SESSION_ENV: &[&str] = &[
    "CARGO_HOME=/workspace/.cache/cargo",
    "PIP_CACHE_DIR=/workspace/.cache/pip",
    "npm_config_cache=/workspace/.cache/npm",
];

/// A live session container plus the configuration it was created with, so
/// config changes can be detected and trigger a recreation.
struct Session {
    container_id: String,
    image: String,
    memory_mb: u64,
    nano_cpus: u64,
    last_used: Instant,
}

/// Shared sandbox state.
///
/// `docker` and `host_info` are immutable once created. `image`, `timeout`,
/// `memory_mb`, and `nano_cpus` are wrapped in atomics / locks so the admin
/// config UI can update them at runtime without restarting the server; the
/// session container is recreated on the next command when image or resource
/// limits change.
#[derive(Clone)]
pub struct SandboxState {
    /// Bollard Docker client.
    pub docker: Docker,
    /// Configured sandbox image (updatable via admin config).
    image: Arc<RwLock<String>>,
    /// Default execution timeout in seconds (updatable via admin config).
    timeout: Arc<AtomicU64>,
    /// Memory limit in MB (updatable via admin config).
    memory_mb: Arc<AtomicU64>,
    /// CPU limit in units of 1e-9 CPUs.
    nano_cpus: Arc<AtomicU64>,
    /// Human-readable Docker connection info for error messages.
    pub host_info: String,
    /// Git identity / credentials applied inside the session container.
    git: Arc<SandboxGitConfig>,
    /// Repositories cloned into /workspace on session start.
    repos: Arc<Vec<String>>,
    /// The current session container, if one is running.
    session: Arc<tokio::sync::Mutex<Option<Session>>>,
    /// Whether the idle reaper task has been spawned.
    reaper_started: Arc<AtomicBool>,
}

impl SandboxState {
    /// Create new sandbox state with the given Docker client and config values.
    pub fn new(docker: Docker, host_info: String, config: &crate::config::SandboxConfig) -> Self {
        Self {
            docker,
            image: Arc::new(RwLock::new(config.image.clone())),
            timeout: Arc::new(AtomicU64::new(config.timeout)),
            memory_mb: Arc::new(AtomicU64::new(config.memory_mb)),
            nano_cpus: Arc::new(AtomicU64::new(cpus_to_nano(config.cpus))),
            host_info,
            git: Arc::new(config.git.clone()),
            repos: Arc::new(config.repos.clone()),
            session: Arc::new(tokio::sync::Mutex::new(None)),
            reaper_started: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Read the current sandbox image.
    pub fn image(&self) -> String {
        self.image.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Read the current timeout in seconds.
    pub fn timeout(&self) -> u64 {
        self.timeout.load(Ordering::Relaxed)
    }

    /// Read the current memory limit in MB.
    pub fn memory_mb(&self) -> u64 {
        self.memory_mb.load(Ordering::Relaxed)
    }

    /// Read the current CPU limit in units of 1e-9 CPUs.
    pub fn nano_cpus(&self) -> u64 {
        self.nano_cpus.load(Ordering::Relaxed)
    }

    /// Update the mutable config fields from an admin config change.
    ///
    /// The session container is not touched here; the next command notices the
    /// changed image / resource limits and recreates it.
    pub fn update_config(&self, image: &str, timeout: u64, memory_mb: u64) {
        *self.image.write().unwrap_or_else(|e| e.into_inner()) = image.to_string();
        self.timeout.store(timeout, Ordering::Relaxed);
        self.memory_mb.store(memory_mb, Ordering::Relaxed);
    }

    /// Returns the id of a running session container that matches the current
    /// configuration, creating or recreating one as needed.
    async fn ensure_session(&self) -> Result<String, SandboxError> {
        let image = self.image();
        let memory_mb = self.memory_mb();
        let nano_cpus = self.nano_cpus();

        let mut guard = self.session.lock().await;

        if let Some(session) = guard.as_mut() {
            let config_matches = session.image == image
                && session.memory_mb == memory_mb
                && session.nano_cpus == nano_cpus;
            if config_matches && self.container_running(&session.container_id).await {
                session.last_used = Instant::now();
                return Ok(session.container_id.clone());
            }
            let stale = guard.take().expect("session checked above");
            self.force_remove(&stale.container_id).await;
        }

        self.ensure_image(&image).await?;

        // A previous server process may have left a session container behind;
        // the fixed name guarantees we never accumulate more than one.
        self.force_remove(SESSION_CONTAINER).await;

        let mut env: Vec<String> = SESSION_ENV.iter().map(|s| s.to_string()).collect();
        if let Some(token) = &self.git.token {
            env.push(format!("GIT_TOKEN={token}"));
            env.push(format!(
                "GIT_TOKEN_USER={}",
                self.git.token_user.as_deref().unwrap_or("x-access-token")
            ));
        }

        let config = Config {
            image: Some(image.clone()),
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            working_dir: Some("/workspace".to_string()),
            env: Some(env),
            host_config: Some(HostConfig {
                readonly_rootfs: Some(false),
                privileged: Some(false),
                // Run an init process as PID 1 so zombie processes from
                // exec'd commands get reaped over the container's lifetime.
                init: Some(true),
                // Resources
                memory: Some((memory_mb * 1024 * 1024) as i64),
                nano_cpus: Some(nano_cpus as i64),
                pids_limit: Some(512),
                // Network
                network_mode: Some("bridge".to_string()),
                // Workspace volume
                mounts: Some(vec![Mount {
                    source: Some(WORKSPACE_VOLUME.to_string()),
                    target: Some("/workspace".to_string()),
                    typ: Some(MountTypeEnum::VOLUME),
                    read_only: Some(false),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        };

        let id: String = self
            .docker
            .create_container(
                Some(CreateContainerOptions {
                    name: SESSION_CONTAINER.to_string(),
                    ..Default::default()
                }),
                config,
            )
            .await
            .map_err(|e| SandboxError::docker("create_container", &self.host_info, e))?
            .id;

        self.docker
            .start_container::<String>(&id, None)
            .await
            .map_err(|e| SandboxError::docker("start_container", &self.host_info, e))?;

        tracing::info!(image = %image, container = %id, "sandbox: session container started");

        if let Some(script) = self.init_script() {
            self.run_session_init(&id, &script).await;
        }

        *guard = Some(Session {
            container_id: id.clone(),
            image,
            memory_mb,
            nano_cpus,
            last_used: Instant::now(),
        });
        drop(guard);

        self.spawn_reaper();
        Ok(id)
    }

    /// Builds the one-time initialization script for a fresh session
    /// container. Returns `None` when nothing is configured.
    fn init_script(&self) -> Option<String> {
        build_init_script(&self.git, &self.repos)
    }

    /// Runs the initialization script in the freshly created session
    /// container. Failures are logged but never block the session: a clone
    /// error shouldn't make the whole sandbox unusable.
    async fn run_session_init(&self, container_id: &str, script: &str) {
        let exec_config = CreateExecOptions {
            cmd: Some(vec!["timeout", "600", "/bin/sh", "-c", script]),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/workspace"),
            ..Default::default()
        };
        let exec_id = match self.docker.create_exec(container_id, exec_config).await {
            Ok(created) => created.id,
            Err(e) => {
                tracing::warn!("sandbox: failed to create init exec: {e}");
                return;
            }
        };
        let started = match self
            .docker
            .start_exec(&exec_id, None::<StartExecOptions>)
            .await
        {
            Ok(started) => started,
            Err(e) => {
                tracing::warn!("sandbox: failed to start init exec: {e}");
                return;
            }
        };
        let mut output_buf = CappedBuffer::new(DEFAULT_OUTPUT_BYTES);
        if let StartExecResults::Attached { mut output, .. } = started {
            let collect = async {
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(
                            bollard::container::LogOutput::StdOut { message }
                            | bollard::container::LogOutput::StdErr { message },
                        ) => output_buf.push(&message),
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            };
            if timeout(Duration::from_secs(600) + BACKSTOP_GRACE, collect)
                .await
                .is_err()
            {
                tracing::warn!("sandbox: session init timed out");
                return;
            }
        }
        match self.docker.inspect_exec(&exec_id).await {
            Ok(inspect) if inspect.exit_code == Some(0) => {
                tracing::info!("sandbox: session init completed");
            }
            Ok(inspect) => {
                tracing::warn!(
                    exit_code = ?inspect.exit_code,
                    output = %output_buf.into_string(),
                    "sandbox: session init finished with errors"
                );
            }
            Err(e) => tracing::warn!("sandbox: failed to inspect init exec: {e}"),
        }
    }

    /// Whether the given container exists and is currently running.
    async fn container_running(&self, id: &str) -> bool {
        self.docker
            .inspect_container(id, None::<InspectContainerOptions>)
            .await
            .ok()
            .and_then(|info| info.state)
            .and_then(|state| state.running)
            .unwrap_or(false)
    }

    /// Remove a container by id or name, ignoring "not found" errors.
    async fn force_remove(&self, id: &str) {
        let options = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        if let Err(e) = self.docker.remove_container(id, Some(options)).await
            && !matches!(
                e,
                bollard::errors::Error::DockerResponseServerError {
                    status_code: 404,
                    ..
                }
            )
        {
            tracing::warn!("sandbox: failed to remove container {id}: {e}");
        }
    }

    /// Tear down the current session container, if any. The next command will
    /// create a fresh one.
    async fn invalidate_session(&self) {
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.take() {
            self.force_remove(&session.container_id).await;
        }
    }

    /// Make sure the configured image is available, pulling it only when it
    /// is not already present locally (locally built images are never pulled).
    async fn ensure_image(&self, image: &str) -> Result<(), SandboxError> {
        if self.docker.inspect_image(image).await.is_ok() {
            return Ok(());
        }
        let mut pull_stream = self.docker.create_image(
            Some(CreateImageOptions {
                from_image: image,
                ..Default::default()
            }),
            None,
            None,
        );
        while let Some(pull_result) = pull_stream.next().await {
            pull_result.map_err(|e| SandboxError::docker("pull_image", &self.host_info, e))?;
        }
        Ok(())
    }

    /// Spawn the background task that removes the session container after it
    /// has been idle for [`IDLE_TIMEOUT`]. Runs once per process.
    fn spawn_reaper(&self) {
        if self.reaper_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let state = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(REAP_INTERVAL).await;
                let mut guard = state.session.lock().await;
                let idle = guard
                    .as_ref()
                    .is_some_and(|s| s.last_used.elapsed() > IDLE_TIMEOUT);
                if idle {
                    let session = guard.take().expect("session checked above");
                    tracing::info!(
                        container = %session.container_id,
                        "sandbox: removing idle session container"
                    );
                    state.force_remove(&session.container_id).await;
                }
            }
        });
    }
}

fn cpus_to_nano(cpus: f64) -> u64 {
    (cpus.max(0.1) * 1_000_000_000.0) as u64
}

/// Escapes a string for use inside single quotes in a POSIX shell command.
/// Replaces embedded `'` with `'\''` and wraps the result in single quotes.
pub(crate) fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Builds the session initialization script: git identity, credential helper,
/// and repository clones. Returns `None` when nothing is configured.
fn build_init_script(git: &SandboxGitConfig, repos: &[String]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(name) = &git.user_name {
        parts.push(format!(
            "git config --global user.name {}",
            shell_escape(name)
        ));
    }
    if let Some(email) = &git.user_email {
        parts.push(format!(
            "git config --global user.email {}",
            shell_escape(email)
        ));
    }
    if git.token.is_some() {
        // The token itself stays in the container environment; the helper
        // just reads it, so nothing secret is written to disk.
        parts.push(
            "git config --global credential.helper \
             '!f() { echo \"username=${GIT_TOKEN_USER}\"; echo \"password=${GIT_TOKEN}\"; }; f'"
                .to_string(),
        );
    }
    if !parts.is_empty() || !repos.is_empty() {
        parts.insert(
            0,
            "git config --global --add safe.directory '*'".to_string(),
        );
    }
    for url in repos {
        let escaped = shell_escape(url);
        parts.push(format!(
            "name=$(basename {escaped} .git); \
             [ -d \"/workspace/$name\" ] || git clone {escaped} \"/workspace/$name\""
        ));
    }
    if parts.is_empty() {
        None
    } else {
        // ';' so one failed step (e.g. an unreachable repo) doesn't stop
        // the rest of the initialization.
        Some(parts.join("; "))
    }
}

/// Options for a single sandboxed command execution.
#[derive(Debug, Default, Clone)]
pub struct ExecOptions {
    /// Max execution time in seconds; defaults to the configured sandbox
    /// timeout, capped at 600.
    pub timeout_secs: Option<u64>,
    /// Working directory inside the container; defaults to /workspace.
    pub cwd: Option<String>,
    /// Extra environment variables as KEY=VALUE pairs.
    pub env: Vec<String>,
    /// Bytes piped to the command's stdin. Use this to transfer file content
    /// instead of embedding it in the command string, which is subject to the
    /// kernel's per-argument size limit.
    pub stdin: Option<Vec<u8>>,
    /// Per-stream output cap in bytes; defaults to 100 kB.
    pub output_limit: Option<usize>,
}

/// Result of a sandboxed command execution.
#[derive(Debug, Serialize)]
pub struct SandboxResult {
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
    /// Process exit code.
    pub exit_code: i64,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
    /// Whether the output was truncated due to size limits.
    pub truncated: bool,
}

/// Errors that can occur during sandbox execution.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// Docker daemon error with operation context.
    #[error("Docker error (host={host}, op={operation}): {source}")]
    Docker {
        /// Which Docker API operation failed (e.g. "pull", "create_exec").
        operation: &'static str,
        /// Human-readable Docker connection info.
        host: String,
        /// The underlying bollard/hyper error.
        source: bollard::errors::Error,
    },
    /// Command exceeded its timeout.
    #[error("Command timed out after {0}s")]
    Timeout(u64),
    /// Other internal errors.
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl SandboxError {
    /// Create a Docker error with operation and host context.
    pub fn docker(operation: &'static str, host: &str, source: bollard::errors::Error) -> Self {
        SandboxError::Docker {
            operation,
            host: host.to_string(),
            source,
        }
    }
}

/// Per-stream output collector that keeps the first `head_bytes` and the
/// last `tail_bytes` of output, dropping the middle when output overflows.
struct CappedBuffer {
    head_bytes: usize,
    tail_bytes: usize,
    head: Vec<u8>,
    tail: Vec<u8>,
    omitted: u64,
}

impl CappedBuffer {
    /// Creates a buffer that keeps 40% of `limit` as head and 60% as tail.
    fn new(limit: usize) -> Self {
        let head_bytes = limit * 2 / 5;
        Self {
            head_bytes,
            tail_bytes: limit.saturating_sub(head_bytes),
            head: Vec::new(),
            tail: Vec::new(),
            omitted: 0,
        }
    }

    fn push(&mut self, data: &[u8]) {
        let head_room = self.head_bytes.saturating_sub(self.head.len());
        let take = head_room.min(data.len());
        self.head.extend_from_slice(&data[..take]);
        let rest = &data[take..];
        if rest.is_empty() {
            return;
        }
        self.tail.extend_from_slice(rest);
        if self.tail.len() > self.tail_bytes {
            let drop = self.tail.len() - self.tail_bytes;
            self.tail.drain(..drop);
            self.omitted += drop as u64;
        }
    }

    fn truncated(&self) -> bool {
        self.omitted > 0
    }

    fn into_string(self) -> String {
        let head = String::from_utf8_lossy(&self.head);
        let tail = String::from_utf8_lossy(&self.tail);
        if self.omitted == 0 {
            format!("{head}{tail}")
        } else {
            format!(
                "{head}\n[... output truncated: {} bytes omitted ...]\n{tail}",
                self.omitted
            )
        }
    }
}

/// Run a shell command in the sandbox.
///
/// Convenience wrapper around [`exec_with`] for callers that only need a
/// command and an optional timeout.
pub async fn exec(
    state: &SandboxState,
    command: &str,
    timeout_secs: Option<u64>,
) -> Result<SandboxResult, SandboxError> {
    exec_with(
        state,
        command,
        ExecOptions {
            timeout_secs,
            ..Default::default()
        },
    )
    .await
}

/// Run a shell command in the sandbox session container.
///
/// The command executes via the Docker exec API inside the long-lived session
/// container (created on demand). Output is collected with head+tail capping,
/// and the timeout is enforced in-container via `timeout(1)` with a host-side
/// backstop that recreates the container if it stops responding.
pub async fn exec_with(
    state: &SandboxState,
    command: &str,
    options: ExecOptions,
) -> Result<SandboxResult, SandboxError> {
    let start = Instant::now();
    let timeout_secs = options
        .timeout_secs
        .unwrap_or(state.timeout())
        .clamp(1, MAX_TIMEOUT_SECS);

    let container_id = state.ensure_session().await?;

    let timeout_str = timeout_secs.to_string();
    let exec_config = CreateExecOptions {
        cmd: Some(vec![
            "timeout",
            timeout_str.as_str(),
            "/bin/sh",
            "-c",
            command,
        ]),
        attach_stdin: Some(options.stdin.is_some()),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        working_dir: Some(options.cwd.as_deref().unwrap_or("/workspace")),
        env: if options.env.is_empty() {
            None
        } else {
            Some(options.env.iter().map(String::as_str).collect())
        },
        ..Default::default()
    };

    // If exec creation fails (e.g. the container died underneath us),
    // recreate the session once and retry.
    let exec_id = match state
        .docker
        .create_exec(&container_id, exec_config.clone())
        .await
    {
        Ok(created) => created.id,
        Err(_) => {
            state.invalidate_session().await;
            let container_id = state.ensure_session().await?;
            state
                .docker
                .create_exec(&container_id, exec_config)
                .await
                .map_err(|e| SandboxError::docker("create_exec", &state.host_info, e))?
                .id
        }
    };

    let started = state
        .docker
        .start_exec(&exec_id, None::<StartExecOptions>)
        .await
        .map_err(|e| SandboxError::docker("start_exec", &state.host_info, e))?;

    let output_limit = options.output_limit.unwrap_or(DEFAULT_OUTPUT_BYTES);
    let mut stdout = CappedBuffer::new(output_limit);
    let mut stderr = CappedBuffer::new(output_limit);

    if let StartExecResults::Attached { mut output, input } = started {
        // Feed stdin from a separate task so a large payload can't deadlock
        // against unread output; dropping the writer closes the stream.
        if let Some(bytes) = options.stdin {
            use tokio::io::AsyncWriteExt;
            let mut input = input;
            tokio::spawn(async move {
                if let Err(e) = input.write_all(&bytes).await {
                    tracing::debug!("sandbox exec stdin write failed: {e}");
                }
                let _ = input.shutdown().await;
            });
        }
        let collect = async {
            while let Some(chunk) = output.next().await {
                match chunk {
                    Ok(bollard::container::LogOutput::StdOut { message }) => {
                        stdout.push(&message);
                    }
                    Ok(bollard::container::LogOutput::StdErr { message }) => {
                        stderr.push(&message);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::debug!("sandbox exec output error: {e}");
                        break;
                    }
                }
            }
        };
        // Backstop: the in-container `timeout` should end the command, but if
        // the container stops responding entirely, recreate it.
        let deadline = Duration::from_secs(timeout_secs) + BACKSTOP_GRACE;
        if timeout(deadline, collect).await.is_err() {
            state.invalidate_session().await;
            return Err(SandboxError::Timeout(timeout_secs));
        }
    }

    let inspect = state
        .docker
        .inspect_exec(&exec_id)
        .await
        .map_err(|e| SandboxError::docker("inspect_exec", &state.host_info, e))?;
    let exit_code = inspect.exit_code.unwrap_or(-1);

    let duration_ms = start.elapsed().as_millis() as u64;

    // GNU timeout exits with 124 when the time limit was hit. Only treat that
    // as a sandbox timeout when the elapsed time backs it up, since the
    // command itself may legitimately exit with 124.
    if exit_code == 124 && duration_ms >= timeout_secs.saturating_mul(1000) {
        return Err(SandboxError::Timeout(timeout_secs));
    }

    {
        let mut guard = state.session.lock().await;
        if let Some(session) = guard.as_mut() {
            session.last_used = Instant::now();
        }
    }

    let truncated = stdout.truncated() || stderr.truncated();
    Ok(SandboxResult {
        stdout: stdout.into_string(),
        stderr: stderr.into_string(),
        exit_code,
        duration_ms,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_result_serialization() {
        let result = SandboxResult {
            stdout: "hello".into(),
            stderr: "world".into(),
            exit_code: 0,
            duration_ms: 123,
            truncated: false,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"stdout\":\"hello\""));
        assert!(json.contains("\"stderr\":\"world\""));
        assert!(json.contains("\"exit_code\":0"));
    }

    #[test]
    fn capped_buffer_passes_small_output_through() {
        let mut buf = CappedBuffer::new(DEFAULT_OUTPUT_BYTES);
        buf.push(b"hello ");
        buf.push(b"world");
        assert!(!buf.truncated());
        assert_eq!(buf.into_string(), "hello world");
    }

    #[test]
    fn capped_buffer_keeps_head_and_tail() {
        let mut buf = CappedBuffer::new(DEFAULT_OUTPUT_BYTES);
        // Overflow both head and tail with distinctive markers at each end.
        buf.push(b"START");
        buf.push(&vec![b'x'; DEFAULT_OUTPUT_BYTES]);
        buf.push(b"END");
        assert!(buf.truncated());
        let out = buf.into_string();
        assert!(out.starts_with("START"));
        assert!(out.ends_with("END"));
        assert!(out.contains("output truncated"));
    }

    #[test]
    fn capped_buffer_reports_exact_omitted_bytes() {
        let mut buf = CappedBuffer::new(DEFAULT_OUTPUT_BYTES);
        buf.push(&vec![b'a'; DEFAULT_OUTPUT_BYTES + 1000]);
        assert!(buf.truncated());
        assert!(buf.into_string().contains("1000 bytes omitted"));
    }

    #[test]
    fn capped_buffer_respects_custom_limit() {
        let mut buf = CappedBuffer::new(10);
        buf.push(b"0123456789abcdef");
        assert!(buf.truncated());
        let out = buf.into_string();
        // 40% head (4 bytes) + 60% tail (6 bytes).
        assert!(out.starts_with("0123"));
        assert!(out.ends_with("abcdef"));
    }

    #[test]
    fn init_script_is_empty_without_config() {
        assert!(build_init_script(&SandboxGitConfig::default(), &[]).is_none());
    }

    #[test]
    fn init_script_sets_identity_credentials_and_clones() {
        let git = SandboxGitConfig {
            user_name: Some("Bot".into()),
            user_email: Some("bot@example.com".into()),
            token: Some("tok".into()),
            token_user: None,
        };
        let repos = vec!["https://github.com/acme/widget.git".to_string()];
        let script = build_init_script(&git, &repos).unwrap();
        assert!(script.contains("git config --global user.name 'Bot'"));
        assert!(script.contains("git config --global user.email 'bot@example.com'"));
        assert!(script.contains("credential.helper"));
        // The token value itself must never appear in the script.
        assert!(!script.contains("tok\""));
        assert!(script.contains("git clone 'https://github.com/acme/widget.git'"));
        assert!(script.contains("safe.directory"));
    }

    #[test]
    fn init_script_clones_without_git_identity() {
        let repos = vec!["https://example.com/a.git".to_string()];
        let script = build_init_script(&SandboxGitConfig::default(), &repos).unwrap();
        assert!(script.contains("git clone"));
        assert!(!script.contains("user.name"));
    }

    #[test]
    fn cpus_convert_to_nano() {
        assert_eq!(cpus_to_nano(2.0), 2_000_000_000);
        assert_eq!(cpus_to_nano(0.5), 500_000_000);
        // Clamped to a sane floor.
        assert_eq!(cpus_to_nano(0.0), 100_000_000);
    }
}
