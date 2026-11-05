# Agent Studios — M15 UI-06 Release Hardening
## Document 04: Security & Compatibility Review

**Milestone:** M15 UI-06 (Agent-First Experience, Shared Session Continuity & Real Daemon Integration)  
**Branch:** `feat/m15-ui-06-agent-ide-continuity`  
**Target Base:** `integration/m15-unified-codebase`  
**Status:** AUDITED & SECURE  
**Date:** 2026-11-06  

---

### 1. Security Architecture & Threat Surface

Agent Studios bridges VS Code Extension Webviews, the Node.js Extension Host, and the compiled Rust native daemon (`agsd`) via local IPC:

```
[Webview Sandbox] ──(postMessage RPC)──► [Extension Host] ──(NamedPipe / UDS)──► [agsd Native Daemon]
   (CSP, No inline JS,                    (Validation, Path Sanitization,          (HMAC Challenge,
    Sanitized State)                        Token / ID Verification)                Role Enforcement)
```

---

### 2. Detailed Security Audit Findings

#### 1. Inbound Webview Message Validation & Prototype Pollution
- **Mechanism:** `ChatWebviewViewProvider.ts` and `WebviewSecurityGuards.ts`.
- **Review:**
  - Inbound messages from webviews are validated against strict JSON schemas before being passed to bridge managers or commands.
  - JSON deserialization paths protect against prototype pollution (`__proto__`, `constructor.prototype`) by sanitizing object keys and rejecting non-plain objects.
  - Webview HTML uses strict Content Security Policy (CSP):
    - `default-src 'none';`
    - `img-src ${webview.cspSource} https: data:;`
    - `script-src 'nonce-${nonce}';`
    - `style-src ${webview.cspSource} 'unsafe-inline';`

#### 2. Session ID Integrity & Authorization Check
- **Mechanism:** `BridgeManager.switchSession()` and `ModeContinuityCommands`.
- **Review:**
  - Session IDs are strictly typed UUID strings.
  - Passing malformed, arbitrary, or directory-traversal strings as session IDs fails validation before reaching the bridge.
  - Switching sessions triggers a complete state reset and rehydration from the daemon to prevent stale cross-session state leaks.

#### 3. Path Traversal & UNC Injection Defenses
- **Mechanism:** `WorkspacePathSanitizer.ts` and `AgsdBinaryResolver.ts`.
- **Review:**
  - Rejects directory traversal tokens (`..`, `../`, `..\`).
  - Blocks Windows UNC network paths (`\\server\share`) to prevent NTLM credential harvesting and remote binary loading.
  - Workspace validation asserts that opened paths reside strictly within authorized workspace roots.

#### 4. Pending Approvals & Fail-Closed Lifecycle
- **Mechanism:** `SessionStateProjection.ts` and `ChatHtmlBuilder.ts`.
- **Review:**
  - Pending approval actions require explicit user interaction with token-bound nonce verification.
  - When the daemon disconnects or enters degraded state, the bridge immediately fails closed:
    - Editor refactoring actions are blocked.
    - Test generation commands are disabled.
    - No automatic task re-submission occurs on reconnect.

#### 5. Daemon Privilege Separation (Operator vs Admin)
- **Mechanism:** `RealAgentIdeContinuityE2E.test.ts` and `RustIpcProtocolTypes.ts`.
- **Review:**
  - The native daemon exposes dual bootstrap endpoints:
    - Standard operator endpoint (`agsd.json`) with standard client key.
    - Admin endpoint (`agsd-admin.json`) with separate admin key.
  - Proved in real daemon E2E test #5:
    - Standard operator client attempting `shutdown` command receives `Permission Denied` / `Unauthorized` and daemon process remains alive.
    - Only authenticated admin connection can issue `shutdown`.

#### 6. Provider Fallback Labels & Discovery Compatibility
- **Mechanism:** `ProviderRegistry.ts` and `ProvidersTreeView.ts`.
- **Review:**
  - Proposed Provider Discovery IPC methods (`discover_providers`, `list_provider_models`) have NOT been integrated prematurely, strictly honoring the constraint to await Claude 1's backend contracts.
  - Fallback provider instance labels (`Anthropic Claude 3.5 Sonnet`, `Codex Custom Engine`) display clear provenance tags (`[Preview Fixture]` vs `[Daemon Connected]`).
  - Webview surfaces display a discovery pending notice explaining that dynamic provider negotiation will activate once daemon capability contracts land.
