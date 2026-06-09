# Execution Sandbox — Implementation Plan

This document outlines the plan to implement a Docker-based sandbox for safe command execution within `helpcore`. This enables the AI to perform general tasks like code exploration, builds, testing, and git operations in a secure, isolated environment.

## Overview

The sandbox is implemented as a **built-in tool** (`sandbox_exec`) in the `helpcore-server`. It uses `bollard` to communicate with the Docker daemon to spawn ephemeral containers.

### Key Components

- **Built-in Tool:** `sandbox_exec` registered in `plugins/runtime.rs`.
- **Sandbox Image:** A standard image (defaults to `ubuntu:latest`).
- **Persistence:** A named Docker volume `helpcore-sandbox-workspace` mounted at `/workspace`.
- **Security:** Zero network access, dropped capabilities, non-root user, and resource limits.

---

## Phase 0: Infrastructure (Manual Setup)

### 0.1 — Mount the Docker Socket
In `docker-compose.yml`, mount the host's Docker socket into the `helpcore` service (read-only).
```yaml
services:
  helpcore:
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
```

### 0.2 — Create the Workspace Volume
The workspace is persistent across tool calls.
```bash
docker volume create helpcore-sandbox-workspace
```
Add to `docker-compose.yml`:
```yaml
services:
  helpcore:
    volumes:
      - helpcore-sandbox-workspace:/workspace
volumes:
  helpcore-sandbox-workspace:
```

### 0.3 — Dependencies
Add `bollard` to `Cargo.toml`.
```toml
# packages/server/Cargo.toml
bollard = "0.18"
```

---

## Phase 1: Implementation (Execution Phase)

### 1.1 — Configuration
Add `SandboxConfig` to `packages/server/src/config.rs`.
- `enabled`: bool (default false)
- `image`: string (default "ubuntu:latest")
- `timeout`: u64 (default 120s)
- `memory_mb`: u64 (default 512)

### 1.2 — State Management
Add `SandboxState` to `AppState` in `state.rs`. Initialize in `main.rs` if enabled.

### 1.3 — The Sandbox Module
Create `packages/server/src/sandbox/mod.rs` to handle Docker container lifecycle:
1. `docker.create_image(...)` (ensure pull)
2. `docker.create_container(...)` (with security constraints)
3. `docker.start_container(...)`
4. `docker.wait_container(...)` (with timeout)
5. `docker.logs(...)` (collect and truncate)

### 1.4 — Tool Registration
Update `packages/server/src/plugins/runtime.rs`:
- Add `sandbox_exec` to `BUILTIN_TOOL_NAMES`.
- Define `ToolDefinition` in `builtin_tool_definitions()`.
- Add match arm in `execute_builtin()` to call `sandbox::exec`.

---

## Security Invariants (Non-Negotiable)

- **`--network none`:** Zero network access from inside the sandbox.
- **`--cap-drop ALL`:** No kernel capabilities.
- **`--security-opt no-new-privileges:true`:** Prevent privilege escalation.
- **`readonly_rootfs`:** The container's system files are immutable.
- **Named Volume Only:** No bind mounts to prevent path traversal on the host.
- **Non-root User:** Runs as UID 1000.
- **Resource Limits:** Hard caps on Memory (512MB), CPU (1 core), and PIDs (100).
- **Hard Timeout:** Container killed after timeout (max 600s).
- **Output Truncation:** Logs capped at 100KB to prevent memory DoS.

## Production Considerations

When running in a production Docker environment (e.g. Linux):

1. **Socket Permissions:** Ensure the `helpcore` user in the container has permission to read/write the mounted `/var/run/docker.sock`. You may need to match the GID of the `docker` group on the host.
2. **Security Proxy (Recommended):** Instead of mounting the raw socket, use a security proxy like `tecnativa/docker-socket-proxy`. Configure it to only allow `POST /containers/create` and `POST /containers/start`, and point `helpcore` to the proxy via the `host` setting.
3. **Persistence:** Ensure the `helpcore-sandbox-workspace` volume is backed up if it contains important AI-generated state.

---

## Implementation Sequence

1. **Infrastructure:** Update `Cargo.toml`, `config.rs`, and `state.rs`.
2. **Module:** Implement `sandbox/mod.rs` container logic.
3. **Integration:** Wire the tool into `runtime.rs`.
4. **Validation:** Test with simple `echo`, `ls`, and then `git clone` within the sandbox.
