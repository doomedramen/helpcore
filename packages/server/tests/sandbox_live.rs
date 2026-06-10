//! Live sandbox tests that require a running Docker daemon and the
//! `helpcore-sandbox:latest` image (build it with `make sandbox-image`).
//!
//! These are ignored by default so CI without Docker stays green. Run with:
//!
//! ```sh
//! cargo test -p helpcore-server --test sandbox_live -- --ignored --nocapture
//! ```

use helpcore_server::config::{SandboxConfig, SandboxGitConfig};
use helpcore_server::sandbox::{self, ExecOptions, SandboxError, SandboxState};
use std::time::Instant;

fn live_state(config: SandboxConfig) -> SandboxState {
    let docker =
        bollard::Docker::connect_with_local_defaults().expect("Docker daemon not reachable");
    SandboxState::new(docker, "local docker".into(), &config)
}

/// One sequential suite: the session container has a fixed name, so parallel
/// tests would tear each other's containers down.
#[tokio::test]
#[ignore = "requires Docker and the helpcore-sandbox:latest image"]
async fn live_sandbox_suite() {
    let state = live_state(SandboxConfig {
        enabled: true,
        ..Default::default()
    });

    // Basic execution and exit codes.
    let result = sandbox::exec(&state, "echo hello", None).await.unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout.trim(), "hello");

    let result = sandbox::exec(&state, "exit 3", None).await.unwrap();
    assert_eq!(result.exit_code, 3);

    // Warm container: the second command must not pay container start-up cost.
    let start = Instant::now();
    sandbox::exec(&state, "true", None).await.unwrap();
    let warm = start.elapsed();
    assert!(
        warm.as_millis() < 1000,
        "warm exec took {warm:?}, expected well under a second"
    );

    // Workspace persistence across separate commands.
    sandbox::exec(&state, "echo persisted > /workspace/live-test.txt", None)
        .await
        .unwrap();
    let result = sandbox::exec(&state, "cat /workspace/live-test.txt", None)
        .await
        .unwrap();
    assert_eq!(result.stdout.trim(), "persisted");

    // cwd and env options.
    sandbox::exec(&state, "mkdir -p /workspace/live-sub", None)
        .await
        .unwrap();
    let result = sandbox::exec_with(
        &state,
        "pwd && printf '%s' \"$LIVE_TEST_VAR\"",
        ExecOptions {
            cwd: Some("/workspace/live-sub".into()),
            env: vec!["LIVE_TEST_VAR=42".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(result.stdout.contains("/workspace/live-sub"));
    assert!(result.stdout.trim_end().ends_with("42"));

    // Large writes travel via stdin (no per-argument size limit).
    let big = "x".repeat(300_000);
    let result = sandbox::exec_with(
        &state,
        "cat > /workspace/live-big.txt",
        ExecOptions {
            stdin: Some(big.into_bytes()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(result.exit_code, 0);
    let result = sandbox::exec(&state, "wc -c < /workspace/live-big.txt", None)
        .await
        .unwrap();
    assert_eq!(result.stdout.trim(), "300000");

    // Output overflow keeps head and tail.
    let result = sandbox::exec(
        &state,
        "echo FIRST; yes filler | head -n 20000; echo LAST",
        None,
    )
    .await
    .unwrap();
    assert!(result.truncated);
    assert!(result.stdout.starts_with("FIRST"));
    assert!(result.stdout.trim_end().ends_with("LAST"));
    assert!(result.stdout.contains("output truncated"));

    // Timeouts are enforced in-container and reported as errors.
    let start = Instant::now();
    let err = sandbox::exec(&state, "sleep 30", Some(2))
        .await
        .unwrap_err();
    assert!(matches!(err, SandboxError::Timeout(2)), "got: {err}");
    assert!(start.elapsed().as_secs() < 10);
    // The session container survives a command timeout.
    let result = sandbox::exec(&state, "echo alive", None).await.unwrap();
    assert_eq!(result.stdout.trim(), "alive");

    // Background process pattern used by sandbox_exec(background=true).
    // NB: mkdir is separated with ';' so only the setsid command is
    // backgrounded by '&'.
    let launcher = "mkdir -p /tmp/helpcore-proc || exit 1; \
                    setsid /bin/sh -c 'sleep 30' > /tmp/helpcore-proc/livetest.log 2>&1 < /dev/null & \
                    PID=$!; printf '%s\n' \"$PID\" > /tmp/helpcore-proc/livetest.meta; echo \"$PID\"";
    let result = sandbox::exec(&state, launcher, Some(30)).await.unwrap();
    let pid: i64 = result.stdout.trim().parse().expect("launcher prints a pid");
    assert!(pid > 0);
    let result = sandbox::exec(
        &state,
        "kill -0 $(sed -n 1p /tmp/helpcore-proc/livetest.meta) && echo running",
        None,
    )
    .await
    .unwrap();
    assert_eq!(result.stdout.trim(), "running");
    let result = sandbox::exec(
        &state,
        "PID=$(sed -n 1p /tmp/helpcore-proc/livetest.meta); \
         kill -TERM -- \"-$PID\" 2>/dev/null || kill -TERM \"$PID\"; sleep 1; \
         if kill -0 \"$PID\" 2>/dev/null; then echo still-alive; else echo stopped; fi",
        None,
    )
    .await
    .unwrap();
    assert_eq!(result.stdout.trim(), "stopped");

    // Config changes recreate the session container (container-local /tmp
    // state is lost, /workspace persists).
    sandbox::exec(&state, "touch /tmp/live-marker", None)
        .await
        .unwrap();
    state.update_config(&state.image(), state.timeout(), state.memory_mb() + 256);
    let result = sandbox::exec(
        &state,
        "test -f /tmp/live-marker && echo old-container || echo fresh-container",
        None,
    )
    .await
    .unwrap();
    assert_eq!(result.stdout.trim(), "fresh-container");
    let result = sandbox::exec(&state, "cat /workspace/live-test.txt", None)
        .await
        .unwrap();
    assert_eq!(result.stdout.trim(), "persisted");

    // Network is available (the image build itself proves registry access,
    // but verify DNS + TCP from inside the session container).
    let result = sandbox::exec(
        &state,
        "curl -fsS -o /dev/null -w '%{http_code}' --max-time 20 https://example.com",
        Some(30),
    )
    .await
    .unwrap();
    assert_eq!(result.stdout.trim(), "200", "stderr: {}", result.stderr);

    // Cleanup.
    sandbox::exec(
        &state,
        "rm -rf /workspace/live-test.txt /workspace/live-big.txt /workspace/live-sub",
        None,
    )
    .await
    .unwrap();
}

/// Git identity from config is applied by the session init script.
#[tokio::test]
#[ignore = "requires Docker and the helpcore-sandbox:latest image"]
async fn live_sandbox_git_identity() {
    let state = live_state(SandboxConfig {
        enabled: true,
        git: SandboxGitConfig {
            user_name: Some("Live Test Bot".into()),
            user_email: Some("live-test@example.com".into()),
            token: Some("dummy-token".into()),
            token_user: None,
        },
        ..Default::default()
    });

    let result = sandbox::exec(&state, "git config --global user.name", None)
        .await
        .unwrap();
    assert_eq!(result.stdout.trim(), "Live Test Bot");
    let result = sandbox::exec(&state, "git config --global user.email", None)
        .await
        .unwrap();
    assert_eq!(result.stdout.trim(), "live-test@example.com");
    // Credential helper reads from the environment.
    let result = sandbox::exec(
        &state,
        "printf 'protocol=https\nhost=example.com\n\n' | git credential fill 2>/dev/null \
         | grep password=",
        None,
    )
    .await
    .unwrap();
    assert_eq!(result.stdout.trim(), "password=dummy-token");
}
