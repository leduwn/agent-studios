# Agent Studios — Milestone M14-UI-01: Security Review

## Security Assessment Summary
The M14-UI-01 Chat-First Coding Assistant runs inside a VS Code WebviewView. Because webviews execute within an isolated Chromium renderer, security defenses must ensure zero script injection, zero credential leakage across boundaries, strict fail-closed authorization, and full compliance with VS Code Workspace Trust.

---

## 1. Strict Content Security Policy (CSP) & Cryptographic Nonce

### 1.1 Policy Definition
Every render of the chat HTML generates a cryptographically secure 16-byte random nonce using Node.js `crypto`:

```typescript
const nonce = crypto.randomBytes(16).toString('base64');
```

The Webview HTML includes the following HTTP `<meta>` Content-Security-Policy header:

```html
<meta http-equiv="Content-Security-Policy" content="
  default-src 'none';
  style-src ${webview.cspSource} 'unsafe-inline';
  script-src 'nonce-${nonce}';
  font-src ${webview.cspSource};
">
```

### 1.2 Attack Surface Mitigation
- **Inline Script Injection**: Blocked. Browsers reject any `<script>` tags that lack the exact matching `nonce` attribute.
- **External Script Loading**: Blocked (`script-src` restricts to nonce only; no CDN, remote domain, or arbitrary `http://` / `https://` sources are permitted).
- **Resource Hijacking**: Blocked (`default-src 'none'` disallows unlisted fetches, connections, frames, workers, or embeds).
- **Font & Asset Protection**: Restricted strictly to `${webview.cspSource}` (local extension package assets).

---

## 2. HTML Sanitization & XSS Prevention

### 2.1 Entity Escaping Strategy
All dynamic content rendered into HTML (user prompts, assistant responses, tool arguments, file paths, and summaries) passes through HTML entity encoding:

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

### 2.2 Markdown Rendering Pipeline
The markdown parser in `markdownRenderer.ts` and `ChatHtmlBuilder.ts` adheres to a strict two-stage process:
1. **Stage 1 (Complete Escaping)**: The entire raw markdown text is escaped using `escapeHtml()`. Malicious payloads such as `<script>alert(1)</script>`, `<img src=x onerror=...>`, or `javascript:void(0)` are converted to harmless plaintext entities (`&lt;script&gt;`).
2. **Stage 2 (Controlled Formatting Replacements)**: Regex replacements are applied only to standard markdown syntax patterns (fenced code blocks, inline code, bold, italics, headings, lists, blockquotes). No arbitrary HTML tags are ever reintroduced.

---

## 3. Zero Secrets in Webview Boundary

### 3.1 Threat Model
Webviews have distinct memory spaces and developer tool inspection surfaces. If API keys, access tokens, or private endpoints were serialized into webview HTML or sent via `postMessage`, malicious extensions or compromised dependencies could intercept them.

### 3.2 `sanitizeProviderSummary` Sanitization Filter
The extension host enforces a strict filter in `ProviderModelSelector.ts` (`sanitizeProviderSummary`) before any provider metadata is dispatched:

```typescript
export function sanitizeProviderSummary(raw: unknown): SafeProviderSummary | null {
  // Explicitly constructs SafeProviderSummary containing ONLY:
  // - instanceId (string)
  // - providerType (anthropic | openai | gemini | ...)
  // - displayName (string)
  // - status (available | unconfigured | offline)
  // - models: readonly SafeModelSummary[] (id, name, contextWindow, formattedContextWindow)
  //
  // Strips all of the following:
  // - apiKey / api_key
  // - clientSecret / secret
  // - bearerToken / token
  // - password
  // - authHeaders / headers / authorization
  // - custom endpoints containing embedded credentials
}
```

### 3.3 Verification
Automated pure unit tests in `controls.test.ts` verify that contaminated raw provider payloads containing dummy secret keys (`sk-ant-api03-...`, `secret_passphrase_...`, `Basic ...`) have 100% of their secrets eliminated before entering the webview HTML or state payloads.

---

## 4. Fail-Closed Approvals & Actions

### 4.1 Disconnected Bridge Security
If the daemon is disconnected, unreachable, or in an error state:
- All approval requests fail closed:
  ```typescript
  // DisconnectedBridge.ts
  public async requestApprovalDecision(id: string, decision: 'approved' | 'denied'): Promise<ApprovalDecisionResult> {
    throw new BridgeDisconnectedError('Cannot decide approval: daemon bridge is disconnected.');
  }
  ```
- All prompt submissions fail closed:
  ```typescript
  public async sendMessage(request: ChatSendRequest): Promise<ChatSendResult> {
    throw new BridgeDisconnectedError('Cannot send chat message: daemon bridge is disconnected.');
  }
  ```
- All editor context actions (`explainSelection`, `refactorSelection`, `generateTests`) fail closed with an explicit warning banner: `Cannot execute action: daemon bridge is disconnected.`

### 4.2 Modal Confirmation Escalation
- `LOW` and `MEDIUM` risk actions can be approved directly inline.
- `HIGH` and `CRITICAL` risk actions (e.g. destructive file deletions, package installation, shell commands) mandate a native modal dialog (`vscode.window.showWarningMessage` with `{ modal: true }`).
- Dismissing or cancelling the dialog aborts the approval immediately without notifying the daemon or updating state.

---

## 5. Workspace Trust Enforcement

VS Code's Workspace Trust security feature protects developers when opening untrusted repositories:
- When `vscode.workspace.isTrusted === false`:
  - AI code execution, file mutation commands, and tool invocations are strictly blocked.
  - The extension presents an informational warning instructing the user to trust the workspace before running agent actions.
  - Read-only queries in `ask` mode remain permitted only for basic file inspection.

---

## 6. Safe Diff Review & Disk Mutation Prevention

### 6.1 Read-Only Virtual Diff Documents
Diff previews are served in-memory via the virtual document provider `agent-studios-diff://`. No temporary files are written to the workspace or operating system `/tmp` directory.

### 6.2 Reconciliation Proof Enforcement
In `DiffReviewManager.ts`:
- Workspace files are **never** modified on disk directly by webview commands.
- File writes require a verified `BackendReconciliationProof` (`proof.verified === true && proof.reconciliationId`).
- Unverified disk modification requests are rejected immediately:
  `Safe review policy violation: Mutation of workspace file 'X' rejected. Backend reconciliation is not verified.`
