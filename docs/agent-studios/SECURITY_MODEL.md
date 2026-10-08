# Agent Studios — Security & Trust Model

> **Status**: Core Architecture Specification
>
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Executive Summary & Security Tenets

Agent Studios gives autonomous AI agents real command execution and filesystem mutation capabilities. This level of power requires a rigorous, defense-in-depth security architecture.

### Core Security Tenets
1. **Zero Plaintext Secrets**: API keys and credentials never touch disk, persistent logs, event stores, or telemetry in plaintext.
2. **Execution Boundary Enforcement**: Security rules and budgets are enforced at the actual operating system and runtime dispatch seams, not merely as client-side UI recommendations.
3. **Fail-Closed Permissions**: If a permission, budget, or tool check fails or times out, execution is blocked immediately.
4. **Physical Isolation**: Mutating tasks execute in dedicated Git worktrees to prevent destructive file clobbering.

---

## 2. Zero Plaintext Secrets Architecture

Credentials (OpenAI keys, Anthropic tokens, custom gateway headers) are strictly managed via zero-secret references:

```text
User / OS Credential Store
           │
           ▼
SecretReference { source: EnvVar("ANTHROPIC_API_KEY") }
           │
           │ (Catalog, Events, Projections, UI)
           ▼
[ ZERO PLAINTEXT SECRETS IN MEMORY OR STORAGE ]
           │
           ▼ (Resolved Ephemerally at HTTP Wire Boundary)
InMemorySecretResolver::resolve()
           │
           ▼
HTTP Header: `x-api-key: sk-ant-...`
           │
           ▼ (Dropped Immediately After Request Dispatch)
ZeroizeOnDrop Memory Buffer
```

### Invariants
- **No Disk Serialization**: `ProviderInstance` and `Catalog` records serialize only `SecretReference` descriptors (e.g., `EnvVar("NAME")`, `Keyring("ID")`), never the raw token string.
- **Redacted Telemetry**: Event envelopes, tool execution summaries, and Context Inspector views replace secret values with `***` or typed references.
- **Memory Zeroization**: Raw token buffers in `agent-studios-runtime-transport` implement `zeroize::ZeroizeOnDrop` to clear memory when dropped.

---

## 3. Runtime Execution Boundary Enforcement

Security checks occur at real system execution boundaries:

```text
               Agent Intent: `exec_command("rm -rf /")`
                                 │
                                 ▼
                     1. Pre-Execution Budget Gate
                        - Turns remaining?
                        - Tool budget available?
                                 │
                                 ▼
                     2. Policy & Sandbox Filter
                        - Command permitted?
                        - Path inside worktree root?
                                 │
                                 ▼
                     3. Human Approval Gate
                        - Requires human confirmation?
                        - [ Pending in Approval Inbox ]
                                 │ (Human Approves)
                                 ▼
                     4. Operating System Sandbox / Containment
                        - Directory Confinement (worktree root)
                        - Environment Sanitization
                        - OS Process Guard (TBD M10+)
                                 │
                                 ▼
                        Real Shell Execution
```

- If pre-execution checks fail, Codex emits `ToolCallOutcome::Blocked`. The tool handler is **never called**.

---

## 4. Process Containment & Sandboxing

### Canonical Architectural Requirements
1. **Windows-First, Portable Core**:
   Target platform is native Windows 11 x64 (MSVC) while preserving cross-platform core portability.
2. **Directory Confinement**:
   - Agents are restricted to their allocated `source_cwd` inside `.git/agent-studios/worktrees/<session>/<task_id>/`.
   - Path traversal (`../..`) targeting parent repositories or system root directories is detected and blocked before command execution.
3. **Environment Sanitization**:
   - Subprocesses inherit a sanitized environment block with system path defaults. User credentials not explicitly mapped to the task are stripped from the environment.

### OS Containment Implementation Status (Open Design Question in M10+)
- **Windows**: Windows Job Objects and AppContainer are **planned candidates** for external runtimes and process containment in Milestone M10+. They are not locked as implemented facts or permanent architectural constraints today.
- **Linux & macOS**: Mechanisms such as Linux cgroups/namespaces or macOS sandbox-exec remain **future exploration** for Milestone M10+ and are not prematurely decided.

---

## 5. Credential Stores Integration

Agent Studios supports multiple secure credential backends:
1. **Environment Variables**: Primary standard for developer environments (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`).
2. **OS Native Keyring (Windows Credential Manager / macOS Keychain / Linux Secret Service)**: Planned integration in desktop milestones for persistent, encrypted at-rest API key storage without `.env` files.
3. **Local Encryption at Rest**: Studio configuration files containing metadata are stored locally with file permissions restricted to the current user account.
