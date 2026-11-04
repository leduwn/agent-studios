# M15 UI-05A — Report 01: Baseline & Subagent File Ownership Matrix

## Metadata
- **Milestone**: M15 UI-05A: Frontend Reliability, Shared State Consistency & Integration Readiness
- **Repository**: `E:\project\agent-studios`
- **Worktree**: `D:\Projects\agent-studios-ide-ui\apps\agent-studios-vscode`
- **Frontend PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`) targeting `feat/m14-mission-control-connected-integration`
- **Baseline Frontend HEAD**: `854eab9a5ccc5f18c16ba94ac04e01fc502a8447`
- **Authoritative Pinned Backend SHA**: `dcbcb39bf61225a4660be5b533cf1ffd5de14b29` (Backend PR #30, M15-04.1)
- **Concurrent Work**: Claude 1 developing M15-05 Backend at PR #30
- **Date**: 2026-11-02
- **Lead Integration**: Claude 2 (Frontend / VSCode Extension Integration Lead)

---

## 1. Executive Summary & Quality Baseline

Milestone M15 UI-05A builds upon the green CI foundation of M15 UI-04.1 (Ubuntu job `125105871471`, Windows job `125105871602` passing 100% on run `41392216006`).

The focus of M15 UI-05A is comprehensive hardening of the VS Code extension across:
1. Shared state consistency between Chat and Mission Control.
2. IPC reconnection, subscription error recovery, and race-free domain hydration.
3. Strict task and session isolation across rapid switching.
4. Truthful operator confirmation and runtime readiness UX.
5. High-security binary artifact verification, path traversal prevention, and byte-level export safety.
6. Integration harness preparation for Claude 1's backend M15-05 durable replay.

### Strict Safety & Security Constraints
- **Read-Only Workbench**: Production `task.submit` remains statically gated closed.
- **Zero Billable AI Model Calls**: Commercial frontier model inference (Anthropic/OpenAI) is disabled in automated testing.
- **Untouched Worktrees**: Claude 1's M15-04/M15-05 worktree, backend Rust source code (`agent-studios-rs/`), and `codex-rs/` are strictly read-only and unmutated.
- **No Force-Push or Merge**: PR #27 remains in Draft status; branch changes are cleanly pushed without force-pushing.

---

## 2. Baseline Test & Quality Metrics

| Quality Dimension | Reference Baseline (`854eab9a5c`) | Target Quality Gate | Status |
|---|---|---|---|
| **TypeScript Compilation** | 0 errors | 0 errors | **PASSED** |
| **Typecheck** (`npm run typecheck`) | 0 errors | 0 errors | **PASSED** |
| **ESLint Analysis** (`npm run lint`) | 0 errors, 324 warnings | 0 errors, <= 324 warnings | **PASSED** |
| **Unit Test Suite** (`npm run test:unit`) | 610 passing | 100% passing | **PASSED** |
| **Real Daemon E2E Suite** (`npm run test:e2e`) | 78 passing | 100% passing | **PASSED** |
| **Combined Test Total** | 688 passing tests | 100% passing | **PASSED** |
| **Remote CI (Ubuntu & Windows)** | Run `41392216006` SUCCESS | SUCCESS on final HEAD | **ENFORCED** |

---

## 3. Subagent Orchestration & File Ownership DAG

To ensure maximum focus, speed, and zero file collision, work is partitioned across four specialized subagents orchestrated by the Lead Agent under a strict Directed Acyclic Graph (DAG):

```
       ┌───────────────────────────┐
       │ Lead Agent: Baseline & DAG│
       └─────────────┬─────────────┘
                     │
        ┌────────────┴────────────┐
        ▼                         ▼
┌──────────────┐          ┌──────────────┐
│   Agent A    │          │   Agent B    │
│ Shared State │          │IPC & Recov.  │
└───────┬──────┘          └───────┬──────┘
        │                         │
        └────────────┬────────────┘
                     ▼
        ┌─────────────────────────┐
        │  Phase 2 Subagents:     │
        │ Agent C: UX & Safety    │
        │ Agent D: Artifact Sec.  │
        └────────────┬────────────┘
                     │
                     ▼
       ┌───────────────────────────┐
       │ Lead: Integration & Gate  │
       └───────────────────────────┘
```

### Detailed Ownership Matrix

| Subagent | Role & Focus | Exclusive Owned Files | Regression Test Files |
|---|---|---|---|
| **Agent A** | Shared State & Projection Auditor | `src/state/SessionStateProjection.ts`<br>`src/state/TaskExecutionStore.ts`<br>`src/mission-control/state/MissionControlProjection.ts` | `src/test/unit/ChatMissionControlConsistency.test.ts` |
| **Agent B** | IPC & Lifecycle Reliability Engineer | `src/bridge/ipc/NativeIpcTransport.ts`<br>`src/bridge/DaemonConnectedBridge.ts` | `src/test/unit/IpcReconnectLifecycle.test.ts`<br>`src/test/e2e/RealRustDaemonRecoveryE2E.test.ts` |
| **Agent C** | UX & Operator Safety Engineer | `src/bridge/CapabilityMatrix.ts`<br>`src/bridge/OperatorConfirmationManager.ts`<br>`src/bridge/ExecutionClient.ts`<br>`src/security/OperatorConfirmationModal.ts`<br>`src/chat/ChatHtmlBuilder.ts` | `src/test/unit/OperatorConfirmationSafety.test.ts` |
| **Agent D** | Artifact Review & Security Engineer | `src/bridge/ipc/ArtifactClient.ts`<br>`src/views/workbench/DiffParser.ts`<br>`src/views/workbench/WorkbenchViewProvider.ts` | `src/test/unit/ArtifactSecurityRegression.test.ts` |
| **Lead Agent** | Integration Lead & Quality Gate | Architecture, DAG Orchestration, Reports 01–10, CI, Git Commits & Push | `package.json`, test runner, CI workflow |
