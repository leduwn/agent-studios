# Agent Studios — External Runtimes Architecture

> **Status**: Interface Implemented (Milestone M10 COMPLETED; Adapters M11–M12 PLANNED)
> **Current Reality**: Internal Codex (`codex-rs`) is the active production runtime. Vendor-neutral `agent-studios-external-runtime` interface and contract test matrix are **IMPLEMENTED**. Adapters are **PLANNED**.
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

    /// Discovers runtime availability, local binary path, and metadata in the environment.
    async fn discover(&self) -> Result<DiscoveredRuntime, RuntimeError>;

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
        session_id: &RuntimeSessionId,
        input: RuntimeInput,
    ) -> Result<(), RuntimeError>;

    /// Interrupts the active execution turn while keeping the session process and context intact.
    async fn interrupt(&self, session_id: &RuntimeSessionId) -> Result<(), RuntimeError>;

    /// Resumes execution after interruption if the runtime advertises `RuntimeCapability::Resume`.
    async fn resume(
        &self,
        session_id: &RuntimeSessionId,
        input: Option<RuntimeInput>,
    ) -> Result<(), RuntimeError>;

    /// Terminates the runtime session permanently.
    async fn stop(&self, session_id: &RuntimeSessionId) -> Result<(), RuntimeError>;

    /// Queries the current lifecycle state of a session.
    async fn status(
        &self,
        session_id: &RuntimeSessionId,
    ) -> Result<RuntimeLifecycleState, RuntimeError>;

    /// Subscribes to the broadcast stream of normalized events emitted by the session.
    async fn events(
        &self,
        session_id: &RuntimeSessionId,
    ) -> Result<broadcast::Receiver<RuntimeEvent>, RuntimeError>;
}
```

---

## 4. Process Containment & Isolation

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

## 5. Planned Runtime Adapters

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
