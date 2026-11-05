# M15 UI-06 — Report 05: Shared Session Contract & State Synchronization

## Metadata
- **Milestone**: M15 UI-06: Agent-First Experience, Shared Session Continuity & Real Daemon Integration Acceptance
- **Worktree**: `D:\Projects\agent-studios-active\m15-ui-06`
- **Subsystem**: State Management & Bridge Communication
- **Date**: 2026-11-05
- **Status**: **VERIFIED & ACCEPTED**

---

## 1. Authoritative State Tuple

To guarantee absolute consistency between Agent Mode, IDE Mode, Mission Control, and the native Rust daemon, all views coordinate around an authoritative **Session Tuple**:

$$\mathcal{S} = \langle \text{session\_id}, \text{cwd}, \text{run\_id}, \text{tasks}, \text{approvals}, \text{artifacts}, \text{sequence}, \text{epoch} \rangle$$

Where:
- `session_id`: Canonical studio session identifier persisted in native SQLite WAL database.
- `cwd`: Absolute workspace path bound during daemon startup / workspace folder discovery.
- `run_id`: Active execution run identifier (or `null` when idle).
- `tasks`: Read-only list of projected DAG task nodes with deterministic status mapping.
- `approvals`: Pending operator confirmation requests awaiting explicit decision.
- `artifacts`: Verified diff and log outputs associated with completed/running tasks.
- `sequence`: Monotonically increasing event sequence watermark for replay gap detection.
- `epoch`: Local connection epoch preventing stale async responses across reconnects or session switches.

---

## 2. Authority vs Projection Boundary

```
┌─────────────────────────────────────────────────────────────┐
│                   Native Rust agsd Daemon                    │
│   (Authoritative State in SQLite WAL: state.sqlite)         │
└──────────────────────────────┬──────────────────────────────┘
                               │ IPC (Named Pipe / UDS)
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                 DaemonConnectedBridge                       │
│   (Two-Phase HMAC Auth, Sequence Replay, Watermark Sync)    │
└──────────────────────────────┬──────────────────────────────┘
                               │ Reactive Events
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                 SessionStateProjection                      │
│   (Single Client-Side Source of Truth across all Views)      │
├──────────────────────────────┼──────────────────────────────┤
│                              │                              │
▼                              ▼                              ▼
Agent Mode View       IDE Mode Workbench          Mission Control
(ChatWebview)         (File Tree, Editors)        (DAG Webview)
```

1. **Write Authority**: Only `agsd` mutations (via authorized HMAC IPC commands) commit state changes to SQLite.
2. **Read Reactive Projection**: `SessionStateProjection` caches current session state and emits change events to all subscribed UI views.
3. **Session Switching**: Calling `switchSession(id)`:
   - Increments `_epoch` to discard in-flight async responses from the previous session.
   - Clears pending confirmation challenges in `OperatorConfirmationManager`.
   - Resets task execution transient state in `TaskExecutionStore`.
   - Re-hydrates tasks, approvals, and artifacts for the newly active session.

---

## 3. Webview Message Security & Allowlisting

To prevent malicious webview injection or prototype pollution attacks, `ChatSecurityGuards.validateWebviewMessage` guards all incoming messages:

```typescript
export const ALLOWED_WEBVIEW_COMMANDS = new Set([
  'sendMessage',
  'submitTask',
  'cancelTask',
  'cancelRun',
  'approveAction',
  'denyAction',
  'openDiff',
  'viewTaskDiff',
  'openArtifactReview',
  'attachContext',
  'removeAttachment',
  'switchMode',
  'switchTask',
  'switchProviderModel',
  'switchSession',
  'newSession',
  'openTaskGraph',
  'saveDraft',
  'enablePreviewMode',
  'connectDaemon',
  'dismissOnboarding',
  'openInIde',
  'openMissionControl',
  'continueSession',
] as const);
```

### Security Guard Enforcements
1. **Prototype Pollution Guard**: Scans incoming object keys for `__proto__`, `constructor`, and `prototype`. Rejects immediately if detected.
2. **Command Allowlist**: Messages with unrecognized `command` values are rejected.
3. **Schema Validation**:
   - `continueSession` enforces string `sessionId` with a maximum length of 128 characters.
   - Path fields are checked against directory traversal (`..`, null bytes).
4. **Workspace Trust**: AI actions and session switches fail closed in untrusted workspaces (`vscode.workspace.isTrusted === false`).
