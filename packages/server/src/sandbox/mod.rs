//! Docker sandbox for safe command execution.
//!
//! Spawns ephemeral containers with tight security constraints.
//! All state lives on a persistent named volume at /workspace.

use bollard::{
    Docker,
    container::{Config, CreateContainerOptions, WaitContainerOptions},
    image::CreateImageOptions,
    models::{HostConfig, Mount, MountTypeEnum},
};
use futures_util::StreamExt;
use serde::Serialize;
use std::time::Duration;
use tokio::time::timeout;

const WORKSPACE_VOLUME: &str = "helpcore-sandbox-workspace";
const MAX_OUTPUT_BYTES: usize = 100_000;

/// Shared sandbox state.
#[derive(Clone)]
pub struct SandboxState {
    /// Bollard Docker client.
    pub docker: Docker,
    /// Configured sandbox image.
    pub image: String,
    /// Default execution timeout in seconds.
    pub timeout: u64,
    /// Memory limit in MB.
    pub memory_mb: u64,
    /// Human-readable Docker connection info for error messages.
    pub host_info: String,
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
        /// Which Docker API operation failed (e.g. "pull", "create_container").
        operation: &'static str,
        /// Human-readable Docker connection info.
        host: String,
        /// The underlying bollard/hyper error.
        source: bollard::errors::Error,
    },
    /// Command exceeded its timeout.
    #[error("Command timed out after {0}s")]
    Timeout(u64),
    /// Output exceeded size limits.
    #[error("Output too large ({0} bytes)")]
    OutputTooLarge(usize),
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

/// Run a shell command in a sandboxed container.
///
/// Creates a container from the configured sandbox image, runs the command,
/// collects stdout/stderr, waits for exit, then destroys the container.
pub async fn exec(
    state: &SandboxState,
    command: &str,
    timeout_secs: Option<u64>,
) -> Result<SandboxResult, SandboxError> {
    let start = std::time::Instant::now();
    let timeout_secs = timeout_secs.unwrap_or(state.timeout).min(600);

    // Ensure image is pulled
    let mut pull_stream = state.docker.create_image(
        Some(CreateImageOptions {
            from_image: state.image.as_str(),
            ..Default::default()
        }),
        None,
        None,
    );
    while let Some(pull_result) = pull_stream.next().await {
        pull_result.map_err(|e| SandboxError::docker("pull_image", &state.host_info, e))?;
    }

    // Create container
    let config = Config {
        image: Some(state.image.as_str()),
        cmd: Some(vec!["/bin/sh", "-c", command]),
        working_dir: Some("/workspace"),
        host_config: Some(HostConfig {
            // Security
            cap_drop: Some(vec!["ALL".to_string()]),
            security_opt: Some(vec!["no-new-privileges:true".to_string()]),
            readonly_rootfs: Some(false),
            privileged: Some(false),
            // Resources
            memory: Some((state.memory_mb * 1024 * 1024) as i64),
            nano_cpus: Some(1_000_000_000), // 1 CPU
            pids_limit: Some(100),
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
            // Auto-cleanup
            auto_remove: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    };

    let id: String = state
        .docker
        .create_container(None::<CreateContainerOptions<String>>, config)
        .await
        .map_err(|e| SandboxError::docker("create_container", &state.host_info, e))?
        .id;

    // Start
    state
        .docker
        .start_container::<String>(&id, None)
        .await
        .map_err(|e| SandboxError::docker("start_container", &state.host_info, e))?;

    // Wait for exit with timeout
    let wait_result = timeout(
        Duration::from_secs(timeout_secs),
        state
            .docker
            .wait_container::<String>(&id, None::<WaitContainerOptions<String>>)
            .next(),
    )
    .await;

    let exit_code = match wait_result {
        Ok(Some(Ok(output))) => output.status_code,
        Ok(Some(Err(e))) => {
            return Err(SandboxError::docker("wait_container", &state.host_info, e));
        }
        Ok(None) => return Err(anyhow::anyhow!("container disappeared before exiting").into()),
        Err(_) => {
            let _ = state.docker.kill_container::<String>(&id, None).await;
            return Err(SandboxError::Timeout(timeout_secs));
        }
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    // Collect logs
    let logs_options = bollard::container::LogsOptions {
        stdout: true,
        stderr: true,
        tail: "all",
        ..Default::default()
    };

    let mut logs_stream = state.docker.logs(&id, Some(logs_options));
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut truncated = false;

    while let Some(log_result) = logs_stream.next().await {
        match log_result {
            Ok(log_output) => {
                use bollard::container::LogOutput;
                match log_output {
                    LogOutput::StdOut { message } => {
                        let s = String::from_utf8_lossy(&message);
                        if stdout.len() + s.len() > MAX_OUTPUT_BYTES {
                            stdout.push_str(&s[..MAX_OUTPUT_BYTES - stdout.len()]);
                            truncated = true;
                        } else {
                            stdout.push_str(&s);
                        }
                    }
                    LogOutput::StdErr { message } => {
                        let s = String::from_utf8_lossy(&message);
                        if stderr.len() + s.len() > MAX_OUTPUT_BYTES {
                            stderr.push_str(&s[..MAX_OUTPUT_BYTES - stderr.len()]);
                            truncated = true;
                        } else {
                            stderr.push_str(&s);
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => {
                tracing::debug!("sandbox log error: {e}");
                break;
            }
        }
        if truncated {
            break;
        }
    }

    Ok(SandboxResult {
        stdout,
        stderr,
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
}
