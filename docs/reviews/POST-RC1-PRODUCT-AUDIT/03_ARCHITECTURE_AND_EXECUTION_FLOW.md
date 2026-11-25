# 03 — End-to-End Architecture & Execution Flow

> **Scope**: Authoritative runtime flow from user goal submission to worktree verification, diff capture, review, and patch reconciliation (`00e5c862b8`).

---

## 1. System Execution Flow

```mermaid
flowchart TD
    U["User Goal (AgentStudios.exe / VS Code Chat / ags run / ags TUI)"] --> B["Client Transport Layer\n(DaemonConnectedBridge / ConnectedClient / EmbeddedClient)"]
    B -->|"HMAC-SHA256 + Named Pipe / UDS (agsd)"| CP["Control Plane Actor\n(crates/control-plane)"]
    CP -->|"SQLite WAL + Single-Writer Lock"| DB[("%LOCALAPPDATA%\\AgentStudios\\state.db")]
    CP -->|"Kahn Topological Sort"| DAG["Task DAG & Execution Scheduler"]
    DAG -->|"Policy Check"| SEC["OperatorConfirmationManager & SecretRedactor"]
    DAG -->|"Allocate Isolated Branch"| WT["Git Worktree Provisioner\n(crates/workspace)"]
    DAG -->|"Route Runtime Preference"| RR["RuntimeRegistry\n(crates/external-runtime)"]
    RR -->|Priority 1| CODEX["Internal Codex Runtime\n(crates/internal-agent -> codex-rs Patches 001-004)"]
    RR -->|Priority 2| CLAUDE["Claude Code Runtime\n(crates/claude-code-runtime, Win32 Job + 2PC stdin)"]
    RR -->|Priority 3| OPENCODE["OpenCode Runtime\n(crates/opencode-runtime)"]
    CODEX --> PA["Protocol Adapters & Provider Catalog\n(OpenAI / Anthropic / Gemini / Custom BYOK)"]
    CLAUDE --> EXT["M16 Extensibility\n(SKILL.md, MCP Stdio/HTTP, Hooks, AGENTS.md/CLAUDE.md)"]
    CODEX --> EXT
    WT -->|"Ephemeral GIT_INDEX_FILE Diff + Tests"| ART["Artifact Store (SHA-256 Chunked Diffs)"]
    ART -->|"IPC Replay Stream + Watermark"| REV["Review & Diff UI (Mission Control / TUI / CLI)"]
    REV -->|"3-Way Reconcile After Approval"| REPO["Target Git Repository"]
```

---

## 2. Two-Phase Approval & Worktree Isolation Sequence

```mermaid
sequenceDiagram
    participant UI as VS Code Extension / CLI
    participant Agsd as agsd Daemon (crates/ipc-transport)
    participant CP as Control Plane (SQLite WAL)
    participant RT as Runtime Adapter (Codex / Claude / OpenCode)
    participant WT as Git Worktree (crates/workspace)

    UI->>Agsd: auth_challenge -> hmac_sha256(client_key) -> submit_goal
    Agsd->>CP: Persist Run + DAG Tasks -> emit monotonic seq events
    CP->>WT: Create isolated branch & clean worktree directory
    CP->>RT: Start session in worktree cwd (SecretStore injects auth header in-memory)
    RT-->>CP: Tool call requires elevation -> Pause & emit ApprovalRequested
    CP-->>UI: Event envelope (seq N) -> Show 2PC Confirmation Modal
    UI->>Agsd: resolve_approval(approval_id, challenge_token, Approved)
    Agsd->>RT: Deliver 2PC approval (if pipe write fails -> FailClosed kill)
    RT-->>WT: Apply edits + execute verification commands
    CP->>WT: git add -A under ephemeral GIT_INDEX_FILE -> generate diff -> store SHA-256 artifact
    UI->>Agsd: inspect diff / accept -> reconcile_task -> merge clean commit
```

---

## 3. Authoritative Source Anchors

- **Daemon Entrypoint & IPC Loop**: `agent-studios-rs/crates/ipc-transport/src/bin/agsd.rs`, `server.rs`
- **Daemon & Client Auth Protocol**: `agent-studios-rs/crates/ipc-protocol/src/redaction.rs`, `approval.rs`
- **DAG & SQLite State**: `agent-studios-rs/crates/control-plane/src/`
- **Ephemeral Diff Capture & 3-Way Reconciliation**: `agent-studios-rs/crates/workspace/src/{capture.rs,reconciliation.rs,artifact_store.rs}`
- **Runtime Override Seams into Upstream Codex**: `codex-rs/core/src/{client.rs,model_runtime.rs,thread_manager.rs,agent/control.rs}`
- **Extension Entry & Mode Continuity**: `apps/agent-studios-vscode/src/{extension.ts,commands/CommandsRegistry.ts,bridge/DaemonConnectedBridge.ts}`
