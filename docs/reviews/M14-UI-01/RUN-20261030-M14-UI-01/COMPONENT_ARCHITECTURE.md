# Agent Studios — Milestone M14-UI-01: Component Architecture

## Architecture Overview
The M14-UI-01 architecture implements a chat-first coding assistant inside a native VS Code WebviewView. The extension separates webview UI presentation from extension host state management through a strictly typed, asynchronous message-passing bridge.

```
+--------------------------------------------------------------------------------+
|                             VS Code Extension Host                             |
|                                                                                |
|  +--------------------+         +-------------------+       +---------------+  |
|  | BridgeManager      | <-----> | SessionState      | ----> | 6 Native      |  |
|  | (AgentStudiosBridge|         | Projection        |       | TreeViews     |  |
|  +--------------------+         +-------------------+       +---------------+  |
|           |                                                                    |
|           v                                                                    |
|  +--------------------------------------------------------------------------+  |
|  |                     ChatWebviewViewProvider                              |  |
|  |                     (viewType: agent-studios.chat)                       |  |
|  |                                                                          |  |
|  |  +-------------------+  +-------------------+  +----------------------+  |  |
|  |  | ModeSelector      |  | ProviderModel     |  | ContextAttachment    |  |  |
|  |  | (Ask/Plan/Agent)  |  | Selector          |  | Manager              |  |  |
|  |  +-------------------+  +-------------------+  +----------------------+  |  |
|  |  +-------------------+  +-------------------+  +----------------------+  |  |
|  |  | SlashCommand      |  | DiffReviewManager |  | ChatHtmlBuilder      |  |  |
|  |  | Registry          |  |                   |  | (Strict CSP + Nonce) |  |  |
|  |  +-------------------+  +-------------------+  +----------------------+  |  |
|  +--------------------------------------------------------------------------+  |
|                                       ^                                        |
|                          postMessage  |  postMessage                           |
|                                       v                                        |
+---------------------------------------|----------------------------------------+
                                        | (IPC Message Passing)
+---------------------------------------|----------------------------------------+
|                             Webview Context                                    |
|                                                                                |
|  +--------------------------------------------------------------------------+  |
|  | Header: Brand, Connection Status Dot, Session Switcher, New Session Btn  |  |
|  +--------------------------------------------------------------------------+  |
|  | Conversation Container (role="log", aria-live="polite", smooth scroll)   |  |
|  |   - User Message Bubbles (Content + Attached Context Chips)              |  |
|  |   - Assistant Message Bubbles (Markdown formatting + Blinking Cursor)   |  |
|  |   - ToolCallCard (Icon, Duration, Status Spinner/Check/Error, Foldable)  |  |
|  |   - ApprovalCard (Risk Badge, Inline Approve/Deny, Modal Warning)        |  |
|  |   - TaskProgressCard (Active Agent, Progress Bar, Completed/Total)       |  |
|  |   - ChangedFilesCard (+X/-Y stats, One-click Working Tree Diff)          |  |
|  +--------------------------------------------------------------------------+  |
|  | Context Chips Bar (Removable file tags, Add File, Attach Active Editor)  |  |
|  +--------------------------------------------------------------------------+  |
|  | Composer (Auto-growing textarea, Draft persistence, Enter / Shift+Enter) |  |
|  +--------------------------------------------------------------------------+  |
|  | Action Toolbar (Mode Dropdown, Model Dropdown, Stop Btn, Send Btn)        |  |
+--------------------------------------------------------------------------------+
```

---

## 1. WebviewView Provider (`ChatWebviewViewProvider`)

- **File**: `apps/agent-studios-vscode/src/chat/ChatWebviewViewProvider.ts`
- **View Type**: `agent-studios.chat` (registered in `package.json` under `agent-studios-activitybar`)
- **Lifecycle Management**:
  - `resolveWebviewView()`: Instantiates HTML with cryptographically secure nonce and sets `localResourceRoots`.
  - `onDidChangeVisibility()`: Listens for tab reveal and triggers `syncState()` to reconnect session metrics, active providers, and pending approvals.
  - `dispose()`: Cleans up listeners, cancels pending stream subscriptions, and unbinds events.

### Typed Bidirectional Protocol

```typescript
// Webview -> Extension Host
export type WebviewToHostMessage =
  | { command: 'sendMessage'; text: string; mode: AgentMode; modelId?: string; providerId?: string; attachments?: ContextAttachment[] }
  | { command: 'cancelRun' }
  | { command: 'approveAction'; approvalId: string }
  | { command: 'denyAction'; approvalId: string }
  | { command: 'openDiff'; artifactId?: string; filePath?: string }
  | { command: 'attachContext'; source: 'filePicker' | 'activeEditor' }
  | { command: 'removeAttachment'; path: string }
  | { command: 'saveDraft'; draft: string }
  | { command: 'switchMode'; mode: AgentMode }
  | { command: 'switchProviderModel'; providerId: string; modelId: string }
  | { command: 'switchSession'; sessionId: string }
  | { command: 'newSession' }
  | { command: 'openTaskGraph' };

// Extension Host -> Webview
export type HostToWebviewMessage =
  | { type: 'syncState'; state: ChatClientState }
  | { type: 'streamChunk'; chunk: ChatStreamChunk }
  | { type: 'messageComplete'; messageId: string }
  | { type: 'approvalResult'; approvalId: string; decision: 'approved' | 'denied'; applied: boolean; error?: string }
  | { type: 'contextAttached'; attachment?: ContextAttachment; attachments: ContextAttachment[] };
```

---

## 2. Composer & Context Attachment Manager

