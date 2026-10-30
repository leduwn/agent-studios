# Runtime Security Hardening & Isolation Architecture

**Milestone:** M14-UI-02.1  
**Run ID:** RUN-20261030-M14-UI-02  
**Date:** 2026-10-30  
**Scope:** Fail-closed message validation, recursive prototype pollution defense, path traversal prevention, command schemas, and workspace trust gating.

---

## 1. Threat Model & Webview Security Boundary

The VS Code webview environment is isolated from extension host Node.js capabilities by default, but communicates via bidirectional `postMessage`. In malicious or misconfigured repository contexts, unchecked webview messages can lead to:
1. **Remote Code Execution / Privilege Escalation:** Arbitrary command dispatching to extension host.
2. **Prototype Pollution:** Modifying JavaScript object prototypes via crafted payloads.
3. **Local File Inclusion / Traversal:** Leaking or overwriting arbitrary filesystem files via unvalidated path properties.
4. **Untrusted Workspace Compromise:** Executing modifying tools or scripts inside untrusted repositories.

Milestone M14-UI-02.1 deploys defense-in-depth through `ChatSecurityGuards.ts`:

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
│ 3. Fail-Closed Prototype Pollution Check (Depth <= 5, Arrays, Keys)    │
│    │                                                                   │
│    ▼                                                                   │
│ 4. Command Allowlist Check (ALLOWED_WEBVIEW_COMMANDS)                  │
│    │                                                                   │
│    ▼                                                                   │
│ 5. Command-Specific Schemas & Max Length Bounds                        │
│    │                                                                   │
│    ▼                                                                   │
│ 6. Path Traversal & Protocol Breakout Check (.., \0, UNC, Schemes)     │
│    │                                                                   │
│    ▼                                                                   │
│ 7. Canonical Workspace Path Containment & Symlink Validation           │
│    │                                                                   │
│    ▼                                                                   │
│ 8. Workspace Trust Verification (vscode.workspace.isTrusted)           │
│    │                                                                   │
│    ▼                                                                   │
│ 9. Safe Dispatch to Extension Host Handlers                            │
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
] as const);
```

---

## 3. Remediated Prototype Pollution Defense (Fail-Closed)

### Security Defect in Prior Implementation
The previous implementation suffered from two critical security regressions:
1. **Fail-Open on Depth Exceeded:** It returned `false` when `depth > 5`, allowing deeply nested pollution payloads to bypass inspection completely.
2. **Array Traversal Blindspot:** It skipped `Array.isArray(val)`, allowing attackers to smuggle prototype pollution payloads inside nested arrays or collections (such as `attachments` or `metadata`).

### Remediated Implementation
In `ChatSecurityGuards.ts`, traversal strictly **fails closed** whenever recursion depth exceeds `MAX_OBJECT_DEPTH` (5), and recursively inspects array items, object keys, and multidimensional collections:

```typescript
public static checkPrototypePollution(
  val: unknown,
  depth = 0
): { hasPollution: boolean; reason?: string } {
  // Fail-closed when depth exceeds limit
  if (depth > SECURITY_LIMITS.MAX_OBJECT_DEPTH) {
    return {
      hasPollution: true,
      reason: `Maximum object nesting depth (${SECURITY_LIMITS.MAX_OBJECT_DEPTH}) exceeded: fail-closed.`,
    };
  }

  if (!val || typeof val !== 'object') {
    return { hasPollution: false };
  }

  // Inspect array elements recursively
  if (Array.isArray(val)) {
    for (let i = 0; i < val.length; i++) {
      const itemResult = this.checkPrototypePollution(val[i], depth + 1);
      if (itemResult.hasPollution) {
        return itemResult;
      }
    }
    return { hasPollution: false };
  }

  const obj = val as Record<string, unknown>;
  const keys = Object.getOwnPropertyNames(obj);

  for (const key of keys) {
    if (key === '__proto__' || key === 'constructor' || key === 'prototype') {
      return {
        hasPollution: true,
        reason: `Prohibited prototype pollution key detected: '${key}'.`,
      };
    }

    const propVal = obj[key];
    if (propVal && typeof propVal === 'object') {
      const nestedResult = this.checkPrototypePollution(propVal, depth + 1);
      if (nestedResult.hasPollution) {
        return nestedResult;
      }
    }
  }

  return { hasPollution: false };
}
```

---

## 4. Command Schemas & Sizing Bounds

To prevent memory exhaustion attacks and malformed input, each command has explicit schema requirements and bounded length limits:

```typescript
export const SECURITY_LIMITS = {
  MAX_MESSAGE_BYTES: 512 * 1024,      // 512 KiB max message payload
  MAX_TEXT_LENGTH: 128 * 1024,        // 128 KiB max prompt / draft
  MAX_PATH_LENGTH: 2048,              // 2048 chars max path
  MAX_ID_LENGTH: 128,                 // 128 chars max ID
  MAX_ATTACHMENTS: 10,                // 10 attachments max
  MAX_OBJECT_DEPTH: 5,                // Max recursion depth
} as const;
```

Key Schema Constraints:
- `sendMessage`: Requires `text` string `<= 128 KiB`, `mode` in `{'agent', 'plan', 'ask'}`, attachments count `<= 10`.
- `saveDraft`: Requires `draft` string `<= 128 KiB`.
- `approveAction` / `denyAction`: Requires `approvalId` matching `^[a-zA-Z0-9_\-.:]+$` `<= 128 chars`, rejecting SQL/shell injection attempts.
- `openDiff`: Validates `artifactId` and verifies `filePath` against canonical workspace boundaries.
- `attachContext`: Requires `source` in `{'filePicker', 'activeEditor'}`.
- `removeAttachment`: Validates `path` string `<= 2048 chars` and ensures no directory traversal.

---

## 5. Path Traversal & Protocol Breakout Protection

All message properties carrying filesystem paths (`path`, `filePath`, `targetPath`) are inspected by `isPathTraversal()`:

1. **Null-Byte Injection:** Blocks `\0` and URL-encoded `%00`.
2. **Directory Traversal:** Normalizes slashes and verifies that no path segment equals `..`.
3. **UNC Path Traversal:** Blocks Windows UNC paths (e.g. `\\attacker\share` and `//attacker/share`).
4. **URL-Encoded Traversal:** Decodes URI components to block `%2e%2e%2f` sequences.
5. **Protocol Scheme Breakout:** Blocks dangerous URI schemes (`javascript:`, `data:`, `file:`) while explicitly permitting:
   - Single-letter Windows drive letters (`C:`, `D:`).
   - Safe internal virtual scheme `agent-studios-diff://`.
6. **Canonical Workspace Containment:** Resolves absolute target paths, canonicalizes Windows drive case, verifies target starts with workspace folder root + path separator, and inspects real paths to prevent symlink breakout.

---

## 6. Workspace Trust Enforcement

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
  - `openTaskGraph`
  - `saveDraft`
  - `dismissOnboarding`
