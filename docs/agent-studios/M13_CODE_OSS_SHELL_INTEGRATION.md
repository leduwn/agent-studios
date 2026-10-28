# Milestone M13 — Code-OSS Shell Integration Foundation & IPC Architecture

- **Milestone**: `M13 — Code-OSS Shell Integration Foundation`
- **Status**: **APPROVED ARCHITECTURE SPECIFICATION & EXECUTION BLUEPRINT**
- **Date**: `2026-10-28`
- **Lead Owner**: Principal Engineering Lead & Distributed Systems Architect
- **Precedence**: Subservient to `MASTER_VISION.md` and `CODE_OSS_INTEGRATION.md`.

---

## 1. Executive Summary & Architectural Scope

Milestone M13 establishes the **shared shell and native IPC foundation** bridging the standalone CLI (`ags`) and the Code-OSS IDE workbench.

### Architectural Core Invariants

1. **Zero Control Plane Duplication**: The Control Plane kernel (`agent-studios-control-plane`) remains the single authoritative state store for task graphs, run states, workspace leases, and approval records. The IPC app-server exposes Control Plane projections over typed streaming transports without inventing separate IDE-only state machines.
2. **First-Class Shared Sessions**: A developer can launch an agent goal from the CLI (`ags prompt "..."`), close or detach the terminal, open Code-OSS, and observe active streaming turns, pending approvals, and diff viewers in the `Agents` sidebar against the exact same session.
3. **Fail-Closed Local IPC**: Communication between CLI/IDE clients and the local daemon is secured via Windows Named Pipes and Unix Domain Sockets guarded by OS file permissions and cryptographic session nonces. Unauthorized local processes cannot inject tool approvals or snoop event feeds.
4. **Resilient Attach / Detach / Reconnect**: Terminal clients can disconnect abruptly (terminal closed, network drop in remote SSH, SIGKILL) without interrupting ongoing background agent execution. Reconnecting clients seamlessly backfill historical events from their last observed sequence watermark without dropping or duplicating events.

---

## 2. Evaluation of Codex App-Server Architecture as Foundation

The upstream Codex architecture provides an `app-server` JSON-RPC over stdio / WebSocket implementation.

### Assessment Matrix

| Dimension | Upstream Codex App-Server | Agent Studios M13 Evaluation | Decision & Path Forward |
|---|---|---|---|
| **Transport** | Stdio pipe or TCP port loopback | TCP loopback exposes local ports to all local processes on multi-user systems | **Adopt Native IPC**: Named Pipes on Windows (`\\.\pipe\agent-studios-...`) and UDS on Unix (`/run/user/...` or `/tmp/...`). |
| **State Machine** | Ephemeral in-memory session threads | Single-threaded memory model tied to process lifecycle | **Retain Agent Studios Control Plane**: Durable event log, monotonic sequence IDs, deterministic state replay. |
| **Multi-Client** | Typically single client per process | Multiple concurrent readers/approvers (CLI + IDE) | **Multiplexed Hub**: Shared `SessionEventHub` broadcasting events across $N$ connected clients with independent watermarks. |
| **Approval Bridge** | Simple blocking RPC | Two-Phase Commit (2PC) with fail-closed delivery verification | **Port M12.2/M12.3 2PC Engine**: Approvals require active transport delivery verification to runtime child stdin. |

---

## 3. High-Level IPC & System Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                    Developer Workspace                       │
├───────────────────────────────┬──────────────────────────────┤
│      Code-OSS IDE Workbench   │      Terminal / CLI (`ags`)  │
│  (Extension Host / TypeScript)│       (Ratatui Rust TUI)     │
│               │               │               │              │
│      Language Client / RPC    │        ConnectedClient       │
└───────────────┬───────────────┴───────────────┬──────────────┘
                │ Native IPC Transport          │ Native IPC Transport
                │ (Windows Named Pipe / UDS)    │ (Windows Named Pipe / UDS)
                ▼                               ▼
