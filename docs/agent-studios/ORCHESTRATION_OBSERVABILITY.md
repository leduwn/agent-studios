# Agent Studios — Orchestration Observability Specification

> **Status**: Core UX & Architecture Specification
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Overview & Observability Philosophy

Agent Studios rejects both extremes of AI agent interfaces:
1. **The Opaque Chatbot**: A single chat input that silently runs commands behind the scenes with zero architectural visibility.
2. **The Sci-Fi Swarm Dashboard**: An overwhelming, noisy SaaS dashboard with glowing neon nodes, buzzing chat rooms, and simulated agent banter.

Instead, Agent Studios provides **orchestration observability as a quiet, precise, developer-grade lens**:
- Read models are projected deterministically from the Control Plane event stream.
- Structured agent telemetry is exposed without terminal scraping.
- Visualizations use quiet monochrome styling.
- Secrets are zeroized and never leak into inspectable state.

---

## 2. Event-Sourced Read Models (`agent-studios-orchestration`)

The `agent-studios-orchestration` crate consumes raw `ControlPlaneEvent` envelopes and maintains synchronized read-model snapshots:

```rust
pub struct OrchestrationStateProjection {
    pub task_graph: TaskGraphSnapshot,
    pub agents: HashMap<AgentId, AgentSummary>,
    pub runs: HashMap<RunId, RunSummary>,
    pub active_approvals: Vec<ApprovalInboxItem>,
    pub worktrees: HashMap<WorktreeId, WorktreeSnapshot>,
    pub artifacts: ArtifactIndexSnapshot,
}
```

### Deterministic Invariants
- **Zero Polling**: Projections update reactively via Tokio broadcast subscription.
- **Ordered Determinism**: Task lists and agent lists are sorted by canonical keys (`created_at`, `TaskId`, alphabetical alias) so renders are 100% stable across reloads.
- **Replay Parity**: Replaying 1,000 historical events produces the exact same projection as receiving 1,000 live events.

---

## 3. Interactive Task Graph UX Specification

The interactive **Task Graph** visualizes the active plan and dependency structure:

```text
        Architecture
             │
      ┌──────┴──────┐
      ▼             ▼
   Backend       Frontend
      │             │
      └──────┬──────┘
             ▼
           Testing
             │
             ▼
           Review
```

### Interactive Capabilities
- **Viewport Manipulation**: Smooth pan, pinch/wheel zoom, fit to viewport, center selected node, reset camera, mini-map navigator.
- **Keyboard Traversal**: Arrow keys traverse dependency edges; Tab navigates nodes; Enter opens node details.
- **Auto-Layout Engine**: Hierarchical directed acyclic graph layout (Sugiyama / top-to-bottom or left-to-right) preserving edge directionality without crossing spaghetti.
- **Focus & Follow**: Auto-focus follows the currently running task; filter by status (`Ready`, `Running`, `Blocked`, `Succeeded`, `Failed`, `Cancelled`).

### Subtle Animated Connectors
Connectors along dependency edges provide quiet, informative visual cues:
- **Task Start**: Gentle progressive pulse along incoming edge.
- **Worker Assignment**: Brief node border accent when a worker agent binds to the task.
- **Dependency Unblocking**: Subtle forward pulse when an upstream dependency transitions to `Succeeded`.
- **Artifact Handoff**: Small packet indicator moving from task to integration workspace.
- **Approval Waiting**: Steady, soft amber indicator on node waiting for human decision.
- **Strict Visual Restraint**: No glowing lasers, neon borders, or pulsating cyberpunk rings. Monochrome and quiet.

---

## 4. The Approval Inbox

Human-in-the-loop approvals are centralized in the **Approval Inbox**:

```text
┌────────────────────────────────────────────────────────┐
│ APPROVAL INBOX (1 PENDING)                             │
├────────────────────────────────────────────────────────┤
│ ⚠ Tool Execution Request                               │
│ Agent: Coder (Worker 1)                                │
│ Task:  Backend Authentication                          │
│ Tool:  exec_command                                    │
│ Command: `npm install @auth/core`                      │
│ CWD:   .git/agent-studios/worktrees/feat-auth/         │
│ Reason: Required for session token handling            │
│                                                        │
│ [ Approve Once ]  [ Approve for Run ]  [ Deny ]        │
└────────────────────────────────────────────────────────┘
```

### Principles
- **No Background Blocking**: A pending approval blocks only the requesting task; independent tasks continue running.
- **Granular Actions**: Approve once, approve for the remainder of the active run, or deny with a custom reason message returned to the agent.
- **Zero Plaintext Secrets**: Any command containing resolved environment secrets displays obfuscated tokens (`***`).

---

## 5. Structured Agent Activity (No Terminal Scraping)

Agent Studios explicitly rejects parsing raw stdout or scraping terminal ANSI sequences to deduce agent state:

- Tool starts, completions, and errors are emitted as typed `ControlPlaneEvent::ToolStarted`, `ToolCompleted`, and `ToolFailed`.
- The Activity Feed renders structured entries:
  - Timestamp, Agent Alias, Tool Name, Duration (ms), Outcome status.
- File diffs are displayed using native Monaco diff viewer components driven by structured patch deltas, not raw terminal diff dumps.

---

## 6. Context Inspector & Zero Plaintext Secrets

The **Context Inspector** allows developers to inspect the active token window and state of any agent:

1. **System Prompt & Role**: Base system instructions and active role guidelines.
2. **`AGENTS.md` Context**: Repository guidelines discovered from the workspace hierarchy.
3. **Active Skills & Tools**: Loaded MCP tools and Skills (`SKILL.md`).
4. **Token Usage & Compaction**: Total context window, prompt tokens, completion tokens, cached tokens, and history compaction status.

### Zero Secrets Guarantee
- Environment variables containing API keys (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, custom headers) are **never visible** in the Context Inspector.
- Secret references are displayed strictly as `SecretRef(ENV: ANTHROPIC_API_KEY) [CONFIGURED]`.
- Plaintext credentials never touch inspector memory, JSON serializations, or client UI trees.
