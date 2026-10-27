# Agent Studios CLI (`ags`) — Component Inventory & Ratatui Architecture

> **Document**: `AGS_CLI_COMPONENT_INVENTORY.md`  
> **Status**: Canonical Widget Inventory & Implementation Spec for Agent C  
> **Target Subsystem**: `ags` / Ratatui UI Component Hierarchy  
> **Precedence**: Aligned with `AGS_CLI_VISUAL_DESIGN_SYSTEM.md` and `AGS_CLI_TUI_LAYOUTS.md`.

---

## 1. Component Hierarchy & Architecture Overview

The `ags` user interface is modeled as a strictly typed, unidirectional state-driven tree of Ratatui widgets. State is stored in an authoritative UI state model driven by the Agent Studios Control Plane event receiver.

```text
AgsApp (Root Component)
 ├── HeaderWidget (Path, Git, Model, Runtime, Session)
 ├── AgentActivityBar (Horizontal Ribbon of AgentChipWidgets)
 │    └── [AgentChipWidget] (Role, Task, State, Duration)
 ├── ViewportContainer (Split or Single Column)
 │    ├── TranscriptWidget (Virtualized Scrollable Message Stream)
 │    │    ├── UserMessageBlock
 │    │    ├── AssistantMessageBlock (Streaming Text + Thoughts)
 │    │    ├── MarkdownBlockWidget (Headings, CodeBlocks, Lists)
 │    │    ├── ToolCallWidget (Collapsed / Expanded Tool Execution)
 │    │    └── PatchSummaryBlock (Additions / Deletions / Files)
 │    └── [Optional Right Pane] TaskGraphWidget / LiveDiffInspector
 ├── ComposerWidget (Multiline Input, Grapheme Cursor, Bracketed Paste)
 ├── StatusBarWidget (Token Counter, Cost Estimator, Key Hints)
 └── ModalLayer (Top Z-Index Overlays)
      ├── ApprovalModalWidget (Security Gate)
      ├── DiffViewerWidget (Unified Patch Review)
      ├── SlashCommandOverlayWidget (Fuzzy Command Palette)
      └── ModelPickerModalWidget (Provider / Model Selector)
```

---

## 2. Complete Component Specifications

### 2.1 `HeaderWidget`

#### Purpose
Renders the top 1-2 lines of the application, anchoring the developer with the current working directory, git branch, active provider/model, and runtime.

#### Data Model
```rust
pub struct HeaderState {
    pub workspace_root: PathBuf,
    pub git_branch: Option<String>,
    pub git_is_dirty: bool,
    pub provider_instance: String, // e.g., "anthropic"
    pub model_id: String,          // e.g., "claude-3-7-sonnet"
    pub runtime_id: String,        // e.g., "internal-codex"
    pub session_id: String,        // e.g., "s_9a8f21"
    pub session_duration_secs: u64,
}
```

#### Ratatui Rendering Logic
- If terminal width $< 90$: Render single row with truncated path and branch (`ags ~/auth-svc · 󰊢 feat/auth* · claude-3-7`).
- If terminal width $\ge 90$: Full 1-row layout with separate badges for Provider, Model, and Runtime.
- Colors: `surface_card` background, `border_base` bottom rule, `brand_accent` for `[ags]`, `status_warning` for dirty git indicator.

---

### 2.2 `AgentActivityBar` & `AgentChipWidget`

#### Purpose
Visualizes the deterministic multi-agent team in real-time without overwhelming the transcript. When 1+ worker agents are spawned by the Coordinator, this horizontal ribbon renders active worker chips.

#### Data Model
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    Starting,
    Idle,
    Running { current_action: String, elapsed_secs: u64 },
    Blocked { reason: String },
    Succeeded,
    Failed { error: String },
}

pub struct AgentChipState {
    pub agent_id: String,
    pub role_name: String,       // e.g., "Coordinator", "Coder", "Tester", "Reviewer"
    pub assigned_task_id: Option<String>,
    pub worktree_rel_path: Option<String>,
    pub status: AgentStatus,
}