┌──────────────────────────────────────────────────────────────┐
│         Agent Studios Background Daemon (`agsd` / IPC Server) │
├──────────────────────────────────────────────────────────────┤
│  ┌────────────────────────────────────────────────────────┐  │
│  │                IPC Session Router & Auth               │  │
│  │  - OS DACL / ACL Verification                           │  │
│  │  - Per-Session Bearer Nonce Handshake                  │  │
│  │  - Connection Watermark Registry                       │  │
│  └────────────────────────────┬───────────────────────────┘  │
│                               │                              │
│  ┌────────────────────────────▼───────────────────────────┐  │
│  │          Agent Studios Control Plane Kernel            │  │
│  │  - Event Store (Contiguous Sequence Numbers)           │  │
│  │  - Task Graph & Run State Machine                      │  │
│  │  - Workspace Lease & Git Worktree Orchestrator        │  │
│  │  - 2PC Approval Authority & Session Correlation        │  │
│  └────────────────────────────┬───────────────────────────┘  │
│                               │                              │
│  ┌────────────────────────────▼───────────────────────────┐  │
│  │              External AI Runtime Host                  │  │
│  │  - OpenCode Adapter                                    │  │
│  │  - Claude Code Adapter (Fail-Closed Stdin Watchdog)    │  │
│  │  - Local Deterministic Runtime (Demo / Preview)        │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

---

## 4. Native IPC Transports & Security Controls

### 4.1. Platform-Specific Transports

1. **Windows**:
   - Named Pipe format: `\\.\pipe\agent-studios-{workspace_hash}-{studio_id}`.
   - Access Control: Configured with a Windows Security Descriptor restricting access to the current Windows user SID (`SECURITY_CREATOR_OWNER_RID` / `Deny Network`).
   - Rejects cross-user access, elevated/non-elevated boundary cross without token authentication.
2. **Unix (Linux / macOS)**:
   - Unix Domain Socket format: `$XDG_RUNTIME_DIR/agent-studios/{workspace_hash}.sock` (fallback: `/tmp/agent-studios-{uid}/{workspace_hash}.sock`).
   - File permissions: Mode `0600` (readable and writable only by the invoking user).

### 4.2. Local Handshake & Cryptographic Nonce Authentication

To prevent local unprivileged processes from connecting to open sockets, each workspace session generates an ephemeral 256-bit cryptographic nonce saved to:
`.agent-studios/run/session.token` (permissions `0600` / Windows read-restricted to owner).

**Handshake Sequence**:
```text
Client                                             Daemon
  │                                                  │
  │ ─── 1. Connect (Pipe / UDS) ───────────────────> │
  │                                                  │
  │ <── 2. Challenge (Random 32-byte nonce) ──────── │
  │                                                  │
  │ ─── 3. Response: HMAC-SHA256(challenge, token) ─>│
  │                                                  │
  │ <── 4. HandshakeOk { studio_id, protocol_v: 1 } ─│ (or Close on mismatch)
```

---

## 5. CLI `ConnectedClient` Protocol & Methods

The `ConnectedClient` implements `AgentStudiosClient` (the same trait implemented by `EmbeddedClient`), enabling the Ratatui TUI to operate transparently in either embedded or remote/daemon mode:

```rust
pub struct ConnectedClient {
    transport: Arc<IpcStreamTransport>,
    studio_id: StudioId,
    workspace_name: String,
    mode: RuntimeMode,
    watermark: Arc<AtomicU64>,
}

#[async_trait]
impl AgentStudiosClient for ConnectedClient {
    async fn workspace_info(&self) -> Result<WorkspaceInfo, ClientError>;
    async fn submit_goal(&self, prompt: &str, opts: GoalSubmissionOptions) -> Result<TaskId, ClientError>;
    async fn cancel_active_run(&self, reason: Option<String>) -> Result<(), ClientError>;
    async fn resolve_approval(&self, approval_id: ApprovalId, decision: ClientApprovalDecision) -> Result<(), ClientError>;
    async fn subscribe_events(&self, from_sequence: u64) -> Result<ClientSubscription, ClientError>;
    async fn get_worktree_diff(&self, worktree_id: WorktreeId) -> Result<String, ClientError>;
    async fn reconcile_patch(&self, reconciliation_id: ReconciliationId) -> Result<(), ClientError>;
}
```

