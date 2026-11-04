# M15 UI-02.2 — Operator Confirmation Lifecycle & Security Audit

## Metadata
- **Milestone**: M15 UI-02.2
- **Topic**: Operator Confirmation Modal, Cryptographic Challenge Binding, Session Invalidation
- **Target Branch**: `feat/m15-real-agent-execution-ui` (Draft PR #27)
- **Base Branch**: `feat/m14-mission-control-connected-integration`
- **Pinned Backend Commit**: `61cb7e88e4ae0e3a0432134b77eee0c30f56c302` (PR #28 / PR #26)
- **Date**: 2026-11-02

---

## 1. Executive Summary

Milestone M15 UI-02.2 audits and hardens the Operator Confirmation lifecycle. Every mutating coding agent task requires explicit, interactive operator confirmation within the trusted Extension Host process boundary before any command frame is serialized or dispatched over IPC.

Security enhancements in M15 UI-02.2:
1. **Daemon Session ID Cryptographic Binding**: Challenge HMAC-SHA256 tokens now cryptographically bind the authenticated daemon connection `sessionId`.
2. **Reconnection & Reset Invalidation**: Disconnection, transport reset, or bridge reconnection clears all pending challenges (`OperatorConfirmationManager.clearPendingChallenges()`). Tokens generated for a previous session cannot be used in a subsequent session.
3. **Single-Use Atomic Consumption**: Challenges are consumed atomically during preflight evaluation; replay attempts fail immediately.
4. **Test Injection Isolation**: Test dialog handlers in `OperatorConfirmationModal.setDialogHandlerForTesting()` are blocked in production mode and outside active automated test runners.

---

## 2. Cryptographic Challenge Token Architecture

### 2.1 Bound Parameters

The `OperatorConfirmationChallenge` payload binds all mutable execution parameters:
- `studioId`: Active studio UUID.
- `promptHash`: SHA-256 hash of the exact prompt text.
- `workspacePath`: Target workspace path (canonicalized).
- `modelId`: Selected model identifier.
- `providerId`: Selected provider identifier.
- `runtimePreference`: Selected runtime preference (e.g. `'opencode'`).
- `runtimeInstanceId`: Specific discovered runtime instance ID (e.g. `'rt-inst-alpha-01'`).
- `sessionId`: Authenticated daemon connection session ID.
- `intent`: Fixed string `'task_execution'`.
- `issuedAt`: Monotonic epoch timestamp (validity window: 300 seconds).

### 2.2 Signature Calculation & Verification

The HMAC-SHA256 signature is calculated within the Extension Host using a cryptographically secure random key:
```typescript
const payload = [
  studioId,
  promptHash,
  options?.workspacePath ?? '',
  options?.modelId ?? '',
  options?.providerId ?? '',
  options?.runtimePreference ?? '',
  options?.runtimeInstanceId ?? '',
  options?.sessionId ?? '',
  intent,
  issuedAt.toString(),
].join('|');
```

When verified:
- Signature is verified with timing-safe comparison.
- Expiration is checked against a 300,000ms TTL.
- Every bound field (including `sessionId`) is compared against the dispatch request. Any mismatch fails closed.
- Upon successful verification, the challenge token is deleted from the active challenge set, preventing replay.

---

## 3. Trusted Host Boundary Enforcement

1. **Native VS Code Modal Dialog**:
   - `OperatorConfirmationModal` calls `vscode.window.showWarningMessage('...', { modal: true, detail }, 'Confirm & Execute', 'Cancel')`.
   - Untrusted Webview UI (such as the Chat iframe) has no ability to dismiss or fabricate this confirmation.
2. **Cancellation Handling**:
   - If the operator selects 'Cancel' or closes the dialog, `OperatorConfirmationManager.clearPendingChallenges()` purges any unconsumed challenge tokens.
3. **Transport Session Invalidation**:
   - On IPC transport disconnect or reconnect in `DaemonConnectedBridge`, `OperatorConfirmationManager.clearPendingChallenges()` is invoked to prevent stale challenge replay across network or daemon restarts.
