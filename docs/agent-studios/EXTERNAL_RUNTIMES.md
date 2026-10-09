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

## 4. Contract Hardening & Remediation (Milestone M10.1)

Milestone M10.1 systematically resolved all findings across the external runtime contract:

1. **P1-01: Reliable Event Startup, Subscription, and Gapless Replay**:
   - `SessionEventHub` coordinates atomic monotonic sequence generation (`EventSequencer`), bounded in-memory replay history (`VecDeque<RuntimeEvent>`), and real-time broadcast (`tokio::sync::broadcast`).
   - `RuntimeEventSubscription` drains historical events prior to yielding live broadcast events, with sequence deduplication, sequence gap detection (`EventStreamGap`), buffer lag detection (`EventStreamLagged`), and clean terminal closure.
   - `events_after` and `replay` fail closed with `EventRetentionExceeded` when requested offsets fall outside retained history.
2. **P1-02: Elimination of Plaintext Credential Bypass**:
   - `NonSecretValue` custom serializer/deserializer enforces keyword rejection (`KEY`, `TOKEN`, `SECRET`, `PASSWORD`, `PASSWD`, `AUTH`, `BEARER`) and prefix rejection (`sk-`, `ghp_`, `gho_`, `xoxb-`, `xoxp-`, `glpat-`, `npm_`).
   - `EnvironmentVariableBinding::literal` and `RuntimeStartRequest::with_metadata` strictly reject sensitive values.
   - Secret injection requires `EnvironmentBindingSource::Secret` wrapping validated `SecretReference`.
3. **P1-03: Strict Pre-Spawn Start Validation**:
   - `validate_instance_start` and `DiscoveredRuntimeInstance::validate_start_request` enforce implementation identity, instance membership, availability state (`RuntimeAvailability::Available`), supported configuration authority (`supports_config`), and workspace isolation prior to handle creation or side-effect execution.
4. **P1-04: Canonical Lifecycle Precedence and Terminal Semantics**:
   - Standardized precedence ordering: Session reference validation -> Terminal state check -> Idempotent stop check -> Per-instance capability check -> State transition validation -> Failure injection -> State mutation -> Correlated event emission.
   - `stop()` on `Stopped` session is strictly idempotent (`Ok(())`).
   - Operations on terminal states (`Completed`, `Failed`, `Stopped`) fail closed with typed `TerminalStateError`.
5. **P1-05: Authoritative Event Sequence Enforcement**:
   - `EventSequencer` strictly generates monotonic sequences starting at 1.
   - `EventBoundaryValidator` validates ingestion streams: sequence 1 must begin with `SessionStarted`, sequence numbers must be gapless and strictly increasing, session correlation must match, and no events may follow terminal lifecycle states.
6. **P1-06: Per-Instance Capability Authority**:
   - Runtime instances declare per-instance `capabilities: RuntimeCapabilities` in `DiscoveredRuntimeInstance`.
   - `RuntimeSessionHandle` and session state track instance-specific capabilities, overriding global implementation defaults so that restricted instances cannot execute unauthorized actions (e.g. stop/interrupt/resume).
7. **P2-01: RuntimeConfigRef Deserialization Integrity**:
   - Custom deserialization on `RuntimeConfigRef` validates trimmed non-empty strings, rejecting whitespace-only, empty, and invalid config identifiers.
8. **P2-02: Comprehensive Diagnostic Sanitization Coverage**:
   - `sanitize_error_message` and `SanitizedRuntimeMessage` scrub credentials across all cases: case-insensitive keywords, single/double quoted values, query string parameters (`?key=...`, `&token=...`), and basic authentication URIs (`https://user:pass@host/`).
9. **P2-03: ExecutionWorkspace Isolation Policy**:
   - `WorkspaceAccessMode` enum (`ReadOnly`, `Mutating`).
   - `RuntimeStartRequest::validate` strictly prohibits `ExecutionWorkspace::SharedSource` when `WorkspaceAccessMode::Mutating` is configured.
10. **P2-04: Fault-Tolerant RuntimeRegistry Discovery**:
    - `RuntimeRegistry::discover_all` uses `tokio::task::JoinSet` for parallel discovery across registered implementations.
    - Locks are dropped before `.await` calls to prevent lock contention and deadlocks.
    - Collects into `RegistryDiscoveryOutcome`, preserving deterministic `BTreeMap` ordering while isolating individual runtime failures into sanitized error diagnostics without failing the entire registry.

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
