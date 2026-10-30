# Read-Only ConnectedClient & Fail-Closed Guard Verification: M14-IPC-03

**Milestone:** M14-IPC-03  
**Run ID:** RUN-20261030-M14-IPC-03  
**Date:** 2026-10-30  

---

## 1. Principle of Fail-Closed Read-Only Vertical Slice

Milestone M14-IPC-03 focuses strictly on establishing an authenticated, verified bi-directional connection between the VS Code Extension Host and the native daemon (`agsd`).

To prevent unauthorized mutations, premature agent runs, or unexpected filesystem writes before the comprehensive execution sandbox is fully verified in subsequent milestones, all mutating capabilities are intentionally **disabled and fail-closed**.

---

## 2. Capability Matrix

| Operation | Category | Status in M14-IPC-03 | Behavior |
| :--- | :--- | :--- | :--- |
| `getHealth()` | Read Query | **Active** | Returns daemon uptime, active clients, status |
| `ping()` | Read Query | **Active** | Measures socket round-trip latency |
| `getCurrentSession()` | Read Query | **Active** | Returns active studio metadata and progress metrics |
| `listTasks()` | Read Query | **Active** | Lists task records mapped to extension host types |
| `getTaskGraph()` | Read Query | **Active** | Builds topological graph nodes and dependency edges |
| `listProviderSummaries()`| Read Query | **Active** | Returns sanitized provider metadata and models |
| `onEvent()` | Stream | **Active** | Delivers real-time events with watermarking |
| `requestApprovalDecision`| Mutation | **Fail-Closed** | Returns `{ applied: false, message: ... }` |
| `submitEditorAction()` | Mutation | **Fail-Closed** | Returns `{ accepted: false, message: ... }` |
| `sendMessage()` | Mutation | **Fail-Closed** | Rejects with explicit disabled error |
| `createSession()` | Mutation | **Fail-Closed** | Rejects with explicit disabled error |
| `cancelCurrentRun()` | Mutation | **Safe No-Op** | Logs warning and no-ops safely |

---

## 3. Implementation Verification

### 3.1 Approval Decision Guard
```typescript
public async requestApprovalDecision(
  id: string,
  decision: 'approved' | 'denied'
): Promise<ApprovalDecisionResult> {
  return {
    id,
    decision,
    applied: false,
    message: 'Approval mutation is disabled in M14-IPC-03 read-only connected vertical slice.',
  };
}
```
**Test Verification:** `assert.strictEqual(approvalRes.applied, false)` passes across all unit test runs.

### 3.2 Editor Action Guard
```typescript
public async submitEditorAction(action: EditorActionPayload): Promise<EditorActionResult> {
  return {
    actionId: `act-${Date.now()}`,
    accepted: false,
    message: 'Editor actions are disabled in M14-IPC-03 read-only connected vertical slice.',
  };
}
```
**Test Verification:** `assert.strictEqual(editorRes.accepted, false)` passes across all unit test runs.

### 3.3 Prompt & Message Submission Guard
```typescript
public async sendMessage(
  request: AgentMessageRequest,
  _onChunk?: (chunk: string) => void
): Promise<void> {
  throw new Error('Agent message submission is disabled in M14-IPC-03 read-only connected vertical slice.');
}
```
**Test Verification:** `await assert.rejects(bridge.sendMessage(...))` verifies fail-closed rejection.

### 3.4 Session Creation Guard
```typescript
public async createSession(_goal: string): Promise<SessionOverview> {
  throw new Error('Session creation is disabled in M14-IPC-03 read-only connected vertical slice.');
}
```
**Test Verification:** `await assert.rejects(bridge.createSession(...))` verifies fail-closed rejection.
