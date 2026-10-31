# Milestone M15 UI-01.2 Review: Operator Confirmation Cryptographic Gate Audit

**Milestone**: M15 UI-01.2 — Real Rust Execution Integration & Final Safety Gates  
**Area**: Cryptographic Binding, Host Modal Dialog & Atomic Consumption  
**Verdict**: APPROVED  

---

## 1. Executive Summary

Milestone M15 UI-01.2 hardens the task submission safety gate into an immutable, single-use, cryptographically bound operator confirmation mechanism. The extension host presents a native VS Code modal warning dialog (`vscode.window.showWarningMessage` with `{ modal: true }`) strictly before token issuance.

Approved challenge tokens bind runtime implementation, runtime instance, model ID, provider ID, studio ID, workspace path, and prompt SHA-256 hash into an HMAC-SHA256 signature with intent `'task_execution'`. The token is consumed atomically at the dispatch boundary (`SubmitTaskAdapter.submitTask`), preventing replay, tampering, or unauthorized re-dispatch.

---

## 2. Cryptographic Challenge Token Construction

### 2.1 Signed Payload Structure

```json
{
  "studioId": "studio-001",
  "promptHash": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945",
  "workspacePath": "/authorized/workspace/root",
  "modelId": "claude-3-7-sonnet",
  "providerId": "anthropic",
  "runtimePreference": "Claude Code (Native CLI)",
  "runtimeInstanceId": "inst-claude-01",
  "intent": "task_execution",
  "issuedAt": 1730347200000,
  "expiresAt": 1730347260000,
  "nonce": "d8f1e6b9c4..."
}
```

Signature: `HMAC-SHA256(canonicalJson(payload), internalExtensionSecret)`

### 2.2 Security Invariants Enforced

1. **Host-Level Modal Presentation**: The confirmation dialog is rendered exclusively by the trusted VS Code Extension Host. Untrusted webviews cannot approve their own requests.
2. **Zero Pre-Issued Tokens**: Challenge tokens do not exist prior to explicit operator interaction with the native modal dialog.
3. **Intent Pinning**: Challenge tokens are strictly signed with `intent: 'task_execution'`. Any other intent (or missing intent) is rejected with `Invalid confirmation challenge intent`.
4. **Exact Parameter Matching**: Every request parameter (`studioId`, `workspacePath`, `modelId`, `providerId`, `runtimePreference`, `runtimeInstanceId`, `promptHash`) must match the signed token exactly. Any mismatch triggers an immediate rejection.
5. **Atomic Single-Use Consumption**: Tokens are consumed upon entry into `SubmitTaskAdapter.submitTask`. Attempting to submit a second task with the same token fails with `Challenge token has already been consumed`.
6. **Zero Webview Exposure**: The HMAC signing secret is stored exclusively in module memory within the Extension Host and is never transmitted across webview boundaries.