### 2.1 Composer Input
- **Auto-Growing Textarea**: Dynamically recalculates `scrollHeight` between 40px minimum and 150px maximum.
- **Draft Persistence**: Synchronizes draft changes via `saveDraft` command to `ChatWebviewViewProvider._composerDraft`. Survives panel switching and view re-opening.
- **Keyboard Navigation**:
  - `Enter` (without Shift): Submits active prompt and context attachments.
  - `Shift+Enter`: Inserts a newline without dispatching.

### 2.2 Context Attachment Manager (`ContextAttachmentManager`)
- **File**: `apps/agent-studios-vscode/src/composer/ContextAttachmentManager.ts`
- **Supported Context Types**:
  1. `file`: Workspace files selected via QuickPick or typed `@`-paths.
  2. `selection`: Exact 1-based line ranges captured from `activeTextEditor`.
  3. `diagnostics`: Extracted compiler errors and warnings for the active file.
- **Security & Economy Bounds**:
  - Max 5 context attachments per prompt.
  - Max 20KB per attachment (UTF-8 byte truncation with omission indicator).
  - Max 200 lines per text selection snippet.

### 2.3 Slash Command Registry (`SlashCommandRegistry`)
- **File**: `apps/agent-studios-vscode/src/composer/SlashCommandRegistry.ts`
- Canonical commands:
  - `/help`: Lists commands, keyboard shortcuts, and agent mode capabilities.
  - `/clear`: Clears conversation message log and resets composer.
  - `/plan`: Sets mode to `plan` and prefaces architecture formulation.
  - `/review`: Initiates code review against active diffs or targets.
  - `/explain`: Captures editor selection snippet and prompts explanation.
  - `/tests`: Generates unit test scaffolding for active document.
  - `/providers`: Formats configured providers and models in markdown table.
  - `/sessions`: Lists active sessions with fast-switch affordance.

---

## 3. Card Component Architecture (`src/cards/`)

Cards are rendered using `CardRenderer` (`src/cards/CardRenderer.ts`) with strict HTML escaping, native VS Code theme tokens, and accessible ARIA attributes.

### 3.1 `ToolCallCard`
- **Visual Design**: 1px monochrome border, collapsible disclosure header, duration badge (`X ms`).
- **Tool Mapping**:
  - `read_file` -> `file-text`
  - `edit_file` -> `edit`
  - `search_codebase` -> `search`
  - `exec_command` -> `terminal`
  - `run_tests` -> `beaker`
  - `apply_patch` -> `diff`
  - `review_code` -> `eye`
  - `delegate_agent` -> `hubot`
- **Status Indicators**:
  - `running`: Animated CSS spinner (`@keyframes agent-studios-spin`).
  - `completed`: Green check badge (`--vscode-testing-iconPassed`).
  - `failed`: Red error badge (`--vscode-testing-iconFailed`).

### 3.2 `ApprovalCard`
- **Risk Classification**: `LOW`, `MEDIUM`, `HIGH`, `CRITICAL`.
- **Action Buttons**: `[Approve]` (`btn-primary`), `[Deny]` (`btn-secondary`).
- **Modal Escalation**: High and critical risk actions prompt a modal dialog in the editor window (`vscode.window.showWarningMessage` with `{ modal: true }`) before bridge dispatch.

### 3.3 `TaskProgressCard`
- **Agent Roles**: Displays coordinator name (e.g. `DevCore`) alongside active executing agent (e.g. `DevProto`).
- **Progress Gauge**: Visual progress bar (`--vscode-progressBar-background`) with completed/total task counts and percentage.
- **Deep Inspection**: Direct `[View Task Graph]` button launching the DAG webview.

### 3.4 `ChangedFilesCard`
- **Unified Diff Accounting**: Aggregates +X additions and -Y deletions per file.
- **One-Click Diff Review**: Button triggering `agent-studios.inspectArtifact` via `DiffDocumentProvider`.

---

## 4. Controls Architecture (`src/controls/`)

### 4.1 Mode Selector (`ModeSelector`)
- **Modes**:
  1. `Ask`: Read-only, no file edits, no command execution (`canEditFiles: false`, `canRunCommands: false`).
  2. `Plan`: Architectural planning and DAG generation (`canEditFiles: false`, `canProposeDag: true`).
  3. `Agent`: Full execution mode with tool invocations (`canEditFiles: true`, `canRunCommands: true`).
- **Persistence**: Remembers selection via `vscode.Memento` key `agentStudios.lastUsedMode`.

### 4.2 Provider & Model Selector (`ProviderModelSelector`)
- **Read-Only Model Discovery**: Lists providers and models returned by daemon without exposing configuration dialogs or secrets.
- **Context Window Formatting**: Normalizes token limits into clean badges (e.g. `200k tokens`, `1M tokens`).
- **Security Sanitization**: `sanitizeProviderSummary()` completely strips `apiKey`, `clientSecret`, `bearerToken`, and passwords prior to webview rendering.
- **Fallback Hierarchy**: Persisted selection -> provider default -> first available provider -> offline fallback.

---

## 5. Diff Review Architecture (`DiffReviewManager`)

- **File**: `apps/agent-studios-vscode/src/cards/DiffReviewManager.ts`
- **Virtual Document Scheme**: Uses `agent-studios-diff://` to serve proposed diffs directly from memory without writing temp files to the user's workspace.
- **Native Diff Viewer**: Dispatches `vscode.diff` between the active workspace file on disk and the virtual diff document.
- **Safe Review Policy**: Disallows workspace file writes without cryptographic reconciliation proof (`BackendReconciliationProof.verified === true`), preventing unverified automated file corruption.