### JSON-RPC 2.0 Framing Specification

Frames are newline-delimited JSON (`ndjson`) or length-prefixed binary frames:

```json
{"jsonrpc": "2.0", "id": 1, "method": "submit_goal", "params": {"prompt": "Fix auth", "options": {}}}
{"jsonrpc": "2.0", "id": 1, "result": {"task_id": "018f3a9b-..."}}

{"jsonrpc": "2.0", "method": "event_notify", "params": {"envelope": {"sequence": 104, "event": {...}}}}
```

---

## 6. Shared Session & Linearizable Event Replay

### Sequence Watermarking & Zero-Loss Reconnect

1. **Backfill Snapshot**: Upon connection or reconnection, the client transmits `last_received_sequence`.
2. **Linear Delivery**: Daemon transmits all historical events with `sequence > last_received_sequence` from the contiguous Event Store before interleaving live broadcast stream events.
3. **Gap Detection**: If the client detects a gap (`sequence > last_received + 1`), it freezes state transitions, issues `replay_missing(from, to)`, and re-establishes monotonicity before rendering.

---

## 7. Process Lifecycle & Attach/Detach Semantics

### Daemon Spawning & Detach Modes

1. **Headless Execution**: If no daemon is running, `ags` launches `agsd` in background detached mode, registers PID lockfile at `.agent-studios/run/daemon.pid`, and connects.
2. **Clean Detach**: Pressing `Ctrl+D` or quitting `ags` with active tasks closes the CLI interface while `agsd` continues executing the task, streaming output to the log.
3. **Re-Attach**: Running `ags attach` discovers the active daemon, performs nonce handshake, backfills missed transcript lines, and resumes interactive TUI rendering.

---

## 8. Code-OSS Workbench Integration Module (M14 Bridge)

The TypeScript extension (`agent-studios-vscode`) implements the client protocol:
1. Connects to `\\.\pipe\agent-studios-...` or Unix domain socket.
2. Binds to Code-OSS Custom Tree View in the `Agents` Activity Bar container.
3. Maps pending approvals directly to Code-OSS QuickPick and modal notifications.
4. Renders git diffs natively using Code-OSS's built-in Diff Editor via virtual document provider (`agent-studios-diff://...`).

---

## 9. Implementation Plan & Concrete Acceptance Gates

### Module Ownership & Crates

- `crates/ipc-protocol`: Shared serialization types, RPC framing, handshake, challenge/response HMAC.
- `crates/ipc-transport`: Platform-abstracted Named Pipe (Windows `tokio::net::windows::named_pipe`) and Unix Domain Socket (`tokio::net::UnixStream`).
- `crates/cli-client/src/connected.rs`: `ConnectedClient` implementation of `AgentStudiosClient`.
- `crates/app-server`: Standalone daemon service hosting Control Plane actor and multiplexing client streams.

### Acceptance Verification Gates (M13 Gate)

1. **IPC Security Gate**: Non-owner process access attempt is rejected with explicit OS permission or HMAC verification failure.
2. **Shared Session Gate**: CLI submits goal $\rightarrow$ IDE receives streaming chunks and approval request $\rightarrow$ IDE approves $\rightarrow$ CLI transcript reflects approved tool run $\rightarrow$ task completes identically on both ends.
3. **Disconnect Recovery Gate**: Kill CLI mid-stream $\rightarrow$ agent task completes in daemon $\rightarrow$ restart CLI and attach $\rightarrow$ full transcript and terminal task status recovered cleanly without loss or duplicate items.
4. **Platform Parity Gate**: 100% test pass on Windows 11 and Ubuntu Linux.
