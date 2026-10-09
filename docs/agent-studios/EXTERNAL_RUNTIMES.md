# Agent Studios — External Runtimes Architecture

> **Status**: Interface Implemented & Hardened (Milestones M10 & M10.1 COMPLETED; Adapters M11–M12 PLANNED)
> **Current Reality**: Internal Codex (`codex-rs`) is the active production runtime. Vendor-neutral `agent-studios-external-runtime` interface, hardened contract boundaries, and 24-point contract test matrix are **IMPLEMENTED & VERIFIED**. Concrete adapters are **PLANNED**.
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Architectural Role & Motivation

While the Internal Codex Engine provides our core, portable agent runtime, Agent Studios is designed as an open multi-agent environment capable of supervising diverse third-party coding agents.

Different coding agents possess unique tool ecosystems, specialized prompts, and idiosyncratic strengths:
- **OpenCode** (`opencode-ai`): Highly flexible, open provider CLI with terminal and desktop ergonomics.
- **Claude Code** (`@anthropic-ai/claude-code`): Specialized Anthropic harness with sub-agent loops and tool abstractions.
- **Codex CLI**: Standalone upstream OpenAI Codex CLI executable.

The External Runtime subsystem integrates these distinct CLI agents into the Agent Studios Control Plane as first-class task executors.

---

## 2. Fundamental Implementation Reality

```text
┌─────────────────────────────────────────────────────────────────┐
│                      IMPLEMENTATION STATUS                      │
├────────────────────────────────┬────────────────────────────────┤
│ Runtime Engine                 │ Status                         │
├────────────────────────────────┼────────────────────────────────┤
│ Internal Codex (`codex-rs`)    │ [IMPLEMENTED & PRODUCTION]     │
│ External Runtime API (M10)     │ [IMPLEMENTED & TESTED]         │
│ OpenCode Adapter (M11)         │ [PLANNED]                      │
│ Claude Code Adapter (M12)      │ [PLANNED]                      │
│ External Codex CLI Adapter     │ [PLANNED]                      │
└────────────────────────────────┴────────────────────────────────┘
```

The vendor-neutral `agent-studios-external-runtime` crate defines the authoritative interface, strongly-typed identifiers, lifecycle state machine, tristate capability model, event streaming, and registry. Concrete CLI adapters for OpenCode and Claude Code are scheduled for M11 and M12.

---

## 3. The `AgentRuntime` Lifecycle Interface

External runtimes implement a standardized asynchronous lifecycle trait that decouples process supervision from internal Control Plane scheduling:

```rust
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    /// Returns the unique implementation identifier for this runtime.
    fn implementation_id(&self) -> &RuntimeImplementationId;

    /// Discovers runtime instances, local binary paths, and metadata in the environment.
    async fn discover(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError>;

    /// Advertises the static or dynamic capability profile supported by this runtime.
    async fn capabilities(&self) -> RuntimeCapabilities;

    /// Starts a new external runtime process or session according to the start request.
    async fn start(
        &self,
        request: RuntimeStartRequest,
    ) -> Result<RuntimeSessionHandle, RuntimeError>;

    /// Sends typed input (user text, continuation, or approval response) to an active session.
    async fn send(
        &self,
        session: &RuntimeSessionRef,
        input: RuntimeInput,
    ) -> Result<(), RuntimeError>;

    /// Interrupts active turn while preserving session context and process.
    async fn interrupt(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError>;

    /// Resumes execution after interruption if the runtime advertises `RuntimeCapability::Resume`.
    async fn resume(
        &self,
        session: &RuntimeSessionRef,
        input: Option<RuntimeInput>,
    ) -> Result<(), RuntimeError>;

    /// Terminates the runtime session permanently (idempotent if already Stopped).
    async fn stop(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError>;

    /// Queries the current lifecycle state of a session.
    async fn status(
        &self,
        session: &RuntimeSessionRef,
    ) -> Result<RuntimeLifecycleState, RuntimeError>;

    /// Subscribes to the broadcast stream of normalized events emitted by the session.
    async fn events(
        &self,
        session: &RuntimeSessionRef,
    ) -> Result<broadcast::Receiver<RuntimeEvent>, RuntimeError>;
}
```

---

## 4. Contract Hardening (Milestone M10.1)

Milestone M10.1 closed 12 architectural contract boundaries prior to concrete adapter development:

1. **Multi-Instance Discovery Authority**:
   - `AgentRuntime::discover()` returns `Vec<DiscoveredRuntimeInstance>`.
   - Each instance possesses unique `RuntimeInstanceId` (`rt-inst-<uuid>`) and typed `RuntimeAvailability` (`Available` or `Unavailable { reason: SanitizedRuntimeMessage }`).
