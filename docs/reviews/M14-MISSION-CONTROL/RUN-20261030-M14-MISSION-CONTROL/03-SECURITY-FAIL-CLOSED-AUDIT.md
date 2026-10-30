# Security & Fail-Closed Boundary Audit: M14 Mission Control

**Milestone:** M14 Mission Control Workbench  
**Run ID:** RUN-20261030-M14-MISSION-CONTROL  
**Date:** 2026-10-30  

---

## 1. Security Architecture & Threat Vectors

The Mission Control workbench operates within the VS Code Extension Host and Webview sandbox. It handles developer workspace files, provider configurations, and human-in-the-loop approval requests.

```
┌────────────────────────────────────────────────────────────────────────┐
│                        Extension Host Boundary                         │
│                                                                        │
│   Bridge / Provider Config           Approval Execution Pipeline       │
│   [apiKey: "sk-...", secret: "..."]  [approveAction / denyAction]      │
│                  │                                  │                  │
│                  ▼                                  ▼                  │
│   ┌──────────────────────────────┐   ┌──────────────────────────────┐  │
│   │   Secret Scrubbing Filter    │   │  Fail-Closed Security Guard  │  │
│   │   (apiKey, secret: undefined)│   │  (Mutations blocked in slice)│  │
│   └──────────────┬───────────────┘   └──────────────┬───────────────┘  │
│                  │                                  │                  │
│                  ▼                                  ▼                  │
│                  State Transfer to Webview (postMessage)               │
└──────────────────────────────────┬─────────────────────────────────────┘
                                   │
                                   ▼ Webview Sandboxed Context
┌────────────────────────────────────────────────────────────────────────┐
│  Mission Control Webview                                               │
│  - Strict Nonce-based CSP (default-src 'none', script-src nonce)       │
│  - Read-Only Badging & Disabled Mutation Controls                      │
│  - HTML XSS Escape Filters on all dynamic labels and traces            │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Security Controls & Audit Findings

### 2.1 Fail-Closed Mutation Boundary
- **Threat:** Unsanitized webview messages triggering unverified shell commands, tool executions, or destructive file modifications.
- **Remediation:** In `MissionControlPanel._handleMessage()`, calls to `approveAction` and `denyAction` are intercepted. A modal warning is displayed:
  ```typescript
  vscode.window.showWarningMessage(
    'Approval decisions are read-only in this vertical slice. Fail-closed security boundary is active.'
  );
  ```
- **Audited in Unit Tests:** Verified in `missionControl.test.ts` (`fails closed on approveAction or denyAction requests`).

### 2.2 Secret Scrubbing at Extension Host Boundary
- **Threat:** Exposure of sensitive LLM provider credentials, OpenAI/Anthropic API keys, or daemon IPC secrets into webview DOM or DevTools.
- **Remediation:** `MissionControlProjection.projectRealState()` maps raw provider summaries into `ProviderUsageSummary`, explicitly omitting authentication headers, API keys, and connection secrets.
- **Audited in Unit Tests:**
  ```typescript
  for (const prov of state.providers) {
    assert.strictEqual((prov as unknown as Record<string, unknown>).apiKey, undefined);
    assert.strictEqual((prov as unknown as Record<string, unknown>).secret, undefined);
  }
  ```

### 2.3 Strict Content Security Policy (CSP) & XSS Prevention
- **Threat:** Injected scripts executing arbitrary code in webview context via malicious task titles, tool outputs, or error traces.
- **Remediation:**
  1. All dynamic strings rendered in HTML templates pass through `escapeXml()` or `escapeHtml()`.
  2. Scripts are restricted exclusively to `script-src 'nonce-${nonce}'`. Inline scripts without the correct nonce are blocked by Chromium.
  3. External network requests (`connect-src`) are disabled (`default-src 'none'`).

### 2.4 Native VS Code Diff Review vs Custom Webview Diff
- **Threat:** Building an ad-hoc in-webview diff viewer introduces syntax highlighting vulnerabilities, memory leaks on large diffs, and lack of Git staging integration.
- **Remediation:** Delegated to `DiffReviewManager` and `vscode.diff` utilizing `agent-studios-diff://` virtual document scheme. Users leverage battle-tested VS Code editor diffing with full syntax tree support, side-by-side / inline switching, and accessibility screen reader hooks.