pub struct AgentActivityBarState {
    pub agents: Vec<AgentChipState>,
    pub focused_index: Option<usize>,
}
```

#### Visual Behavior
- Idle agents: Gray hollow circle (`○`), muted role text.
- Running agents: Studio Emerald / Magenta solid circle (`●`), highlighted role text, active file/action, ticking seconds.
- Blocked agents: Amber circle (`●`), blocked reason.
- Layout: Rendered horizontally using Ratatui `Spans` with `│` dividers between chips.

---

### 2.3 `TranscriptWidget`

#### Purpose
Virtualized scrollback stream rendering the chronological dialogue, reasoning chains, tool invocations, and patch reconciliation events.

#### Data Model
```rust
pub enum TranscriptItem {
    UserPrompt {
        prompt: String,
        timestamp: chrono::DateTime<chrono::Local>,
    },
    AssistantTurn {
        agent_role: String,
        thought_chunks: Vec<String>,
        markdown_content: String,
        is_streaming: bool,
    },
    ToolInvocation(ToolCallState),
    PatchReconciled {
        target_branch: String,
        files_changed: usize,
        additions: usize,
        deletions: usize,
    },
    SystemNotification {
        level: NotificationLevel, // Info, Warning, Error
        message: String,
    },
}

pub struct TranscriptState {
    pub items: Vec<TranscriptItem>,
    pub scroll_offset: usize,
    pub auto_scroll_to_bottom: bool,
    pub selected_item_index: Option<usize>,
}
```

#### Virtualization & Performance
- Only lines visible in the allocated Ratatui viewport rectangle (`Rect`) are calculated and drawn.
- Line wrapping is cached with `textwrap::wrap` and recomputed only on terminal resize events.
- Zero flicker: Double-buffered frame drawing via Ratatui `Terminal::draw`.

---

### 2.4 `MarkdownBlockWidget`

#### Purpose
Renders Markdown streams from assistant agents with syntax-highlighted code blocks, inline code, bold/italic formatting, and ordered/unordered lists.

#### Integration
- Parser: `pulldown-cmark`.
- Code Highlighting: `syntect` with bundled sublime-syntaxes (Rust, Go, TypeScript, Python, C++, SQL, JSON, YAML, TOML).
- Code Blocks are framed in subtle single-line border boxes with the language name pinned to the top-right header:
```text
  ┌─ rust ────────────────────────────────────────────────────────┐
  │ pub async fn authenticate(req: &Request) -> Result<Session> { │
  │     let token = req.bearer_token()?;                          │
  │     TokenValidator::verify(&token).await                      │
  │ }                                                             │
  └───────────────────────────────────────────────────────────────┘
```

---

### 2.5 `ToolCallWidget`

#### Purpose
Represents an individual tool call executed by an agent (e.g., `read_file`, `apply_patch`, `exec_command`, `list_directory`).

#### States
1. **Collapsed (Default)**:
   - Takes exactly 1 line.
   - Shows tool icon (``), tool name, primary argument (truncated to fit), status badge (`[✓ 12ms]` or `[✗ Failed]`).
2. **Expanded (On `Space` or `Enter`)**:
   - Expands to show complete JSON parameters, execution working directory, stdout/stderr pager, and copy shortcuts.

#### Data Model
```rust
pub struct ToolCallState {
    pub tool_name: String,
    pub parameters_summary: String,
    pub full_parameters_json: String,
    pub working_dir: PathBuf,
    pub risk_tier: RiskTier, // ReadOnly, WorkspaceMutating, DangerousHost
    pub execution_status: ExecutionStatus, // Running, Succeeded, Failed, Denied
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub execution_time_ms: u64,
    pub is_expanded: bool,
}
```

---

### 2.6 `ComposerWidget`

#### Purpose
Primary developer input container. Handles multiline text entry, UTF-8 NFC normalization, Vietnamese diacritic typing, history recall, bracketed paste, and suggestion popups.

#### Data Model
```rust
pub struct ComposerState {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col_graphemes: usize, // Grapheme cluster index, NOT byte index!
    pub history: Vec<String>,
    pub history_cursor: Option<usize>,
    pub is_focused: bool,
    pub min_height: u16, // Default 3
    pub max_height: u16, // Default 8
}
```

#### Vietnamese & UTF-8 Invariants
- Cursor positioning MUST calculate display width via `unicode_width::UnicodeWidthStr::width` on grapheme slices up to `cursor_col_graphemes`.
- Handles multi-byte composition events from Windows IME (Unikey, EVKey, Windows Vietnamese Telex).
- `Shift+Enter` inserts a newline; `Enter` submits the prompt.

---

### 2.7 `ApprovalModalWidget`

#### Purpose
High-priority security modal that interrupts the event loop when a tool call requires explicit developer authorization.

#### Data Model
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskTier {
    ReadOnly,
    WorkspaceMutating,
    DangerousHost,
}

pub struct ApprovalRequest {
    pub request_id: String,
    pub agent_role: String,
    pub task_id: String,
    pub tool_name: String,
    pub command_or_path: String,
    pub working_directory: PathBuf,
    pub risk_tier: RiskTier,
    pub explanation: String,
}
```

