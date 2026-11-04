# M15 UI-01: Operator Confirmation Flow & Cryptographic Gate

**Document ID**: `M15-UI-01-DOC-02`  
**Milestone**: M15 UI-01 Interactive Agent Execution Workbench  
**Author**: Principal VS Code Extension Engineer  
**Date**: 2026-10-31  
**Status**: Completed & Verified  

---

## 1. Security Invariants

The operator confirmation modal protects the developer's workspace and host environment from untrusted mutations. It adheres to strict invariants:

1. **Host-Owned UI Isolation**: Webview-rendered confirmation dialogs are strictly prohibited. The webview is treated as an untrusted presentation layer. Confirmation is driven exclusively by the Extension Host using native `vscode.window.showWarningMessage` with `{ modal: true }`.
2. **Cryptographic Parameter Binding**: The Extension Host generates an HMAC-SHA256 challenge token atomically bound to:
   - `token`: 16-byte random hex nonce.
   - `studioId`: Target execution studio identifier.
   - `promptHash`: SHA-256 digest of normalized prompt text.
   - `workspacePath`: Normalized canonical workspace root path.
   - `modelId`: Selected AI model ID.
   - `providerId`: Selected AI provider ID.
   - `intent`: Fixed string identifier (`task_execution`).
   - `expiresAt`: Monotonic expiration timestamp (default 5-minute TTL).
3. **Atomic Single-Use Consumption**: Once verified during submission preflight, the challenge token is marked `consumed = true` and purged, preventing replay attacks.
4. **Cancellation Purge**: If the operator cancels or dismisses the modal, all pending challenges are immediately purged (`OperatorConfirmationManager.clearPendingChallenges()`).
5. **Production Injection Prevention**: Test dialog handlers (`setDialogHandlerForTesting`) are restricted to test environments; any invocation in `NODE_ENV === 'production'` immediately throws an error.

---

## 2. Confirmation Sequence

```
Operator                Webview             ChatWebviewViewProvider       OperatorConfirmationModal     CapabilityMatrix
   |                       |                           |                             |                       |
   |--- Click "Start Task" |                           |                             |                       |
   |    (or Ctrl+Enter) -->|                           |                             |                       |
   |                       |--- submitTask message --->|                             |                       |
   |                       |                           |-- Validate capability & ws -|                       |
   |                       |                           |-- requestConfirmation() --->|                       |
   |                       |                           |                             |-- createChallenge() ->|
   |                       |                           |                             |<-- challenge token ---|
   |                       |                           |                             |-- showWarningMessage -|
   |                       |<-------------------------- Native Modal Dialog ---------|   ({ modal: true })   |
   |                                                                                                         |
   |-- [Confirm & Execute] --------------------------------------------------------------------------------->|
   |                                                                                 |                       |
   |                                                   |<-- confirmed: true, token --|                       |
   |                                                   |-- submitTask(token) ------------------------------->|
   |                                                   |                                                     |-- verify & consume
   |                       |<-- taskSubmitted ---------|                                                     |
```

---

## 3. Implementation Reference

The modal implementation is located in `apps/agent-studios-vscode/src/security/OperatorConfirmationModal.ts`:

```typescript
export class OperatorConfirmationModal {
  public static async requestConfirmation(
    request: OperatorConfirmationDialogRequest
  ): Promise<OperatorConfirmationDialogResult> {
    if (!request.studioId?.trim()) {
      return { confirmed: false, reason: 'Studio ID is required.' };
    }
    if (!request.prompt?.trim()) {
      return { confirmed: false, reason: 'Prompt text is required.' };
    }

    // 1. Issue HMAC-SHA256 challenge bound to execution parameters
    const challenge = OperatorConfirmationManager.createChallenge(
      request.studioId,
      request.prompt,
      {
        workspacePath: request.workspacePath,
        modelId: request.modelId,
        providerId: request.providerId,
        intent: 'task_execution',
      }
    );

    // 2. Format human-readable preview
    const detail = [
      `Action: ${request.actionDescription || 'Mutating Agent Task Execution'}`,
      `Studio: ${request.studioName || request.studioId}`,
      `Runtime: ${request.runtime}`,
      `Provider / Model: ${request.providerId || 'default'} / ${request.modelId || 'default'}`,
      `Workspace: ${request.workspacePath || 'None configured'}`,
      '',
      `Prompt Preview: "${request.prompt.trim().replace(/\r?\n+/g, ' ').substring(0, 200)}..."`,
      '',
      'WARNING: This task may modify workspace files or run commands. Ensure you trust the requested action.'
    ].join('\n');

    // 3. Native modal dialog
    const choice = await vscode.window.showWarningMessage(
      'Agent Studios: Confirm Mutating Agent Task Execution',
      { modal: true, detail },
      'Confirm & Execute',
      'Cancel'
    );

    if (choice !== 'Confirm & Execute') {
      OperatorConfirmationManager.clearPendingChallenges();
      return { confirmed: false, cancelledByUser: true };
    }

    return { confirmed: true, challenge };
  }
}
```
