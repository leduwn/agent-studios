# Runtime Security Hardening & Isolation Architecture

**Milestone:** M14-UI-02  
**Run ID:** RUN-20261030-M14-UI-02  
**Date:** 2026-10-30  
**Scope:** Fail-closed message validation, prototype pollution guards, path traversal prevention, and workspace trust gating.

---

## 1. Threat Model & Webview Security Boundary

The VS Code webview environment is isolated from extension host Node.js capabilities by default, but communicates via bidirectional `postMessage`. In malicious or misconfigured repository contexts, unchecked webview messages can lead to:
1. **Remote Code Execution / Privilege Escalation:** Arbitrary command dispatching to extension host.
2. **Prototype Pollution:** Modifying JavaScript object prototypes via crafted payloads.
3. **Local File Inclusion / Traversal:** Leaking arbitrary filesystem files via unvalidated path properties.
4. **Untrusted Workspace Compromise:** Executing modifying tools or scripts inside untrusted repositories.

Milestone M14-UI-02 deploys defense-in-depth through `ChatSecurityGuards.ts`:

```
┌────────────────────────────────────────────────────────────────────────┐
│                        Webview Message Pipeline                        │
├────────────────────────────────────────────────────────────────────────┤
│ 1. Raw Webview Message (postMessage)                                   │
│    │                                                                   │
│    ▼                                                                   │
│ 2. Null / Primitive Type Check ──► [Fail: Rejected]                    │
│    │                                                                   │
│    ▼                                                                   │
│ 3. Recursive Prototype Pollution Check (__proto__, constructor)        │
│    │                                                                   │
│    ▼                                                                   │
│ 4. Command Allowlist Check (ALLOWED_WEBVIEW_COMMANDS)                  │
│    │                                                                   │
│    ▼                                                                   │
│ 5. Path Traversal & Protocol Breakout Check (.., \0, javascript:)      │
│    │                                                                   │
│    ▼                                                                   │
│ 6. Workspace Trust Verification (vscode.workspace.isTrusted)           │
│    │                                                                   │
│    ▼                                                                   │
│ 7. Safe Dispatch to Extension Host Handlers                            │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Command Allowlist (`ALLOWED_WEBVIEW_COMMANDS`)

Incoming messages are strictly matched against a static `Set<string>`. Any command outside this set is immediately rejected with an error message back to the webview:

```typescript
export const ALLOWED_WEBVIEW_COMMANDS = new Set([
  'sendMessage',
  'cancelRun',
  'approveAction',
  'denyAction',
  'openDiff',
  'attachContext',
  'removeAttachment',
  'switchMode',
  'switchProviderModel',
  'switchSession',
  'newSession',
  'openTaskGraph',
  'saveDraft',
  'enablePreviewMode',
  'connectDaemon',
  'dismissOnboarding',
]);
```

---

## 3. Prototype Pollution Defense

Attackers attempting to inject properties into `Object.prototype` via JSON payloads are intercepted recursively up to 5 levels deep:

```typescript
public static hasPrototypePollution(obj: Record<string, unknown>, depth = 0): boolean {
  if (depth > 5) return false;
  for (const key of Object.keys(obj)) {
    if (key === '__proto__' || key === 'constructor' || key === 'prototype') {
      return true;
    }
    const val = obj[key];
    if (val && typeof val === 'object' && !Array.isArray(val)) {
      if (this.hasPrototypePollution(val as Record<string, unknown>, depth + 1)) {
        return true;
      }
    }
  }
  return false;
}
```

---

## 4. Path Traversal & Protocol Breakout Protection

All message properties carrying filesystem paths (`path`, `filePath`, `targetPath`) are inspected by `isPathTraversal()`:

1. **Null-Byte Injection:** Blocks `\0` and URL-encoded `%00` commonly used to terminate string checks before file extensions.
2. **Directory Traversal:** Normalizes slashes and verifies that no path segment equals `..`.
3. **Protocol Scheme Breakout:** Blocks dangerous URI schemes (`javascript:`, `data:`, `file:`) via regex `/^[a-zA-Z][a-zA-Z0-9+.-]*:/` while explicitly permitting the safe internal virtual scheme `agent-studios-diff://`.

---

## 5. Workspace Trust Enforcement

When an engineer opens an untrusted repository (e.g., cloned from the internet without prior verification), VS Code disables untrusted features. `ChatSecurityGuards.verifyWorkspaceTrust()` enforces this policy:

- **Blocked Mutating Commands:**
  - `sendMessage` (executing model generation/tool actions)
  - `approveAction` (applying modifications to files/terminal)
  - `denyAction`
  - `attachContext`
  - `newSession`
- **Permitted Read-Only Commands:**
  - `switchMode`
  - `switchProviderModel`
  - `openTaskGraph` (read-only visualization)
  - `saveDraft` (local state)
  - `dismissOnboarding`

When trust is absent (`vscode.workspace.isTrusted === false`), mutating commands are rejected with a clear diagnostic explanation.

---

## 6. Content Security Policy (CSP)

`ChatHtmlBuilder.ts` generates a per-session cryptographic nonce (`getNonce()`) and enforces strict CSP headers:

```html
<meta http-equiv="Content-Security-Policy" content="
  default-src 'none';
  img-src ${webview.cspSource} https: data:;
  style-src ${webview.cspSource} 'unsafe-inline';
  font-src ${webview.cspSource};
  script-src 'nonce-${nonce}';
">
```
- Blocks inline script execution without nonce.
- Blocks remote script fetching.
- Restricts stylesheet and icon fonts to extension assets.
