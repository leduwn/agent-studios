# M15 UI-01: Execution Workbench Architecture

**Document ID**: `M15-UI-01-DOC-01`  
**Milestone**: M15 UI-01 Interactive Agent Execution Workbench  
**Author**: Principal VS Code Extension Engineer  
**Date**: 2026-10-31  
**Status**: Completed & Verified  

---

## 1. Architectural Overview

Milestone M15 UI-01 establishes the interactive agent execution workbench in `agent-studios-vscode`, enabling operators to configure, confirm, submit, observe, and cancel real coding agent tasks from within VS Code.

The architecture connects five core modules:

```
+-----------------------------------------------------------------------------------+
| VS Code Extension Host (Trusted Context)                                          |
|                                                                                   |
|  +-----------------------------+         +-------------------------------------+  |
|  | ChatWebviewViewProvider     | <-----> | OperatorConfirmationModal           |  |
|  | - Command allowlist & auth  |         | - Host-owned modal dialog           |  |
|  | - Workspace containment     |         | - HMAC-SHA256 challenge generation  |  |
|  +--------------+--------------+         +------------------+------------------+  |
|                 |                                           |                     |
|                 v                                           v                     |
|  +-----------------------------+         +-------------------------------------+  |
|  | TaskExecutionStore          |         | CapabilityMatrix                    |  |
|  | - Singleton execution state |         | - Fail-closed capability gate       |  |
|  | - Timeline event projection |         | - In-memory challenge token store   |  |
|  +--------------+--------------+         +-------------------------------------+  |
|                 |                                                                 |
|                 v                                                                 |
|  +-----------------------------+                                                  |
|  | ExecutionClient             |                                                  |
|  | - Serde IPC dispatch        |                                                  |
|  | - Rust wire framing parity  |                                                  |
|  +--------------+--------------+                                                  |
+-----------------|-----------------------------------------------------------------+
                  | (4 MiB length-prefixed Rust IPC wire)
                  v
+-----------------------------------------------------------------------------------+
| Agent Studios Rust Daemon (codex-rs)                                              |
| - Handles IpcCommand::SubmitTask & IpcCommand::CancelTask                         |
| - Emits Authoritative TimelineEvents over IPC socket                              |
+-----------------------------------------------------------------------------------+
```

---

## 2. Workbench UI Components

### 2.1 Configuration Toolbar (`workbench-toolbar`)
Positioned directly above the prompt composer in `ChatHtmlBuilder.ts`:
- **Runtime Selector (`#runtimeSelect`)**:
  - `claude_code`: Enabled by default (`Claude Code (Native CLI)`).
  - `codex_external`: Disabled with tooltip `Coming in M16: External process runner support`.
  - `custom_shell`: Disabled with tooltip `Custom Shell execution disabled`.
- **Studio Selector (`#studioSelect`)**: Selects active execution studio (defaults to `default-studio`, `agent-studios-core`).
- **Workspace Containment Badge (`#workspaceBadge`)**:
  - Displays canonicalized path.
  - State `workspace-badge--matched`: Shows `✓ Verified` when local workspace matches daemon session root.
  - State `workspace-badge--mismatched`: Shows `⚠ Mismatch` with reason details when mismatch is detected, blocking task start.

### 2.2 Execution Target Summary Card (`#executionSummaryCard`)
Provides transparent confirmation of execution target parameters prior to dispatch:
- **Runtime**: Claude Code (Native CLI).
- **Model**: Authoritative model name from connected provider summaries.
- **Workspace**: Canonical path with containment check.
- **Approval Policy**: Host-Owned Modal Dialog (cryptographically signed gate).

### 2.3 Multiline Prompt Composer
- **Input Area (`#composerInput`)**:
  - Auto-expanding multiline textarea supporting Markdown formatting.
  - Keyboard shortcuts: `Ctrl+Enter` to submit, `Shift+Enter` for newline insertion.
- **Frame Boundary Banners**:
  - Warning Banner (`#composerWarning`): Alerts operator when UTF-8 byte length exceeds 100 KB.
  - Error Banner (`#composerError`): Triggers assertive alert and disables submit button when prompt exceeds 4 MiB wire limit.
- **Control Buttons**:
  - Start Task (`#sendBtn`): Triggers operator modal confirmation flow.
  - Cancel Task (`#stopBtn`): Active during submission and running phases.