#### Visual Design
- Centered floating box (80% width, 60% height).
- Double border for `DangerousHost` (in `border_danger` Crimson).
- Displays exact command line arguments and working directory.
- Clear keyboard action strip: `[y] Approve Once`, `[n] Reject & Stop`, `[a] Allow for Session`, `[d] Inspect Diff`.

---

### 2.8 `DiffViewerWidget`

#### Purpose
Dedicated modal or pane for inspecting unified diffs produced by `apply_patch` or git workspace mutations.

#### Features
- Unified diff rendering with line numbers for old and new files.
- Added lines highlighted with `diff_added` text and `diff_added_bg` background.
- Removed lines highlighted with `diff_removed` text and `diff_removed_bg` background.
- Hunk headers (`@@ -12,4 +12,6 @@`) highlighted in cyan.
- Multi-file tabs (`[1/3] src/auth.rs`, `[2/3] src/jwt.rs`).
- Fast hunk navigation (`n` for next hunk, `p` for previous hunk).

---

### 2.9 `StatusBarWidget`

#### Purpose
Bottom line of the interface providing continuous ambient status and dynamic keyboard shortcut indicators.

#### Content Elements
- Left: Context token counter (`Tokens: 14,240 / 200,000 (7%)`), estimated session cost (`$0.04`).
- Middle: Control Plane connection indicator (`󰍛 Connected (12ms)`).
- Right: Contextual keyboard hints matching active focus (e.g., `[Enter] Submit · [Tab] Commands · [Ctrl+O] IDE`).

---

## 3. Localization String Keys Inventory

Per `LOCALIZATION.md`, every component draws user-facing strings from standard key bundles.

| Component | Key Identifier | English (`en-US`) | Vietnamese (`vi-VN`) |
| :--- | :--- | :--- | :--- |
| `HeaderWidget` | `header.clean` | `clean` | `sạch` |
| `HeaderWidget` | `header.dirty` | `dirty` | `đã sửa` |
| `AgentChipWidget` | `agent.role.coordinator` | `Coordinator` | `Điều phối viên` |
| `AgentChipWidget` | `agent.role.coder` | `Coder` | `Lập trình viên` |
| `AgentChipWidget` | `agent.role.tester` | `Tester` | `Kiểm thử viên` |
| `AgentChipWidget` | `agent.role.reviewer` | `Reviewer` | `Người đánh giá` |
| `AgentChipWidget` | `agent.status.running` | `Running` | `Đang thực thi` |
| `AgentChipWidget` | `agent.status.blocked` | `Blocked` | `Đang chờ` |
| `ComposerWidget` | `composer.placeholder` | `Ask Agent Studios or type '/' for commands...` | `Nhập yêu cầu cho Agent Studios hoặc gõ '/'...` |
| `ApprovalModal` | `approval.title` | `APPROVAL REQUIRED` | `YÊU CẦU PHÊ DUYỆT` |
| `ApprovalModal` | `approval.risk.high` | `High Risk (Host Mutation)` | `Mức độ rủi ro: Cao (Thay đổi máy)` |
| `ApprovalModal` | `approval.btn.approve` | `Approve Once` | `Phê duyệt một lần` |
| `ApprovalModal` | `approval.btn.reject` | `Reject & Stop` | `Từ chối & Dừng lại` |
| `DiffViewer` | `diff.title` | `REVIEW DIFF` | `ĐÁNH GIÁ BẢN VÁ` |
| `DiffViewer` | `diff.files_count` | `Files Changed` | `Tệp đã sửa đổi` |
| `StatusBar` | `status.tokens` | `Tokens` | `Số token` |
| `StatusBar` | `status.cost` | `Cost` | `Chi phí` |
| `StatusBar` | `status.hint.open_ide` | `Open IDE` | `Mở trong IDE` |
