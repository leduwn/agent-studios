# M15 UI-03 — Security, Content Security Policy & Isolation Audit

## Metadata
- **Milestone**: M15 UI-03
- **Topic**: Webview Security, CSP Invariants, Read-Only Boundary & Static Mutation Disablement
- **Date**: 2026-11-02

---

## 1. Webview Security & Content Security Policy (CSP)

The `ArtifactReviewPanel` enforces strict isolation standards matching Code-OSS webview security guidelines:

```html
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}';">
```

### Defense-in-Depth Measures
1. **Script Execution Restriction**:
   - Only scripts with a cryptographically generated 32-byte hexadecimal nonce (`nonce-${nonce}`) are permitted to execute.
   - Inline `eval`, external script tags, and remote script fetches (`fetch`, `XMLHttpRequest`, `WebSocket`) are blocked (`default-src 'none'`).
2. **HTML & Text Sanitization**:
   - All dynamic strings, filenames, and diff lines rendered in the DOM are escaped via `escapeHtml()`:
     ```typescript
     function escapeHtml(text: string): string {
       return text
         .replace(/&/g, '&amp;')
         .replace(/</g, '&lt;')
         .replace(/>/g, '&gt;')
         .replace(/"/g, '&quot;')
         .replace(/'/g, '&#039;');
     }
     ```
3. **No Arbitrary File Access or Execution**:
   - Patches are rendered purely as text in the webview.
   - Filenames in diff headers cannot trigger shell execution, tool calls, or host filesystem modifications.
   - The "Export Patch" action opens a native VS Code Save Dialog (`vscode.window.showSaveDialog`), placing full path selection control in the hands of the operator.

---

## 2. Command Allowlist & Prototype Pollution Defense

All messages passing from Webviews into the Extension Host pass through `ChatSecurityGuards.validateWebviewMessage`:
- Rejects prohibited prototype pollution keys (`__proto__`, `constructor`, `prototype`).
- Enforces maximum object recursion depth of 5.
- Enforces strict command allowlist (`ALLOWED_WEBVIEW_COMMANDS`).
- New commands `viewTaskDiff` and `openArtifactReview` are added to the allowlist and schema-validated for maximum identifier bounds (max 128 characters).

---

## 3. Static Production Mutation Gate Invariant

Production mutating task execution remains **strictly disabled**:
- `task.submit` is statically configured with `enabled: false` in `CapabilityMatrix`.
- Missing capability metadata in IPC responses continues to fail closed to read-only compatibility.
- Any attempt to trigger task submission or mutation from webview without authorized credentials fails closed.
