# Security & Integrity Audit: M14 Quality Gate

**Milestone:** M14 Integration Quality Gate  
**Run ID:** RUN-20261030-M14-INTEGRATION  
**Date:** 2026-10-30  
**Security Classification:** CRITICAL SECURITY QUALITY GATE  

---

## 1. Security Architecture & Threat Model

The Milestone M14 integration encompasses cross-process communication between a privileged editor extension host and a local native background daemon. The following security controls were verified during this audit:

```
+--------------------------------------------------------------------------------+
|                             SECURITY BOUNDARY MODEL                            |
|                                                                                |
|  [ Webview Sandbox ]                                                           |
|         |  (postMessage only, no direct node/fs/network access)                |
|         v                                                                      |
|  [ Extension Host (TypeScript) ]                                               |
|         |                                                                      |
|         |-- Symlink Guard: lstat check rejects symlinks before reading agsd.json|
|         |-- Credential Redaction: admin_key suppressed, client_key masked      |
|         |-- Read-Only Slice: mutations fail closed                             |
|         |                                                                      |
|         v (OS IPC: Windows Named Pipe / Unix Socket)                           |
|  [ Native Daemon (Rust: agsd.exe) ]                                            |
|         |                                                                      |
|         |-- HMAC-SHA256 Auth: Constant-time signature verification             |
|         |-- Nonce Tracker: Replay protection with sliding window               |
|         |-- Role Enforcement: Standard client vs Admin client                  |
|         |-- Frame Bound Guard: 4 MiB frame limit + 4 MiB + 4 accumulator cap   |
|         v                                                                      |
|  [ Local Filesystem / Git Worktrees / Control Plane ]                          |
+--------------------------------------------------------------------------------+
```

---

## 2. Invariant Verification Checklist

### Invariant 1: Symlink Hijack Prevention (Passed)
- **Mechanism:** `DaemonDiscovery.loadBootstrapInfo` issues `fs.promises.lstat` before reading the bootstrap discovery path (`agsd.json`).
- **Defensive Behavior:** If `stats.isSymbolicLink()` evaluates to true, the loader throws a security exception (`Security Violation: Bootstrap file at ... is a symlink. Potential hijacking attack rejected.`).
- **Audit Finding:** Confirmed active and tested against malicious symlink fixture attempts.

### Invariant 2: Standard Client Credential Suppression (Passed)
- **Mechanism:** `DaemonDiscovery.loadBootstrapInfo` unconditionally sets `admin_key: null`, suppressing administrative secrets from propagating into client runtime memory.
- **Diagnostics Redaction:** `DaemonDiscovery.redact(info)` replaces `client_key` with `'[REDACTED]'` and omits `admin_key`, ensuring zero credential exposure in logging or error handlers.
- **Audit Finding:** Verified that no raw cryptographic secrets appear in any webview DOM fragments, logs, or state inspection payloads.

### Invariant 3: Provider API Key Sanitization (Passed)
- **Mechanism:** In `DaemonConnectedBridge.listProviderSummaries()`, the raw `authentication` payload received from `list_providers` is scrubbed of all `secret_key` and token properties before projecting `ProviderUsageSummary` to Mission Control.
- **Audit Finding:** Automated unit tests confirm that even if a provider config contains `sk-ant-SUPER-SECRET-NEVER-LEAK`, `JSON.stringify(providers).includes('SUPER-SECRET')` evaluates to `false`.

### Invariant 4: Fail-Closed Mutating Operation Barrier (Passed)
- **Mechanism:** In this vertical slice, the connected client is strictly read-only.
- **Mutating Operations Guarded:**
  - `requestApprovalDecision(id, decision)` -> returns `{ applied: false, message: '...disabled in M14-IPC-03...' }`.
  - `submitEditorAction(...)` -> returns `{ accepted: false, message: '...disabled in M14-IPC-03...' }`.
  - `sendMessage(...)` -> throws explicit fail-closed error.
  - `createSession(...)` -> throws explicit fail-closed error.
  - `approveAction` / `denyAction` webview message handlers in `MissionControlPanel.ts` -> display a defensive warning toast without dispatching mutations.
- **Audit Finding:** Confirmed that zero unauthorized file edits, tool executions, or workspace modifications can occur from the Mission Control UI.

### Invariant 5: Role-Based Authorization Enforcement (Passed)
- **Mechanism:** The Rust daemon enforces `session.role == ClientRole::Admin` for privileged operations such as `IpcCommand::Shutdown`.
- **Audit Finding:** A standard client attempting to invoke `shutdown` is refused with `Permission denied: shutdown requires admin role`. Only an authenticated admin client can issue shutdown commands.

### Invariant 6: Strict Content Security Policy in Webview (Passed)
- **Mechanism:** `MissionControlHtmlBuilder.buildHtml` generates a cryptographically random 16-byte nonce (`crypto.randomBytes(16).toString('base64')`) per panel initialization:
  `default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}'; font-src ${webview.cspSource}; img-src ${webview.cspSource} data: https:;`
- **Incremental Hydration:** Subsequent updates patch the DOM via `postMessage`, preserving the initial CSP nonce and eliminating unsafe script injection vectors.
- **Audit Finding:** CSP violations: 0. XSS attack surface: mitigated.

---

## 3. Residual Risk & Future Milestone Scope

- **M15 Scope (Mutating Approvals & Tool Execution):** When approval execution is promoted from read-only to live execution in M15, each approval must re-verify cryptographic origin binding (`approval_origins` in daemon state) before dispatching to external runtimes.
