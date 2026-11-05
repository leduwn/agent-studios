# Agent Studios — Mission Control & UI Specification

> **Status**: Authoritative Mission Control & Observability Specification  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Reference**: Implements Milestone M14 (`MISSION_CONTROL_PRODUCT_SPEC.md`, `CODE_OSS_INTEGRATION.md`, PR #21, #22, #23, #25)

---

## 1. Executive Summary & Purpose

`[EXISTING_CANONICAL_DECISION]` & `[ASSISTANT_OR_CLAUDE_PROPOSAL]`
Mission Control is the dedicated developer observability workbench in Agent Studios. It bridges the gap between passive chat interfaces and complex multi-agent execution, answering seven fundamental engineering inquiries in real time:

1. **What projects, studios, sessions, and runs exist, and which are active?**
2. **Which agents are working, on which tasks, and in which isolated worktrees?**
3. **Which tasks are blocked, in progress, ready for review, or completed?**
4. **What tools ran, what changed, what did tests report, and what waits for approval?**
5. **How do worktrees, branches, PRs, CI status, and review findings relate to the active run?**
6. **Which provider, model, and runtime ran; is token cost and usage data verified?**
7. **What should the human do next, and what concrete evidence supports the completion claim?**

---

## 2. Operational Separation of UI Surfaces

To preserve ergonomic clarity, Agent Studios divides responsibilities between two tightly synchronized surfaces:

```text
┌───────────────────────────────────────┬─────────────────────────────────────────────────┐
│        SIDEBAR CONTAINER              │              EDITOR WORKBENCH TAB               │
│       (agent-studios.chat)            │         (agent-studios.missionControl)          │
├───────────────────────────────────────┼─────────────────────────────────────────────────┤
│  - Natural language goal input        │  - 9-tab deep observability dashboard           │
│  - Conversational message thread      │  - Visual SVG DAG task graph & Kanban board     │
│  - Real-time token streaming          │  - Side-by-side vscode.diff launch              │
│  - Quick approval action prompts      │  - Fleet agent table & worktree inspector       │
│  - Ergonomic width (300-450px)        │  - Monotonic JSON event inspector & diagnostics │
└───────────────────────────────────────┴─────────────────────────────────────────────────┘
```

Both surfaces communicate bi-directionally over the shared backend session via local IPC.

---

## 3. The 9-Tab Workbench Layout

`[ASSISTANT_OR_CLAUDE_PROPOSAL]`
Mission Control is structured into 9 focused tabs within a single full-screen editor webview:

### 3.1 Tab 1: Overview (`overview`)
- **Attention-First Header**: Displays urgent blockers requiring human intervention (pending approvals, failed tasks, merge conflicts).
- **KPI Status Cards**: Active Run Duration, Task Completion Ratio (`X / Y tasks`), Total Verified Tokens, Active Worktree Leases.
- **Recent Event Highlights**: Last 5 significant transitions with relative timestamps and actor badges.

### 3.2 Tab 2: Fleet Inspector (`fleet`)
- **Agent Roster Table**:
  - `Agent ID`: Strongly-typed identifier (`agent-<uuid>`).
  - `Role`: Coordinator, Architect, Coder, Tester, Reviewer, Security.
  - `Runtime / Adapter`: Internal Codex, Claude Code CLI, OpenCode.
  - `Model Target`: Explicit `ModelRef` (`provider_instance_id:model_id`).
  - `Status`: `Idle`, `Busy`, `Waiting`, `Stopped`.
  - `Assigned Task`: Direct link to active task node.
- **Inspector Drawer**: Click an agent row to view turn history, memory usage, and context token breakdown.

### 3.3 Tab 3: Tasks & Dependency Graph (`tasks`)
- **Dual Presentation**:
  - **Kanban Board**: 5 columns (`Pending`, `Blocked`, `Ready`, `Running`, `Terminal: Succeeded / Failed / Cancelled`).
  - **Zero-Dependency SVG DAG**: Interactive visual dependency graph using Kahn's topological layout. Nodes display task state icons; directed edges indicate blocking dependencies.
- **Search & Filtering**: Filter by title, assignee, or state.

### 3.4 Tab 4: Runs Inspector (`runs`)
- **Run Metadata**: `RunId`, start timestamp, initiator, budget ceilings (max tokens, max turns).
- **Execution Progress**: Normalized progress meter based on verified task completion.
- **Console Output**: ANSI-colored terminal log capturing process stdout/stderr from active tools.

### 3.5 Tab 5: Timeline & Audit Stream (`timeline`)
- **Monotonic Event Log**: Chronologically ordered stream of all Control Plane events (`SessionStarted`, `TaskCreated`, `AgentSpawned`, `ToolExecuted`, `ArtifactCreated`).
- **Sequence Watermarks**: Displays `seq` numbers, detecting and alerting on any event sequence gaps.
- **JSON Payload Inspector**: Expand any event to inspect full strongly-typed JSON wire payloads.

### 3.6 Tab 6: Diff & Artifact Review (`review`)
- **Modified File Tree**: Shows modified, added, and deleted files across all task worktrees.
- **Lossless Multi-Chunk Git Diffs**: Integrates `crates/workspace` lossless diff streaming (PR #29).
- **One-Click Native Diff**: Click any file to invoke native `vscode.diff(originalUri, modifiedUri)` side-by-side in Monaco editor via `DiffReviewManager`.

### 3.7 Tab 7: Approval Inbox (`approvals`)
- **Risk-Tiered Queue**: Displays pending action requests with explicit risk classifications:
  - `Low`: Safe reads, schema checks.
  - `Medium`: Local test executions, linters.
  - `High`: File modifications, dependency installations.
  - `Critical`: Git branch operations, destructive commands, external network calls.
- **Action Controls**: `[ Approve ]`, `[ Deny ]`, and optional `Feedback Note` input field.

### 3.8 Tab 8: Provider Catalog (`providers`)
- **Configured Instances**: Lists active providers (Anthropic, OpenAI, Gemini, Ollama) with base URLs and latency metrics.
- **Context Limits**: Advertises model context window capacities and maximum output token ceilings.
- **Secret Scrubbing Verification**: Confirms that API keys are masked (`••••••••`) and stored strictly in-memory.

### 3.9 Tab 9: System Diagnostics (`diagnostics`)
- **IPC Transport Status**: Connected Named Pipe (Windows: `\\.\pipe\agent-studios-<user>`) or Unix Domain Socket (`/tmp/agent-studios-<user>.sock`).
- **Process Telemetry**: Native `agsd` PID, RSS memory footprint, CPU utilization, active child process count.
- **Capability Matrix**: Real-time display of runtime feature negotiation (16 tristate capability dimensions).

---

## 4. Truthful Telemetry Constraints

`[EXISTING_CANONICAL_DECISION]`
To prevent false operator confidence, Mission Control enforces strict telemetry truthfulness rules:

1. **No Fake Progress Bars**: Progress percentages are rendered only when a stable, discrete task denominator exists. In open-ended exploratory tasks, the UI displays item counts (`X completed`) instead of arbitrary percentages.
2. **Terminal Evidence Required**: A task is never marked `Succeeded` based on LLM narrative claims alone. Success requires verified process completion (compiler exit code 0, test runner exit 0).
3. **No Estimated Costs**: Token counts and financial costs are displayed only when backed by authenticated provider usage headers. If telemetry is unavailable, the UI renders `Usage not reported` rather than guessing.
4. **Verified CI & Git Status**: Branch and PR status must reflect verified Git refs or authenticated GitHub API queries; they are never inferred from branch names.
