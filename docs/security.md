AI Assistant Security Architecture (Linux)
A reference guide for building a secure, multi-channel AI assistant — covering sandboxing, permission design, multi-user isolation, and threat mitigation.

Overview
This document covers the security architecture for a self-hosted AI assistant that communicates over multiple channels (web UI, WhatsApp, email, Telegram, terminal, iOS app) and has scoped read/write access to the local filesystem. The target platform is Linux only, which enables the strongest available sandboxing stack.
The four threat vectors this architecture addresses:

AI going rogue or performing unintended actions
Malicious users manipulating the AI via prompt injection or social engineering
Data leakage between users
External attackers compromising the system


Security Stack
Three Rust crates form the core enforcement layers, applied in depth:
toml[dependencies]
cap-std  = "3"
landlock = "0.4"
extrasafe = "0.3"
Layer 1 — Logical Permission Store (your code)
The topmost layer. Implemented in your Rust application, it provides the AI-facing API:

A check(path, permission) function consulted before every file operation
A request_permission(path, permission) tool exposed to the AI for non-locked paths
A hard-locked deny list that the AI cannot even request access to

This layer produces readable error messages, drives the permission-request UX, and acts as the first line of defence. It cannot stop a compromised process on its own, which is why the layers below exist.
Layer 2 — cap-std (structural path safety)
Rather than blocking bad paths, cap-std makes it structurally impossible for code to reference paths outside permitted roots. A Dir handle rooted at /allowed/path cannot express ../../../etc/passwd — the type system prevents it.
rustuse cap_std::fs::Dir;

// The AI's file tools only ever receive a Dir handle
let workspace = Dir::open_ambient_dir("/home/assistant/workspace", cap_std::ambient_authority())?;
workspace.open("notes.txt")?;          // OK
workspace.open("../../etc/shadow")?;   // Structurally impossible
Layer 3 — landlock (kernel-enforced path rules)
Linux Landlock (kernel 5.13+) enforces path access rules at the kernel level. Even if there is a bug in your Rust code, the kernel will refuse the syscall. This is the critical property that distinguishes Linux from other platforms.
rustuse landlock::{Access, AccessFs, Ruleset, RulesetAttr, RulesetCreatedAttr, ABI, PathBeneath, PathFd};

let abi = ABI::V2;
Ruleset::default()
    .handle_access(AccessFs::from_all(abi))?
    .create()?
    .add_rule(PathBeneath::new(PathFd::new("/home/assistant/workspace")?, AccessFs::from_all(abi)))?
    .add_rule(PathBeneath::new(PathFd::new("/var/log/assistant")?, AccessFs::from_read(abi)))?
    .restrict_self()?;
Hard-locked paths are simply absent from the ruleset. There is no API to add them at runtime.
Layer 4 — extrasafe (syscall filtering via seccomp-bpf)
Restricts which syscalls the AI process is allowed to make at all. Blocks execve (cannot spawn subprocesses), disables raw network access if not needed, and removes dangerous syscalls entirely.
rustuse extrasafe::{SafetyContext, builtins::{SystemIO, Networking}};

SafetyContext::new()
    .enable(SystemIO::nothing()
        .allow_open_readonly()
        .allow_read()
        .allow_write()
        .allow_close())?
    // Only enable if the AI process needs outbound network
    // .enable(Networking::nothing().allow_connect())?
    .apply_to_current_thread()?;

Permission Store Design
rustuse std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Permission { Read, Write }

pub enum AccessResult {
    Allowed,
    Denied,       // AI can call request_permission()
    HardDenied,   // No request tool offered; silent or generic refusal
}

pub struct PermissionStore {
    granted:     HashSet<(PathBuf, Permission)>,
    hard_denied: Vec<PathBuf>,   // Checked first, always wins
}

impl PermissionStore {
    pub fn check(&self, path: &Path, perm: &Permission) -> AccessResult {
        // Hard lock — kernel also enforces this, but we check first
        // to avoid leaking path existence via timing
        if self.hard_denied.iter().any(|p| path.starts_with(p)) {
            return AccessResult::HardDenied;
        }
        if self.granted.contains(&(path.to_path_buf(), perm.clone())) {
            return AccessResult::Allowed;
        }
        AccessResult::Denied
    }

    /// Tool exposed to the AI. Hard-locked paths are never mentioned.
    pub fn request_permission(
        &mut self,
        path: &Path,
        perm: Permission,
        requester: &UserId,
    ) -> Result<(), PermissionError> {
        if self.hard_denied.iter().any(|p| path.starts_with(p)) {
            // Return a generic error — don't confirm the path exists
            return Err(PermissionError::Unavailable);
        }
        // Surface to human approver, or auto-approve within policy
        self.granted.insert((path.to_path_buf(), perm));
        Ok(())
    }
}
Hard-locked paths to consider by default:

