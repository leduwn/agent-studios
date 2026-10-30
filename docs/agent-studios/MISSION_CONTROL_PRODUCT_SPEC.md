# Agent Studios — Mission Control Workbench Product Specification
**Milestone:** M14 Mission Control Workbench  
**Status:** Approved Architectural Document  
**Date:** 2026-10-30  
**Target:** VS Code / Code-OSS Extension Host (`apps/agent-studios-vscode`)  

---

## 1. Product Intent & 7 Core Inquiries

Agent Studios is not merely an AI chat sidebar. Mission Control adds a distinct, editor-level observability, review, and monitoring surface answering 7 core inquiries:

1. **What projects, studios, sessions, and runs exist, and which are active?**
2. **Which agents are working, on which tasks, and in which isolated worktrees?**
3. **Which tasks are blocked, in progress, ready for review, or completed?**
4. **What tools ran, what changed, what did tests report, and what is waiting for approval?**
5. **How do worktrees, branches, PRs, CI status, and review findings relate to this run?**
6. **Which provider/model/runtime was actually used; is cost/usage data truly available?**
7. **What should the human do next, and what evidence supports a completion claim?**

### Operational Boundary
- **Sidebar Chat (`agent-studios.chat`):** Used for conversational instruction, prompt composition, and streaming responses.
- **Editor Mission Control (`agent-studios.missionControl`):** Used for observability, review, DAG navigation, and system diagnostics.
- **Shared State Synchronization:** Selected studio/session/task synchronizes bi-directionally across both surfaces. Mission Control never creates its own competing state engine.

---

## 2. Information Hierarchy & Scope

The data model maintains strict ontological separation across hierarchy tiers:
```
Workspace / Project ──► Studio ──► Logical Session ──► Run ──► Task ──► Tool Call / Event / Artifact
```
*Rule:* Never conflate Rust `StudioId`, `SessionId`, `RunId`, or `TaskId`.

### Truthful Status Semantics & Constraints
1. **Succeeded State:** Attributed only when supported by true terminal runtime evidence (exit code 0, verified test suite completion).
2. **Progress Percentage:** Rendered only when a stable, meaningful task denominator exists. Otherwise, display `X / Y known tasks` or count badges. If tasks can be added dynamically, progress is labeled as *current plan coverage*, never a speculative remaining-work estimate.
3. **Token & Cost Metrics:** Rendered only when observed and attributed from actual backend telemetry; never fabricated or simulated. Display explicit "Not available" when telemetry is absent.
4. **GitHub & CI Checks:** Displayed only from authenticated, authorized sources; never inferred or guessed from branch names.

---

## 3. 9-Tab Multi-Tab Workbench Overview

| Tab ID | Tab Name | Core Responsibilities |
| :--- | :--- | :--- |
| `overview` | **Overview Dashboard** | Attention-first summary (pending approvals, failed runs, event gaps), studio scope, KPI status cards, recent events. |
| `fleet` | **Agents Fleet** | Coordinator/workers table, runtime adapter (`codex-rs`, `opencode`), model, assigned task, elapsed time, and slide-out Agent Inspector drawer. |
| `tasks` | **Tasks & DAG** | Searchable task list, 7-state Kanban board, and zero-dependency SVG DAG dependency graph with topological rank layout. |
| `runs` | **Run Inspector** | Active execution run inspector, execution progress meter, telemetry stream, stdout/stderr terminal box with ANSI rendering. |
| `timeline` | **Activity Timeline** | Chronological event stream with monotonically ordered sequence numbers, actor badges, event icons, and JSON payload inspector. |
| `review` | **Review View** | Workspace change review, modified file list with additions/deletions, one-click launch of native `vscode.diff` via `DiffReviewManager`. |
| `approvals` | **Approvals Center** | Pending human-in-the-loop approvals with risk classification badges (Low, Medium, High, Critical), diff preview, and fail-closed security banner. |
| `providers` | **Providers & Usage** | Configured LLM provider instances, context window indicators, latency metrics, and secret scrubbing verification banner. |
| `diagnostics` | **Diagnostics** | IPC socket connection status, agreed protocol capabilities, sequence watermarks, active clients, and connection troubleshooting. |

---

## 4. UI Read Model Schema (Conceptual Projection)

The UI projects state exclusively from validated read models:
- **`MissionScope`:** `workspaceUri`, `studioId`, `sessionId?`, `lastSyncAt`, `source`.
- **`MissionSummary`:** Status counts, `totalKnownTasks`, `finishedTaskCount`, `needsAttention` list, `activeRunIds`, `dataQuality`.
- **`AgentProjection`:** `agentId`, `runId`, `taskId`, `role`, `runtimeId`, `modelRef`, `status`, `lastEventAt`.
- **`TaskProjection`:** `taskId`, `dependencies`, `state`, `assignedAgentId?`, `blockedReason?`, `runId?`, `artifactIds?`.
- **`RunProjection`:** `runId`, `taskId`, `lifecycle`, `startedAt?`, `completedAt?`, `runtimeEvidence?`, `workspaceRef?`.
- **`TimelineEvent`:** `sequence`, `sourceTimestamp`, `type`, scope IDs, redacted summary, replay status.
- **`ApprovalProjection`:** `approvalId`, correlation IDs, `expiry`, `risk`, `status`, `authCapability`, delivery status.
- **`ArtifactProjection`:** `id`, `path`, safe file URI, change type, verification status.
- **`QualityStatus`:** `LIVE`, `RECOVERING`, `DEGRADED`, `STALE`, `UNSUPPORTED`, `PREVIEW`.

---

## 5. Delivery Outcome Classifications

All delivery milestones report outcomes across 4 distinct dimensions:
1. **`UI_COMPLETE`:** All 9 tabs, responsive styles, high-contrast themes, SVG DAG layout, and native diff launches fully implemented and verified via automated unit and integration tests.
2. **`READ_ONLY_LIVE_CONNECTED`:** Read-only state projections bound to live daemon queries (conditional upon IPC-03 wire alignment).
3. **`MUTATION_READY`:** State-changing approval and execution commands guarded by strict fail-closed security boundaries.
4. **`NOT_TESTED`:** Features deferred or unverified (e.g., drag-and-drop task reassignment, autonomous PR merge).
