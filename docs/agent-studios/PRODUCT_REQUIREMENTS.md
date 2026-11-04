# Agent Studios — Product Requirements Document (PRD)

> **Status**: Authoritative Product Requirements Baseline  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Reference**: Implements product directives from `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`

---

## 1. Functional Requirements

### 1.1 Goal Delegation & Task Planning
- `[USER_DIRECT_REQUIREMENT]` The system shall provide an intuitive natural language input interface in **Agent Mode** allowing operators to submit high-level technical goals.
- `[EXISTING_CANONICAL_DECISION]` The cognitive Coordinator shall decompose submitted goals into a Directed Acyclic Graph (DAG) of discrete tasks with explicit dependencies.
- `[IMPLEMENTED_BEHAVIOR]` The Control Plane shall mathematically validate DAG acyclicity using Kahn's algorithm before scheduling any tasks.
- `[IMPLEMENTED_BEHAVIOR]` Tasks with unmet upstream dependencies shall transition to `TaskState::Blocked(UnresolvedDependency)`.

### 1.2 Multi-Agent Execution & Worktree Isolation
- `[IMPLEMENTED_BEHAVIOR]` Mutating tasks shall execute in physically isolated Git worktrees located under `.git/agent-studios/worktrees/<session_id>/<task_id>/`.
- `[EXISTING_CANONICAL_DECISION]` A worker thread's working directory (`cwd`) is strictly bound to its assigned worktree and cannot change during an active turn.
- `[IMPLEMENTED_BEHAVIOR]` The system shall enforce a single-writer mutating lease per worktree via `WorkspacePolicyArbitrator`.
- `[IMPLEMENTED_BEHAVIOR]` Workers shall capture file changes using an isolated ephemeral Git index (`GIT_INDEX_FILE`) without altering the primary repository index.
- `[IMPLEMENTED_BEHAVIOR]` Changes shall be reconciled into the target branch via `git apply --check` and `git apply --3way`. If merge conflicts occur, the task shall transition to `TaskState::Blocked(ReconciliationConflict)`.

### 1.3 Interactive Approval Gates
- `[USER_DIRECT_REQUIREMENT]` The system shall provide an interactive approval mechanism for high-risk actions (file modifications, command executions, network requests).
- `[EXISTING_CANONICAL_DECISION]` Approvals shall be categorized into risk tiers:
  - **Low**: Read-only operations, non-destructive queries.
  - **Medium**: Local non-destructive command execution (`npm test`, `cargo check`).
  - **High**: File write/patch applications, dependency installations.
  - **Critical**: Git branch operations, destructive commands, external network transfers.
- `[IMPLEMENTED_BEHAVIOR]` Live AI execution remains gated by default in `CapabilityMatrix`; mutations fail closed without explicit approval.

### 1.4 Dual-Mode Presentation & Zero-Loss Switching
- `[USER_DIRECT_REQUIREMENT]` The desktop application shall provide two complementary presentation views:
  - **Agent Mode**: Clean, conversational, progressive disclosure interface for goal delegation.
  - **IDE Mode**: Full Code-OSS workbench (Monaco editor, LSP, DAP, Git, Terminal) for hands-on development.
- `[EXISTING_CANONICAL_DECISION]` Both modes shall operate over **one shared backend session** without process restart or data loss.

---

## 2. Zero-Loss Mode Switching Specification

### 2.1 The Shared Session Kernel
The persistent backend session (`session_id`) maintains the authoritative state across mode transitions:

```text
┌─────────────────────────────────────────────────────────────────┐
│                    SHARED SESSION KERNEL                        │
├────────────────────────────────┬────────────────────────────────┤
│ Session Identity:              │ Orchestration State:           │
│  - session_id (UUID v4)        │  - active run & budgets        │
│  - workspace source_root & cwd │  - task_graph (DAG topology)   │
├────────────────────────────────┼────────────────────────────────┤
│ Worker State:                  │ Audit & Telemetry:             │
│  - registered agents & roles   │  - monotonic event log         │
│  - assigned ModelRef instances │  - complete tool_history       │
├────────────────────────────────┼────────────────────────────────┤
│ Isolation & Versioning:        │ Human Interaction:             │
│  - allocated Git worktrees     │  - interactive approvals queue │
│  - content-addressed artifacts │  - prompt & chat transcript    │
└────────────────────────────────┴────────────────────────────────┘
```