/etc, /boot, /proc, /sys, /dev
/root, /home (except the assistant's own workspace)
SSH keys, credentials, secrets directories
Any path not explicitly in the allow list


Multi-User Isolation
Because this project is designed for personal, family, and public open-source use, user isolation is critical. Data leakage between users is a real threat when multiple people interact with the same assistant instance.
Per-user workspaces
Each user gets an isolated directory with strict ownership:
/home/assistant/users/
    alice/        (owned by user:alice, mode 700)
    bob/          (owned by user:bob, mode 700)
    shared/       (read-only shared resources)
Per-session context isolation
The AI must never carry context from one user's session into another's. Enforce this at the session layer:

Each session gets its own context window; no persistent cross-session memory without explicit user opt-in
Conversation history is stored per user, never in a shared buffer
Tool results (file reads, etc.) are scoped to the requesting session

Per-user landlock rulesets
Apply different Landlock rules per user process or per request handler depending on your architecture:
rustfn build_user_ruleset(user: &User) -> Result<(), Box<dyn Error>> {
    let abi = ABI::V2;
    let user_workspace = format!("/home/assistant/users/{}", user.id);

    Ruleset::default()
        .handle_access(AccessFs::from_all(abi))?
        .create()?
        .add_rule(PathBeneath::new(PathFd::new(&user_workspace)?, AccessFs::from_all(abi)))?
        .add_rule(PathBeneath::new(PathFd::new("/home/assistant/shared")?, AccessFs::from_read(abi)))?
        .restrict_self()?;

    Ok(())
}

Prompt Injection Defence
Prompt injection is the primary vector for malicious users manipulating the AI. Inputs arriving from external channels (WhatsApp messages, emails, web forms) must all be treated as untrusted.
Structural defences
Separate system instructions from user content at the API level using distinct message roles. Never interpolate user content directly into system prompts:
rust// BAD — user content can break out of its role
let prompt = format!("You are a helpful assistant. User said: {user_input}");

// GOOD — roles are structurally separate
messages: [
    { role: "system",    content: SYSTEM_PROMPT },
    { role: "user",      content: user_input },   // untrusted content stays here
]
Channel-specific trust levels
Not all channels carry equal trust. A message from a verified family member on a known device is different from an anonymous web UI submission. Tag requests with a trust level and have the system prompt reflect it:
rustpub enum TrustLevel {
    Owner,          // You, authenticated locally
    Trusted,        // Named family members, authenticated
    Community,      // Open source project users, registered
    Anonymous,      // Unauthenticated public
}
Higher trust levels unlock more tools and broader filesystem access. Anonymous users get read-only access to public data only.
Input sanitisation for tool calls
Before the AI's tool calls reach your filesystem layer, validate:

Paths are within the expected namespace (before landlock even sees them)
Arguments are the expected type and within expected length/range
No shell metacharacters in arguments passed to any subprocess


External Attack Surface
Network exposure
Each communication channel is an attack surface. Apply defence in depth:

Terminate TLS at the edge; never accept plain HTTP from external sources
Rate-limit all channel endpoints — by IP and by user identity
For email and WhatsApp/Telegram, verify sender identity before processing message content
Webhook endpoints (Telegram bot, WhatsApp Business API) should validate signatures on every request

Secrets management

API keys, webhook secrets, and database credentials live in environment variables or a secrets manager, never in source code or config files committed to version control
The AI process should not have access to its own API key at runtime — the API call should be proxied through a separate service that holds the key
Rotate secrets on a schedule; treat any leaked secret as immediately compromised

Dependency hygiene
Open source projects are frequent targets of supply chain attacks:

Pin all dependency versions in Cargo.lock and commit it
Use cargo audit in CI to catch known vulnerabilities
Review any dependency that touches cryptography, networking, or process spawning with extra scrutiny
Consider cargo vet for supply chain verification


Process Architecture
Run the AI handler as a separate process with minimal privileges, not as a thread in your main web server:
[Web Server / Channel Handlers]   (owns TLS, auth, rate limiting)
          |
          | (IPC over Unix socket or message queue)
          v
[AI Handler Process]              (minimal privileges, landlock applied)
          |
          | (via cap-std Dir handles)
          v
[User Workspace]                  (per-user isolated directories)
This way, a compromised AI handler cannot reach the TLS certificates, session tokens, or secrets held by the web server process.

Deployment Checklist
Before exposing any channel publicly:

 Landlock rules applied and tested with strace to confirm syscalls are blocked
 extrasafe rules applied; execve blocked
 Per-user workspace directories created with correct ownership and 700 permissions
 Hard-locked paths verified absent from all Landlock rulesets
 Prompt injection tested: attempt to override system prompt via each channel
 Rate limiting active on all public endpoints
 Webhook signatures validated for Telegram and WhatsApp
 cargo audit passing with no known vulnerabilities
 No secrets in source code, config files, or Cargo.toml
 AI process running as a dedicated non-root user
 Logging in place for permission denials, hard-lock hits, and failed auth attempts


Summary
LayerCrate / MechanismThreat addressedLogical permission storeYour codeAI rogue actions, request/hard-lock UXStructural path safetycap-stdPath traversal, ../ escapesKernel path enforcementlandlockBypasses in your code, AI rogue actionsSyscall filteringextrasafeProcess spawning, raw network, dangerous callsPer-user isolationDirectory layout + landlock per userData leakage between usersTrust levelsSession taggingMalicious user escalationInput validationPrompt role separation + sanitisationPrompt injectionProcess separationSeparate AI handler processLateral movement from web layerDependency hygienecargo audit, cargo vetSupply chain attacks
The key property Linux provides that no other platform matches: Landlock and seccomp-bpf are enforced by the kernel. Even a bug in your Rust code cannot cause a syscall the kernel has blocked. The sandboxing is not contingent on your code being correct.
