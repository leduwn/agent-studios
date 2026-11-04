# M15 UI-04 — Integration Plan: Backend Durable Recovery & UI Resynchronization

## 1. Executive Summary & Goals

- **Milestone**: M15 UI-04 (Backend Dependency Update & Crash Recovery Integration)
- **Target Frontend Branch**: `feat/m15-real-agent-execution-ui` (Draft PR #27)
- **Base Branch**: `feat/m14-mission-control-connected-integration`
- **Pinned Backend Dependency**: SHA `dcbcb39bf61225a4660be5b533cf1ffd5de14b29` (Branch `feat/m15-04-durable-state-recovery`, PR #30)
- **Primary Objective**: Integrate backend M15-04.1 durable recovery, SQLite single-writer locking, role-based diagnostics redaction, and crash recovery event streaming into the VS Code extension host UI without modifying backend code or merging PR #30 into PR #27.

---

## 2. Nine Core Principles & Constraints

1. **Branch & PR Isolation**: Maintain `feat/m15-real-agent-execution-ui` on PR #27 (Draft). Do not touch `agent-studios-rs/` source code from the UI worktree.
2. **Pinned Backend Binary Dependency**: Run integration testing against the compiled `agsd` binary built strictly from SHA `dcbcb39bf61225a4660be5b533cf1ffd5de14b29` (`D:\Projects\agent-studios-m15-04\agent-studios-rs\target\debug\agsd.exe`). No automatic git merge of PR #30 into PR #27.
3. **Versioned IPC Contract Compliance**: Extend TypeScript protocol types (`RustIpcProtocolTypes.ts`) with `GetRecoveryStatus`, `RecoveryStatusReport`, role-based field optionality, and verified task failure states.
4. **Authentic Subprocess Frontend Tests**: Implement automated tests executing the authentic compiled `agsd` subprocess covering clean restart, SIGKILL/forced process termination, task crash reconciliation, artifact retrieval, and monotonic watermark resynchronization.
5. **UI Invariants & Non-Resumption Safety**: Verify UI never automatically re-triggers or resumes an interrupted task upon reconnect, never renders an interrupted task as `RUNNING` (must display `FAILED` with `daemon_interrupted`), and never exposes host database paths to standard-role clients.
6. **Strict Test Classification**: Explicitly distinguish `REAL_DAEMON_IPC_TESTS` (OS IPC to authentic compiled binary, no billable LLM calls) from `REAL_AI_AGENT_EXECUTION_TESTS` (live AI model runtime invocations).
7. **Quality Gates & CI Parity**: Execute full linting (`npm run lint`), TypeScript compilation (`npm run compile`), unit tests (`npm run test:unit`), and E2E integration tests on both Windows and Ubuntu runners.
8. **Handoff Protocol for Backend Defects**: If an IPC protocol discrepancy or daemon defect is identified, generate a formal handoff report in `docs/reviews/M15-UI-04/` for Claude 1 rather than editing Rust files.
9. **Release Gate Verification**: Commit and push frontend changes to PR #27, verify remote CI passes on both OS matrix targets, and generate sign-off documentation.

---

## 3. Directed Acyclic Graph (DAG) Execution Workflow

```
[Phase 1: Contract & Types]
        |
        v
[Phase 2: Client & Bridge Implementation] <----+
        |                                       | (Non-overlapping
        v                                       |  subagent ownership)
[Phase 3: Real Daemon Crash Tests] <------------+
        |
        v
[Phase 4: UI Safety & View Invariants]
        |
        v
[Phase 5: Full Quality Gate & CI Verification]
        |
        v
[Phase 6: Audit Documentation & PR #27 Push]
```

### DAG Tasks & Ownership

| Task ID | Task Description | Primary Files Owned | Output / Gate |
| :--- | :--- | :--- | :--- |
| **DAG-1** | IPC Protocol Types & Contract Update | `apps/agent-studios-vscode/src/bridge/ipc/RustIpcProtocolTypes.ts` | `RustRecoveryStatusReport`, `get_recovery_status` command added |
| **DAG-2** | Bridge & Client Diagnostics API | `src/bridge/DaemonConnectedBridge.ts`<br>`src/bridge/FixtureBridge.ts`<br>`src/bridge/AgentStudiosBridge.ts` | `getRecoveryStatus()` exposed with role-aware return type |
| **DAG-3** | Real Daemon Recovery E2E Test Suite | `src/test/e2e/RealRustDaemonRecoveryE2E.test.ts` | Real subprocess clean restart, SIGKILL, and reconciliation tested |
| **DAG-4** | UI Safe Rendering & Non-Resumption | `src/webviews/TaskGraphWebview.ts`<br>`src/views/TasksTreeProvider.ts`<br>`src/views/SessionTreeProvider.ts` | Crash tasks render `FAILED`, no auto-resume, path hidden |
| **DAG-5** | Quality Gates & Compilation | Whole `apps/agent-studios-vscode` workspace | `npm run compile`, `npm run lint`, `npm test` 100% green |
| **DAG-6** | Audit Reports & PR Update | `docs/reviews/M15-UI-04/*` | 6 audit documents published, PR #27 pushed and verified |
