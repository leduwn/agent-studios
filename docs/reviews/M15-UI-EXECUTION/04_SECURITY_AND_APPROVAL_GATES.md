# M15 — Security Boundaries, Approval Gates & Breakout Protections

**Document Reference**: `docs/reviews/M15-UI-EXECUTION/04_SECURITY_AND_APPROVAL_GATES.md`  
**Milestone**: M14 Final Integration Freeze & M15 UI Execution Preparation  
**Auditor**: Principal Security Reviewer & Cross-Platform IPC Engineer  
**Date**: 2026-10-30  
**Status**: VERIFIED SECURITY ARCHITECTURE  

---

## 1. Threat Model & Trust Boundaries

The Agent Studios architecture operates across three distinct security boundaries:
1. **User Workspace Boundary**: The local Git repository containing potentially untrusted code.
2. **VS Code Extension Host (Client)**: Renders webviews, parses IPC packets, formats UI state.
3. **Native Rust Daemon (`agsd`)**: Executes with OS-level authority to spawn child processes, manage worktrees, and communicate with external LLM APIs.

```
[Untrusted Workspace] <---- Isolated Worktree & Win32 Job Object ---- [agsd Daemon]
                                                                           ^
                                                                           | Authenticated Local IPC
                                                                           v
                                                           [VS Code Extension Host]
```

---

## 2. Two-Phase Approval Delivery (2PC) & Origin Binding

### 2.1 Backend Contract
When an agent attempts a mutating tool action (e.g. `write_file`, `execute_command`), the daemon halts execution and emits `approval_requested` over IPC.

- **Session Binding**: Approvals are bound to the specific client session UUID that received the prompt. Another unauthenticated connection cannot hijack pending approvals.
- **Fail-Closed on Drop**: If the IPC transport disconnects while an approval is pending, the daemon treats the approval as unconfirmed and aborts the tool execution.
- **Pre-Execution Delivery**: Approval resolutions (`approved` or `rejected`) must be acknowledged by the daemon (`IpcResponse::ApprovalResolved`) before tool execution is allowed to begin.

### 2.2 UI Risk Normalization & Modal Warning
Approvals are classified into four normalized risk levels:
- `LOW` & `MEDIUM`: Quick confirmation in chat card or approvals view.
- `HIGH` & `CRITICAL`: Triggers a blocking VS Code modal dialog (`vscode.window.showWarningMessage(..., { modal: true })`). If the user cancels the modal dialog, approval is aborted and fails closed.

---

## 3. Path Traversal & Breakout Protections

The VS Code extension enforces strict sanitization on all file paths and URI references:

1. **Relative Traversal Sequences**: Paths containing `..`, `../`, or `..\` are rejected immediately before passing to filesystem or diff APIs.
2. **Null-Byte Injection**: Paths containing `\0` are blocked.
3. **Windows UNC Paths**: Paths beginning with `\\` or `//` are rejected to prevent NTLM credential relay attacks.
4. **URL-Encoded Traversal**: Encoded sequences such as `%2e%2e%2f` are decoded and asserted against traversal rules.
5. **Dangerous Protocol Schemes**: Custom schemes (`file:`, `vscode:`, `agent-studios-diff:`) are strictly allowlisted; unknown or executable schemes (`javascript:`, `data:`, `shell:`) fail closed.
6. **Windows Drive Letters**: Colons (`:`) are only permitted when immediately following a valid single drive letter prefix (e.g., `C:\path` or `D:/path`).

---

## 4. Credential Scrubbing Invariants

- **Standard vs. Admin Separation**: `agsd` writes two distinct bootstrap files:
  * `agsd.json`: Contains standard client credentials (`key_id`, `client_key`). Read-only query commands and task submissions are permitted.
  * `agsd-admin.json`: Contains administrative credentials (`admin_key`). Required for daemon lifecycle management (`IpcCommand::Shutdown`).
- **Zero Credential Leaks**: IPC responses (`list_providers`, `list_models`) are scrubbed before storage in `SessionStateProjection` or transmission to webview message ports. API keys, auth bearer tokens, and private signing keys are never serialized into view state or logged to disk.
