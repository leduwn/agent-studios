# Agent Studios — External Runtimes Architecture

> **Status**: Interface Implemented & Hardened (Milestones M10 & M10.1 COMPLETED; Adapters M11–M12 PLANNED)
> **Current Reality**: Internal Codex (`codex-rs`) is the active production runtime. Vendor-neutral `agent-studios-external-runtime` interface, hardened contract boundaries, and 56-test contract verification suite (54 contract tests + 2 unit tests) are **IMPLEMENTED & VERIFIED**. Concrete adapters are **PLANNED**.
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
    /// Queries the current lifecycle state of a session.
    async fn status(
        &self,
        session: &RuntimeSessionRef,
    ) -> Result<RuntimeLifecycleState, RuntimeError>;

    /// Subscribes to the normalized event stream emitted by the session, delivering replay history followed by live events.
    async fn events(
        &self,
        session: &RuntimeSessionRef,
    ) -> Result<RuntimeEventSubscription, RuntimeError> {
        self.events_after(session, None).await
    }

    /// Subscribes to the normalized event stream after a specific sequence number.
    async fn events_after(
        &self,
        session: &RuntimeSessionRef,
        after_sequence: Option<u64>,
    ) -> Result<RuntimeEventSubscription, RuntimeError>;
}
```

---

## 4. Contract Hardening & Remediation (Milestone M10.1)

Milestone M10.1 systematically resolved all findings across the external runtime contract through two intensive review rounds:

### Round 1 Hardening Matrix (P1-01 to P2-04)

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
   - `RuntimeStartRequest::validate` strictly prohibits `ExecutionWorkspace::SharedSource` pending M11 verified containment isolation.
10. **P2-04: Fault-Tolerant RuntimeRegistry Discovery**:
    - `RuntimeRegistry::discover_all` uses `tokio::task::JoinSet` for parallel discovery across registered implementations.
    - Locks are dropped before `.await` calls to prevent lock contention and deadlocks.
    - Collects into `RegistryDiscoveryOutcome`, preserving deterministic `BTreeMap` ordering while isolating individual runtime failures into sanitized error diagnostics without failing the entire registry.

### Round 2 Source Review Remediation Matrix (R01 to R10)

- **R01 (P1): Event Ingest Boundary Validation**: `SessionEventHub::ingest()` routes through `EventBoundaryValidator` to enforce sequence 1 `SessionStarted`, gapless monotonic increments, session/instance correlation, and post-terminal rejection without partial state corruption on failure.
- **R02 (P1): Atomic Event Emission Synchronization**: `SessionEventHub::emit()` serializes sequence allocation, boundary validation, bounded history append, terminal state tracking, and live broadcast under a single atomic lock boundary (`Arc<RwLock<HubState>>`).
- **R03 (P1): Replay Retention and Offset Boundary Semantics**: Enforces that `after_sequence = N` replays events strictly greater than N; when earliest retained sequence is E, `after_sequence = Some(E - 1)` succeeds and replays from E; initial subscription (`after_sequence = None`) after eviction returns `EventRetentionExceeded`; capacity 1, buffer overflow, and sequence overflow handled cleanly.
- **R04 (P1): Secret Environment Variable Validation Bypass**: `EnvironmentVariableBinding` restricts field visibility (private `name` and `source`), implements custom serde deserialization enforcing constructor invariants, and re-validates all bindings at `RuntimeStartRequest::validate()`.
- **R05 (P1): Terminal StatusChanged Event Stream Finalization**: `RuntimeEventKind::is_terminal()` recognizes `StatusChanged` with terminal states `Stopped`, `Completed`, `Failed`, finalizing hub and subscription streams.
- **R06 (P1): `send()` Lifecycle Error Precedence**: `FakeAgentRuntime::send()` validates session reference ownership and terminal states before evaluating `fail_send`.
- **R07 (P2): Registry Worker Panic Attribution**: `RuntimeRegistry::discover_all()` tracks implementation identity across task panics and cancellations on `JoinSet` and reports typed sanitized failures in `outcome.failures`.
- **R08 (P1): Multi-URL and Query Parameter Credential Sanitization**: `sanitize_error_message()` iteratively processes and redacts all credential-bearing URIs and query parameters without early truncation.
- **R09 (P2): ReadOnly Workspace Policy Demonstrated Enforcement**: Fail closed on `ExecutionWorkspace::SharedSource` under all access modes (including `ReadOnly`) with `RuntimeError::InvalidWorkspaceAccess` pending M11 verified containment primitives.
- **R10 (P2): Documentation & Evidence Alignment**: Comprehensive alignment across architecture docs, roadmap, trait signatures, and passing test suites.

### Round 3 Targeted Closure Matrix (SR3-01 to SR3-08)

- **SR3-01 (P1): Replay Retention Bookkeeping & Capacity 1 Correctness**: Unified history append and retention management in `HubState::record_event()` ensures `earliest_retained_sequence` is continuously synchronized with `state.history.front().unwrap().sequence`, eliminating stale sequence indices when capacity is 1 or buffer evictions occur.
- **SR3-02 (P1): Strictly Single Authoritative SessionStarted Validation**: `EventBoundaryValidator` validates that sequence 1 must be `SessionStarted` matching session and instance IDs, and strictly rejects any duplicate `SessionStarted` attempt at `sequence > 1` without sequence counter advance or state mutation.
- **SR3-03 (P1): Panic Diagnostic Log Sanitization in Registry**: Registry worker panic and cancellation handlers use stable classified log messages without interpolating untrusted `JoinError` or panic payloads into `tracing::error!()`.
- **SR3-04 (P2): High-Concurrency Tokio Event Emission & Competing Terminals**: Real multi-task Tokio concurrency tests (`test_50_sr3_04_real_concurrent_event_emission`) with 10 producers, 100 events, barrier synchronization, and 5 competing terminal transitions verifying strict monotonicity, gapless sequencing, and single-winner terminal closure.
- **SR3-05 (P2): Registry Worker Panic Attribution with Sensitive Canary & Race Elimination**: Worker tasks map `handle.id()` synchronously on the spawning thread into `task_map`, eliminating task attribution races. Verified via `test_49_sr3_03_sr3_05_real_registry_worker_panic_attribution` with a synthetic sensitive credential canary that is never leaked to logs or outcome diagnostics.
- **SR3-06 (P2): Future Replay Offset Validation & Deterministic Terminal Completion**: Added typed `RuntimeError::InvalidReplayOffset` for subscription requests where `after_sequence > latest_committed`. Subscribing to an already-terminal session at latest committed sequence immediately sets `is_closed: true` and cleanly returns `Ok(None)` without hanging on broadcast.
- **SR3-07 (P2): Workspace Isolation Policy vs OS Sandboxing Scope Clarification**: Architecture docs, roadmap, and crate doc comments explicitly state that M10 enforces typed API and workspace policy boundaries at the request layer (rejecting `SharedSource`), while kernel/OS-level process sandboxing is deferred to M11.
- **SR3-08 (P2): Milestone Quality Gates & Review Artifacts**: 56 tests passing (54 contract tests + 2 unit tests), workspace fmt, clippy pedantic green, zero diff under `codex-rs/`, and complete review bundle exported.

---

## 5. Workspace Isolation Policy & Process Containment Scope

> **Important Boundary Clarification (SR3-07)**:  
> Milestone M10 enforces **typed policy boundaries** at the API, configuration, and request level:
>
> - Rejecting `ExecutionWorkspace::SharedSource` across all access modes (`ReadOnly` and `Mutating`) to prevent unmanaged host repository access.
> - Rejecting plaintext credentials and forbidden keywords in metadata and literal environment bindings.
> - Enforcing pre-spawn validation of instance identity, availability, and supported configurations.
>
> Milestone M10 does **NOT** provide kernel-level or OS-level process sandboxing (such as Linux cgroups, namespaces, seccomp, AppArmor, or Windows Job Objects/AppContainer). OS-level process isolation and containment primitives are explicitly scheduled for **Milestone M11** alongside concrete CLI adapters.

1. **Subprocess Sandboxing (Deferred to M11)**:
   - On Windows: Windows Job Objects and AppContainer will be evaluated in M11 for OS-level CPU, memory, network, and handle quotas.
   - On Linux/macOS: Linux cgroups/namespaces and platform containment will be implemented in M11 alongside adapter execution.
2. **Directory Confinement**:
   - External processes are strictly rooted in their assigned isolated Git worktree (`cwd`).
   - Access to parent repositories, global directories, or sibling worktrees is blocked at the workspace policy level in M10 and at the OS boundary in M11.
3. **Environment Sanitization**:
   - Environment variables are scrubbed. Global API keys and tokens are never leaked into the child process environment unless explicitly configured for that specific runtime instance via validated `SecretReference` bindings.
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