2. **Typed Runtime Instance References**:
   - `RuntimeInstanceRef` (`implementation_id`, `instance_id`, `config_ref: Option<RuntimeConfigRef>`) verified at start boundary before process spawn.
3. **Three-Tuple Session Ownership Tokens**:
   - `RuntimeSessionRef` (`implementation_id`, `instance_id`, `session_id`) required across all lifecycle methods (`send`, `interrupt`, `resume`, `stop`, `status`, `events`).
   - Forged or mismatched tokens are rejected with typed `SessionInstanceMismatch` or `SessionImplementationMismatch`.
4. **Zero-Plaintext Secret Architecture**:
   - `RuntimeStartRequest` replaces raw environment maps with typed `EnvironmentVariableBinding`.
   - Secrets use deferred `SecretReference` and `SecretBackend` from `agent-studios-provider`.
5. **Structural Diagnostic Sanitization**:
   - `SanitizedRuntimeMessage` newtype with private inner storage scrubs API tokens and authorization headers on construction and deserialization.
6. **Production Export Cleanliness**:
   - Mock/fake runtime (`FakeAgentRuntime`) relocated to `tests/support/fake_runtime.rs`. Zero simulation mocks in production crate exports.
7. **Stop Semantics & Capability Gating**:
   - `stop()` on `Stopped` session is strictly idempotent (`Ok(())`).
   - `stop()` on `Completed` or `Failed` returns typed `TerminalStateError`.
   - `RuntimeCapability::Stop` checked before termination side effects.
8. **Fail-Closed Capability Enforcement**:
   - `RuntimeCapabilities::ensure_supported` rejects `Unknown` and `Unsupported` capabilities fail-closed.
9. **Concurrency & Registry Discipline**:
   - `RuntimeRegistry` uses `BTreeMap` for deterministic alphabetical ordering.
   - Lock-drop-before-await pattern guarantees no mutex/rwlock guards are held across `.await` points.
10. **Monotonic Event Sequence Authority**:
    - `RuntimeEvent.sequence` (1..N gapless u64) is the sole deterministic ordering authority. Wall-clock `timestamp` is observational metadata.
11. **Event Correlation Consistency**:
    - `RuntimeEventKind::SessionStarted` carries matching authoritative `session_id` and `instance_id`.
12. **Non-Resurrection Lifecycle Invariants**:
    - Terminal states (`Stopped`, `Completed`, `Failed`) forbid transition to active states. All operations on terminal sessions fail with `TerminalStateError`.

---

## 5. Process Containment & Isolation

Because external runtimes execute foreign CLI binaries (Node.js, Python, or Go processes), Agent Studios enforces strict process containment:

1. **Subprocess Sandboxing (Planned Candidate / Future Exploration in M10+)**:
   - On Windows: Windows Job Objects and AppContainer are planned candidates for process containment with CPU, memory, and handle quotas.
   - On Linux/macOS: Linux cgroups/namespaces and macOS sandbox-exec are future exploration for platform containment.
2. **Directory Confinement**:
   - External processes are strictly rooted in their assigned isolated Git worktree (`cwd`).
   - Access to parent repositories, global directories, or sibling worktrees is blocked at the OS or path-mapping level.
3. **Environment Sanitization**:
   - Environment variables are scrubbed. Global API keys and tokens are never leaked into the child process environment unless explicitly configured for that specific runtime instance.
4. **Structured Stdout / SSE Parsing**:
   - The Control Plane communicates with external agents over structured protocols (JSON-RPC over stdin/stdout or local SSE sockets).
   - Unstructured terminal output is captured in raw execution logs, but state transitions are driven strictly by typed events.

---

## 6. Planned Runtime Adapters

### A. OpenCode Adapter (Milestone M11)
- Wraps the OpenCode CLI or Node.js server.
- Maps OpenCode session steps into Agent Studios `Task` and `Run` entities.
- Binds OpenCode provider settings to Agent Studios `ProviderCatalog` instances.

### B. Claude Code Adapter (Milestone M12)
- Wraps Claude Code CLI running in headless or pipe mode.
- Translates Claude Code tool interactions into Agent Studios Control Plane tool events.
- Enforces Agent Studios pre-execution budgets and human-in-the-loop approvals over Claude Code tool calls.

### C. Codex CLI Standalone Adapter
- Launches external upstream `codex` binary for isolated benchmarking against internal `codex-rs` engine.
