# M15 UI-06 — Report 02: Gap Analysis Matrix

## Metadata
- **Milestone**: M15 UI-06: Agent-First Experience, Shared Session Continuity & Real Daemon Integration Acceptance
- **Worktree**: `D:\Projects\agent-studios-active\m15-ui-06`
- **Branch**: `feat/m15-ui-06-agent-ide-continuity`
- **Date**: 2026-11-05
- **Status**: **RESOLVED & IMPLEMENTED**

---

## 1. Requirement vs Implementation Matrix

| Requirement Area | Specification / Expected Behavior | Pre-M15 UI-06 Baseline State | M15 UI-06 Resolved State | Verification Proof |
|---|---|---|---|---|
| **Welcome Surface** | Clean empty state with goal formulation starters, active workspace binding, daemon status, and session restore | Minimal placeholder with basic chat prompt input | Rich `agent-welcome-surface` featuring 4 goal starters, workspace card, daemon badge, and 1-click restore | `AgentIdeContinuity.test.ts` (Tests 1-4) |
| **Model Catalog** | Honest model catalog without synthetic dynamic models; explicit disclosure of pending M15.1 backend API | Static adapters displayed without clear backend readiness status | Explicit notice: "Dynamic Provider Discovery pending M15.1. Static adapters: Claude Code, Internal Codex, Codex." | `AgentIdeContinuity.test.ts` (Test 3) |
| **Progressive Disclosure** | Conversational view clean before execution; compact progress pill during execution; link to Mission Control | Console streaming pane rendered at all times regardless of execution status | Streaming console hidden when `status === 'idle'`; compact progress pill shown during runs; Mission Control link | `AgentIdeContinuity.test.ts` (Tests 5, 7) |
| **Conversation UX States** | 14 distinct visual status badges for end-to-end user comprehension | Only 4 generic status styles (`connecting`, `connected`, `disconnected`, `error`) | All 14 states styled (`connecting`, `connected`, `degraded`, `offline`, `session_restored`, `task_pending`, `awaiting_approval`, `interrupted`, `completed`, `verified_artifact`, `unverified_artifact`, etc.) | `AgentIdeContinuity.test.ts` (Test 6) |
| **Agent Mode ↔ IDE Mode Continuity** | Navigation between conversation and editor workbench preserves session ID, tasks, and daemon connection | No registered commands for mode switching; manual view hunting | Registered `agent-studios.openInIde`, `agent-studios.switchToAgentMode`, and `agent-studios.continueSession` | `AgentIdeContinuity.test.ts` (Tests 8-10), `RealAgentIdeContinuityE2E.test.ts` (Test 3) |
| **Zero Daemon Restart Invariant** | Switching views or continuing session never restarts `agsd` process | Unverified across view switches | Verified: Daemon PID remains identical before and after view transitions | `RealAgentIdeContinuityE2E.test.ts` (Test 3) |
| **Zero Task Duplication Invariant** | Reopening webview or switching views never duplicates tasks or resubmits approvals | Unverified; potential race conditions | Verified: Task count and approval count strictly invariant across mode switches | `RealAgentIdeContinuityE2E.test.ts` (Test 3) |
| **Webview Security Guard** | Mode navigation messages guarded against prototype pollution and malformed payloads | Limited allowlist without mode transition commands | Added `openInIde`, `openMissionControl`, `continueSession` with strict schema & prototype pollution guards | `AgentIdeContinuity.test.ts` (Tests 11-14) |
| **Real Daemon Integration** | Real E2E tests executing against compiled Rust `agsd.exe` binary with Named Pipe IPC and SQLite WAL | Baseline tests verified IPC basics but did not test mode transitions or session continuity | 6 new E2E tests verifying HMAC auth, live session hydration, mode continuity, WAL reconnect, and clean admin shutdown | `RealAgentIdeContinuityE2E.test.ts` (Tests 1-6) |

---

## 2. Technical Debt & Residual Gap Analysis

### 2.1 Backend Provider Discovery Dependency (M15.1-A)
- **Current State**: The frontend presents static adapters (`claude_code`, `internal_codex`, `codex`) and clearly marks dynamic model discovery as pending M15.1.
- **Resolution**: Claude 1's backend PR will introduce the `list_providers` and `list_models` discovery endpoint over IPC. Once merged, `ProviderMatrixAdapter` will seamlessly switch from static fallback to live discovery without frontend breaking changes.

### 2.2 Standalone Code-OSS Desktop Shell (M17)
- **Current State**: Agent Mode and IDE Mode run within the VS Code Extension Host environment using standard VS Code command routing (`workbench.view.explorer`, `workbench.view.extension.agent-studios-activitybar`, `agent-studios.chat.focus`).
- **Resolution**: Standalone desktop shell integration (electron window framing, custom top-level workbench container tabs) remains scheduled for M17. Current extension host vertical slice achieves 100% session continuity today.
