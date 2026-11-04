# M15 UI-05A — Report 05: Truthful UX & Operator Safety Architecture

## Metadata
- **Milestone**: M15 UI-05A: Frontend Reliability, Shared State Consistency & Integration Readiness
- **Focus**: Cryptographic Confirmation, Truthful Runtime UX, Secret Retention & Fail-Closed Gating
- **Target Worktree**: `D:\Projects\agent-studios-ide-ui\apps\agent-studios-vscode`
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`)
- **Author**: Claude 2 (Frontend / VSCode Extension Integration Lead)
- **Date**: 2026-11-02

---

## 1. Problem Statement & UX Safety Boundaries

In an autonomous development environment, UI representations must strictly reflect reality:
- **No Illusion of Execution**: Detecting a local `claude` CLI executable binary on disk must never be conflated with having run an autonomous agent loop or spent billable API tokens.
- **Fail-Closed Execution Gating**: Even if an operator enters a prompt in the Chat panel, the extension must refuse to execute mutating task submissions without explicit, cryptographically verifiable operator authorization.
- **Secret Isolation**: Cryptographic authentication secrets and HMAC tokens must never be passed to untrusted Webview contexts where malicious scripts or DOM extensions could inspect them.

---

## 2. Hardened Architecture & Security Protocols

### A. Cryptographic Operator Confirmation Tokens (`OperatorConfirmationManager.ts`)
When a mutation action is requested (e.g. task execution or approval):
1. **Challenge Generation**: The Extension Host generates an HMAC-SHA256 challenge token:
   ```typescript
   const challenge = {
       studioId,
       promptHash: crypto.createHash('sha256').update(promptText).digest('hex'),
       nonce: crypto.randomBytes(16).toString('hex'),
       expiresAt: Date.now() + 60_000 // 60-second TTL
   };
   const token = hmacSign(challenge, internalExtensionHostSecret);
   ```
2. **Single-Use Consumption**: Once consumed or expired, the challenge nonce is stored in a consumed set, preventing replay attacks.
3. **Mismatched Prompt / Studio Rejection**: If the prompt content or studio ID is altered between presentation and submission, validation fails closed.

### B. Extension Host vs Webview Boundary Isolation
- The Webview receives only a sanitized user-facing description of the action and an opaque, non-sensitive challenge ID.
- The `internalExtensionHostSecret` and raw HMAC signatures remain exclusively in Node.js memory inside the Extension Host.
- Webviews communicate strictly via structured `vscode.postMessage` calls validated against JSON schema interfaces.

### C. Truthful Runtime Status UX (`ChatHtmlBuilder.ts`)
- **Connected / Disconnected Indicators**: The chat panel displays clear visual badges:
  - `Offline / Mock`: Amber badge indicating daemon mock mode.
  - `Daemon Connected (agsd)`: Blue badge confirming authentic local IPC.
  - `Read-Only Sandbox`: Clear badge reminding operator that mutating tasks are gated closed.
- **CLI Binary Inspection**: Inspecting `claude --version` is labeled truthfully as "CLI Binary Detected" rather than "Agent Running".

### D. Production Static Gating
- Production `task.submit` capability remains statically set to `false`.
- Attempting to submit tasks without overriding this gate via test harnesses or authorized admin configuration throws `TaskSubmissionDisabledError`.

---

## 3. Verification & Regressions Handled

- **Test Suites**:
  - `src/test/unit/OperatorConfirmationSafety.test.ts`
  - `src/test/e2e/RealAgentExecutionE2E.test.ts` (Tests 6–19)
- **Verified Invariants**:
  - Challenge tokens expire after 60 seconds.
  - Replayed challenge tokens are rejected.
  - Mutated prompts fail HMAC verification.
  - Unconfirmed task submissions fail closed.