### 2.2 Mode Transition Guarantees
When the operator clicks `[ Open in IDE ]` or `[ Back to Agent ]`:
1. `NO_AGENT_RESTART`: Active LLM generations and subprocess tool executions continue without interruption.
2. `NO_CONTEXT_LOSS`: Conversation history and token counts remain identical.
3. `NO_TOOL_HISTORY_LOSS`: Terminal outputs and tool logs remain intact.
4. `NO_STREAMING_LOSS`: Active Server-Sent Events (SSE) token streams remain attached.
5. `NO_TASK_GRAPH_LOSS`: DAG states persist in the Control Plane event log.
6. `NO_SILENT_WORKSPACE_SWITCH`: File system bindings remain strictly frozen.
7. `NO_NEW_SESSION`: Session UUID is preserved.
8. `NO_RUN_CANCELLATION`: Active runs continue execution to completion.

---

## 3. Non-Functional Requirements

### 3.1 Performance & Resource Efficiency
- **Daemon Footprint**: Native `agsd` daemon binary < 25 MB; idle memory usage < 60 MB RSS.
- **IPC Latency**: Local Named Pipe / Unix Domain Socket round-trip latency < 5 ms.
- **DAG Scheduling**: Kahn's algorithm cycle detection and ready-task dispatch < 1 ms for 100 nodes.
- **Disk Reclamation**: Worktrees cleanly pruned upon completion; dirty worktrees preserved safely.

### 3.2 Security & Sandboxing
- `[EXISTING_CANONICAL_DECISION]` Zero Secret Leakage: API keys and credentials stored in-memory using zeroizing types (`SecretString`). Scrubbed from event logs, diagnostics, and telemetry.
- `[IMPLEMENTED_BEHAVIOR]` OS-Level Sandboxing:
  - Windows: Win32 Job Objects (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) ensuring clean subprocess tree termination.
  - Linux/macOS: Dedicated process groups (`setpgid`) with signal trapping.
- `[IMPLEMENTED_BEHAVIOR]` IPC Security: Length-prefixed binary framing, HMAC-SHA256 handshake authentication, single-writer advisory lock.

### 3.3 Localization & Internationalization
- `[USER_DIRECT_REQUIREMENT]` Full bilingual UI support for **English** and **Vietnamese**.
- `[USER_DIRECT_REQUIREMENT]` Decoupling invariant: UI localization setting does not force or constrain the LLM's response language. If an English UI user prompts in Vietnamese, the agent replies in Vietnamese.

### 3.4 Visual Language & Ergonomics
- `[USER_DIRECT_REQUIREMENT]` **Useful / Native / Quiet**: Monochrome aesthetic, zero neon gradients, zero sci-fi scanlines.
- `[EXISTING_CANONICAL_DECISION]` Single Activity Bar Item: Exactly ONE icon (`Agents`) added to the Code-OSS Activity Bar. All sub-views reside in the `Agents` sidebar container.

---

## 4. User Journeys & Acceptance Criteria

### Journey A: Autonomous Feature Delivery
```text
User Submits Goal
       │
       ▼
Coordinator Generates DAG
       │
       ▼
Control Plane Validates Acyclicity
       │
       ▼
Worker Spawns in Isolated Worktree
       │
       ▼
Approval Triggered for Package Install (Risk: Medium)
       │
       ▼
User Approves Action
       │
       ▼
Worker Completes -> Ephemeral Index Patch Captured
       │
       ▼
3-Way Patch Reconciliation Merges into Target Branch
       │
       ▼
Tests Pass (Exit Code 0) -> Succeeded Status Emitted
```

**Acceptance Criteria**:
- Working tree of main workspace remains 100% clean during worker execution.
- If approval is denied, tool execution aborts fail-closed and the agent transitions to waiting.
- Succeeded status emitted only upon real compiler and test exit code 0.

### Journey B: Zero-Loss Mode Switch
1. User enters complex refactor prompt in Agent Mode.
2. Worker begins generating code and streaming output.
3. User clicks `[ Open in IDE ]` mid-stream.
4. UI transitions to Code-OSS layout instantly (< 500 ms).
5. Streaming continues in `Agents` sidebar without token loss.
6. Developer opens Monaco editor, edits adjacent files, and runs debugger without interfering with worker.
7. Acceptance Criteria: Zero agent restarts, identical session UUID, uncorrupted diff output.
