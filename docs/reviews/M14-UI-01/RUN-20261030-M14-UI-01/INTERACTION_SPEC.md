# Agent Studios — Milestone M14-UI-01: Interaction Specification

## 1. Keyboard Shortcuts & Input Handling

### 1.1 Prompt Submission & Multiline Editing
- **Primary Submit**: `Enter` (without modifier keys).
  - Triggers prompt validation: checks for non-whitespace content.
  - Submits the prompt, bundles active context attachments, updates local message bubbles, clears the textarea, and disables the Send button while displaying the Stop button.
- **Newline Insertion**: `Shift + Enter`.
  - Inserts a standard LF newline (`\n`) at cursor position.
  - Dynamically triggers `autoGrowTextarea()` to expand textarea height up to 150px maximum.
- **Escape Key**:
  - Closes any active autocomplete or dropdown menu.

### 1.2 Slash Commands
The composer checks if the prompt begins with a forward slash (`/`):

| Command | Arguments | Mode Action | Description |
|---|---|---|---|
| `/help` | None | No change | Displays formatted guide of all slash commands, shortcuts, and modes. |
| `/clear` | None | No change | Clears visible chat history and clears composer draft. |
| `/plan` | `<goal>` | Switches to `plan` | Initiates architectural breakdown and DAG formulation. |
| `/review` | `[target/diff]` | Switches to `agent` | Triggers code review against target file or active patch artifact. |
| `/explain` | `[question]` | Switches to `ask` | Captures active editor selection and prompts code explanation. |
| `/tests` | `[file]` | Switches to `agent` | Analyzes active document and generates unit test scaffolding. |
| `/providers` | None | No change | Renders markdown table of configured providers and context windows. |
| `/sessions` | `[switch <id>]` | No change | Lists available sessions or switches active session. |

- **Unsupported Command Handling**: If an unrecognized slash command is entered (e.g. `/deploy`), the system does not fake execution; it returns an immediate warning notice:
  `Command /deploy is not supported by Agent Studios.`

---

## 2. Streaming & Execution Cancellation

### 2.1 Live Streaming Behavior
- Upon prompt dispatch, an assistant message skeleton is immediately appended to the conversation log with an animated blinking cursor (`.streaming-cursor`).
- Token chunks are streamed over the bridge via `ChatStreamChunk`:
  - `text_delta`: Appends incremental markdown tokens to the active message body.
  - `tool_call`: Renders or updates a `ToolCallCard` in the message stream.
  - `task_progress`: Updates active agent indicator and progress percentage.
  - `approval_required`: Renders an `ApprovalCard` pausing execution until resolved.
  - `message_complete`: Removes the streaming cursor, resets composer state, hides Stop button, and enables Send button.

### 2.2 User-Initiated Run Cancellation
- **UI Element**: Sticky red stop button (`.btn-stop`) displayed in the action toolbar during execution.
- **Trigger**: Clicking the Stop button sends `{ command: 'cancelRun' }` to the extension host.
- **Extension Host Action**: Invokes `bridge.cancelCurrentRun()`.
- **State Guarantee**:
  - Halts incoming stream chunks immediately.
  - Preserves all output generated up to the cancellation timestamp.
  - Re-enables the prompt composer for follow-up prompts.

---

## 3. Context Attachment Workflows

### 3.1 Attachment Types
1. **File Attachment**:
   - Triggered via `+ Add File` chip button (opens VS Code file picker QuickPick) or typing `@<filename>`.
   - Captures relative workspace path, language ID, and document content.
2. **Active Editor Attachment**:
   - Triggered via `Attach Active File` chip button.
   - Captures active document URI. If an active selection exists, captures the selected text with 1-based start and end line numbers (e.g. `src/auth.ts (lines 10-25)`).
3. **Compiler Diagnostics Attachment**:
   - Automatically captured when running `/tests`, `/review`, or `/explain` on a file with active language server errors.
   - Formats errors and warnings as: `[Line X, Col Y] Severity: Message (Source)`.

### 3.2 Strict Bounding Constraints
To protect LLM context windows and prevent runaway memory usage:
- **Maximum Attachments**: 5 context chips per prompt. Additional attachments are blocked with a notification.
- **Maximum File Size**: 20KB per attachment. Content exceeding 20KB is truncated at the 20KB UTF-8 boundary with `... [content truncated at 20KB limit]`.
- **Maximum Selection Range**: 200 lines max per selection snippet.

---

## 4. Human-in-the-Loop Modal Confirmations

### 4.1 Risk-Based Decision Matrix

| Risk Level | Action Type Examples | UI Behavior | Confirmation Method |
|---|---|---|---|
| **LOW** | Read file, search codebase, list files | Inline `ApprovalCard` | Single-click `[Approve]` |
| **MEDIUM** | Modify non-critical file, run linting | Inline `ApprovalCard` | Single-click `[Approve]` |
| **HIGH** | Delete file, install dependencies, run bash script | Inline `ApprovalCard` + Modal Dialog | VS Code Modal Dialog (`{ modal: true }`) |
| **CRITICAL** | Git push, force overwrite, shell pipe commands | Inline `ApprovalCard` + Modal Dialog | VS Code Modal Dialog (`{ modal: true }`) with explicit warnings |

### 4.2 Modal Confirmation Flow
1. Developer clicks `[Approve]` on a HIGH or CRITICAL `ApprovalCard`.
2. Extension host catches the action and displays a native VS Code modal dialog:
   ```
   Warning: High Risk Agent Action
   Action: Execute shell command: rm -rf ./cache
   Target: rm -rf ./cache
   Requested Permissions: shell.exec, file.delete

   Are you sure you want to approve this action?
   [Approve Action] [Cancel]
   ```
3. If the developer clicks **[Cancel]** or presses `Esc`, the approval is aborted, the card remains pending or resets, and no daemon IPC command is dispatched.
4. If the developer clicks **[Approve Action]**, the decision is forwarded to the authoritative bridge.
5. If the bridge is disconnected, the action immediately **fails closed** (`BridgeDisconnectedError`) and the user is alerted.
