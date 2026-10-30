# Agent Studios — Milestone M14-UI-00: Security Review

## Executive Summary

Milestone M14-UI-00 establishes the UI workbench for autonomous and semi-autonomous AI agents operating inside Code-OSS. Because AI agents have the ability to suggest command executions, patch code, and interact with language models, security is a tier-0 architectural requirement.

---

## 1. Threat Model & Mitigations

| Threat | Impact | Implemented Mitigation |
|---|---|---|
| **Malicious Markdown / Prompt Injection into Webviews** | Cross-Site Scripting (XSS), token theft, or unauthorized extension API execution. | Cryptographic nonce on all `<script>` tags, strict Content Security Policy (CSP), HTML escaping, and script tag stripping in `renderSafeMarkdown`. |
| **Unauthorized Action Execution during Disconnection** | Command execution or permission escalation when the engine is not authenticated or connected. | **Fail-Closed Security Architecture**: `DisconnectedBridge` rejects all mutations with `BridgeDisconnectedError` (`applied: false`). |
| **Accidental High-Risk Command Approval** | Destructive commands (e.g. `rm -rf`, `git reset --hard`) executed by developer muscle memory. | Modal confirmation dialog (`vscode.window.showWarningMessage` with `{ modal: true }`) mandatory for all `high` and `critical` risk actions. |
| **API Key / Secret Leakage via UI** | Exposure of provider API keys (OpenAI, Anthropic) in client-side extension UI or DOM. | **Strictly Read-Only Providers TreeView**: Keys are stored and managed exclusively in the backend Rust daemon keychain. Extension only receives display summaries with masked models. |
| **Tampered Artifacts** | Malicious patch files inserted into user workspace without verification. | Patch diff inspection runs through in-memory `agent-studios-diff` virtual document provider. No untrusted temporary files are written to the workspace filesystem. |

---

## 2. Content Security Policy (CSP) & Nonces

Every webview (`TaskGraphWebview`, `SessionDetailWebview`) enforces a strict Content Security Policy generated per session:

```html
<meta http-equiv="Content-Security-Policy" content="
  default-src 'none';
  style-src ${webview.cspSource} 'unsafe-inline';
  script-src 'nonce-${nonce}';
  img-src ${webview.cspSource} https:;
  font-src ${webview.cspSource};
">
```

### Key Controls:
- **`default-src 'none'`**: Disallows any default resource fetching (no unapproved iframes, audio, media, objects).
- **`script-src 'nonce-${nonce}'`**: Only scripts carrying the cryptographically randomized 32-character nonce (`crypto.randomBytes(16).toString('hex')`) will execute. Inline script injection is rejected by Chromium.
- **No external script CDN imports**: All visualization and interaction logic is bundled locally with exact dependencies.

---

## 3. XSS Prevention & Markdown Sanitization

All agent-provided markdown, tool outputs, and error summaries are rendered through `renderSafeMarkdown`:

1. **HTML Entity Escaping**:
   - `&` -> `&amp;`
   - `<` -> `&lt;`
   - `>` -> `&gt;`
   - `"` -> `&quot;`
   - `'` -> `&#39;`
2. **Script Neutralization**:
   - `<script>` and `javascript:` URIs are sanitized before parser processing.
3. **Safe Rendering**:
   - Code blocks, blockquotes, lists, bold, italics, and headers render as pure structural HTML without arbitrary attribute injection.

---

## 4. Fail-Closed Approval Policy

When a user attempts to approve or deny an action, the extension requires an authoritative bridge connection:

```typescript
// DisconnectedBridge.ts
public async requestApprovalDecision(
  id: string,
  decision: 'approved' | 'denied'
): Promise<ApprovalDecisionResult> {
  throw new BridgeDisconnectedError(
    `Cannot submit approval decision '${decision}' for approval '${id}': Bridge is disconnected.`
  );
}
```

- When the bridge is disconnected, `requestApprovalDecision` and `submitEditorAction` unconditionally throw `BridgeDisconnectedError`.
- No approval state changes occur in the local projection without authoritative backend confirmation.

---

## 5. Secret Protection & Model Providers

Agent Studios adheres to a strict architectural rule:
> **Model providers in the extension UI are strictly READ-ONLY.**

- The extension possesses **no UI or commands** to input, edit, or display API keys.
- Provider credentials reside exclusively in the daemon's secure OS keychain or environment variables managed by the Rust runtime.
- The `ProvidersTreeProvider` only displays provider names (`Anthropic Claude 3.5 Sonnet`), model identifiers (`claude-3-5-sonnet-20241022`), and status (`available` / `offline`).

---

## 6. Workspace Trust

The extension declares `"supportUntrustedWorkspaces": false` in `package.json` for safety-critical commands:
- Agent execution, terminal command approvals, and code patch application are disabled in untrusted workspaces.
- In untrusted workspaces, the workbench operates in a restricted preview mode without command execution capabilities.
