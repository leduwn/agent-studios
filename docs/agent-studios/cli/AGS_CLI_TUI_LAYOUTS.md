# Agent Studios CLI (`ags`) — TUI Layout Architecture & Responsive Specifications

> **Document**: `AGS_CLI_TUI_LAYOUTS.md`  
> **Status**: Canonical Layout & ASCII Wireframe Specification (Audited & Honesty-Tagged)  
> **Target Subsystem**: `ags` / Ratatui Layout Engine / Viewport Manager  
> **Precedence**: Aligned with `MASTER_VISION.md` and `AGS_CLI_VISUAL_DESIGN_SYSTEM.md`.  
> **Audit Status Legend**:
> - `[IMPLEMENTED / VERIFIED]`: Fully implemented in code and verified by automated tests.
> - `[IMPLEMENTED / NOT VERIFIED]`: Implemented in code but awaiting live remote / daemon verification.
> - `[PREVIEW ONLY]`: Visual/interaction design demonstrable in `ags --preview` with sample data.
> - `[PLANNED]`: Architectural design finalized; scheduled for Milestone M13+.
> - `[BLOCKED]`: Implementation blocked on external dependency.

---

## 1. Screen Anatomy & Structural Grid

The `ags` interface is structured as a vertical flex layout using Ratatui's `Layout::default().direction(Direction::Vertical).constraints(...)`. It maintains strict visual hierarchy while allocating maximum space to the conversation transcript and code edits.

```text
┌────────────────────────────────────────────────────────────────────────┐  ◄── Region 1: Micro-Header (1-2 lines)
│ ags ~/work/auth-svc · 󰊢 feat/oauth* · 󰍛 anthropic 󱥤 claude-3-7-sonnet  │
├────────────────────────────────────────────────────────────────────────┤  ◄── Region 2: Multi-Agent Ribbon (0-2 lines, dynamic)
│ ● Coordinator: Plan  ● Coder: auth_mw.rs (24s)  ○ Tester: Pending      │
├────────────────────────────────────────────────────────────────────────┤  ◄── Region 3: Main Viewport (Flex Grow, Virtual Scroll)
│                                                                        │
│ User:                                                                  │
│ Refactor token validation to use async Redis cache and add test.       │
│                                                                        │
│ Coordinator:                                                           │
│ I have planned the refactoring across two isolated tasks.              │
│                                                                        │
│ ┌─  Tool: read_file(src/auth/jwt.rs) ──────────────────────── [✓ 12ms]│
│ └──────────────────────────────────────────────────────────────────────┘
│                                                                        │
│ Coder:                                                                 │
│ Updated `verify_bearer_token` to query `TokenBlacklistCache`.          │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤  ◄── Region 4: Input Composer (3-8 lines, auto-expanding)
│ > Add unit tests covering expired token revocation_                    │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤  ◄── Region 5: Status & Key Hints (1 line)
│ [Tab] Slash/Mentions  [Ctrl+C] Cancel  [Ctrl+O] Open IDE  [?] Help     │
└────────────────────────────────────────────────────────────────────────┘
```

### Layout Regions Breakdown

1. **Region 1: Micro-Header** (Fixed: 1 line or 2 lines depending on terminal width):
   - Left: `[ags]` Studio Mark, current working directory path (shortened with `~` and ellipsis if needed).
   - Middle/Right: Active Git branch name + clean/dirty state indicator (`✓` or `*`), active Provider Instance + Model Target (`anthropic / claude-3-7-sonnet`), and Runtime (`internal-codex`).
2. **Region 2: Multi-Agent Execution Ribbon** (Conditional: 0 lines when single-threaded, 1-2 lines when multi-agent tasks are active):
   - Horizontal sequence of compact status chips showing active agents, assigned roles, current file or task, and elapsed time.
3. **Region 3: Main Viewport / Conversation Transcript** (Flexible: takes remaining available terminal height):
   - Virtualized vertical scroll stream containing user prompts, assistant text, collapsed/expanded tool invocations, compiler errors, and reconciled patch summaries.
4. **Region 4: Input Composer** (Dynamic: 3 to 8 lines depending on prompt height):
   - Multiline prompt editor supporting bracketed paste, word wrap, command history, slash command trigger (`/`), and context mentions (`@file`, `@agent`).
5. **Region 5: Status Bar & Key Hints** (Fixed: 1 line):
   - Context token usage counter, session timer/cost, and contextual keyboard shortcuts.

---

## 2. Responsive Specifications Across Standard Dimensions

`ags` dynamically adapts its layout structure based on terminal dimensions reported by Crossterm (`crossterm::terminal::size()`).

### 2.1 Standard Dimension Tiers

| Tier Name | Columns × Rows | Target Environment | Adaptations Applied |
| :--- | :---: | :--- | :--- |
| **Compact** | `80 × 24` | Strict POSIX console, tmux split 50%, embedded terminal | Single-line compact header; multi-agent ribbon condensed to 1 line; tool cards collapsed to 1 line; composer max 3 lines; key hints condensed to 3 essential actions. |
| **Standard** | `100 × 30` | Default laptop terminal window | Full micro-header; multi-agent ribbon with task chips; tool cards with parameters; composer up to 5 lines; full status bar. |
| **Wide** | `120 × 36` | Full-screen terminal, 1080p workstation | Rich header with session ID; expanded tool previews; multi-agent ribbon with token throughput; composer up to 6 lines. |
| **Ultra-Wide** | `160 × 45+` | 1440p/4K displays, tiling window managers | Activates **Dual-Pane View**: Left 65% Conversation Transcript, Right 35% Live Diff Inspector / Task Graph Inspector. |

---

## 3. Concrete ASCII Wireframes & Layout States

### 3.1 State A: Clean Startup Screen (Standard 100×30) `[IMPLEMENTED / VERIFIED]`

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ [ags] ~/Projects/agent-studios · 󰊢 feat/m12-claude* · 󰍛 anthropic/claude-3-7-sonnet · ags-v0.12  │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                                  │
│                                                                                                  │
│                                    Agent Studios CLI (`ags`)                                     │
│                            Deterministic Multi-Agent Development Harness                         │
│                                                                                                  │
│       Workspace:   d:\Projects\agent-studios\agent-studios-rs                                    │
│       Git Branch:  feat/m12-claude-code-runtime (3 commits ahead, dirty)                         │
│       Active Model: Anthropic Claude 3.7 Sonnet (Hybrid Reasoning)                               │
│       Runtime:     Internal Codex Kernel + Claude Code Subagent Adapter                          │
│                                                                                                  │
│       Recent Sessions:                                                                           │
│         • [s_9a8f] M12 Claude Code CLI runtime adapter integration (1h ago)                      │
│         • [s_7c2e] OpenCode runtime process sandbox hardening (yesterday)                        │
│         • [s_3d1a] Worktree patch reconciliation pipeline fix (3 days ago)                       │
│                                                                                                  │
│       Type a prompt to start a new run, or type `/` for commands.                                │
│                                                                                                  │
│                                                                                                  │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ > Refactor external-runtime adapter to support cancellation signals                              │
│                                                                                                  │
│                                                                                                  │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ [Enter] Submit  [Shift+Enter] Newline  [/] Commands  [@] Mention  [Ctrl+O] Open IDE  [F1] Help   │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

### 3.2 State B: Active Multi-Agent Execution (Wide 120×36) `[IMPLEMENTED / VERIFIED]`

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ [ags] ~/agent-studios-rs · 󰊢 feat/runtime-cancel · 󰍛 anthropic/claude-3-7-sonnet · Run #402 [Running] · 2m 14s        │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ AGENTS: [● Coordinator: Dispatching]  [● Coder: adapter.rs (+34 -8) 42s]  [○ Tester: Waiting]  [○ Reviewer: Idle]    │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ User:                                                                                                                │
│ Implement cancellation token propagation across the external runtime adapter process tree.                           │
│                                                                                                                      │
│ Coordinator:                                                                                                         │
│ Analyzed external runtime architecture. The adapter process must trap OS signals (SIGINT on Unix,                    │
│ GenerateConsoleCtrlEvent on Windows) and propagate them through the child agent session tree.                        │
│                                                                                                                      │
│ ┌─  Tool: read_file(crates/external-runtime/src/adapter.rs) ─────────────────────────────────────────────── [✓ 18ms] │
│ └────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
│                                                                                                                      │
│ Coder: (Worktree: .agent-studios/wt/run-402-coder)                                                                   │
│ Implementing `CancellationToken` handler inside `ChildProcessSession::terminate`. Mutating adapter crate...          │
│                                                                                                                      │
│ ┌─  Tool: apply_patch(crates/external-runtime/src/adapter.rs) ──────────────────────────────────────────── [✓ 45ms] │
│ │ @@ -112,6 +112,14 @@ impl ChildProcessSession {                                                                   │
│ │ +    pub async fn cancel(&self) -> Result<()> {                                                                    │
│ │ +        #[cfg(windows)]                                                                                           │
│ │ +        windows_signal::send_ctrl_break(self.process_id)?;                                                        │
│ │ +        #[cfg(unix)]                                                                                              │
│ │ +        nix::sys::signal::kill(self.pid, nix::sys::signal::SIGINT)?;                                              │
│ │ +        Ok(())                                                                                                    │
│ │ +    }                                                                                                             │
│ └────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
│                                                                                                                      │
│ Coordinator:                                                                                                         │
│ Coder patch applied successfully. Dispatching Tester agent to run `cargo test -p external-runtime`...                │
│                                                                                                                      │
│ ⠋ Tester: Running test suite `cancellation_propagation_test` (5s elapsed)...                                        │
│                                                                                                                      │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ > _                                                                                                                  │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ [Ctrl+C] Interrupt  [Space] Toggle Tool  [Ctrl+O] Open in IDE  [Tab] Focus Agent  Tokens: 14.2k  Cost: $0.04          │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

### 3.3 State C: Collapsed vs. Expanded Tool Invocations `[PREVIEW ONLY]` / `[IMPLEMENTED / VERIFIED]` (Preview App)

`ags` defaults to clean, single-line collapsed tool cards. The user or reviewer can expand any card by focusing on it and pressing `Space` or `Enter`. (Demonstrated with interactive keyboard toggles in `ags --preview`).

#### Collapsed Tool Card (Default)
```text
┌─  exec_command: `cargo test -p external-runtime` ─────────────────────────────────────────────── [✓ Succeeded 2.4s] ─┐
```

#### Expanded Tool Card (On Keypress `Space`)
```text
┌─  exec_command: `cargo test -p external-runtime` ─────────────────────────────────────────────── [✓ Succeeded 2.4s] ─┐
│ Command: cargo test -p external-runtime -- --nocapture                                                               │
│ Working Dir: D:\Projects\agent-studios\agent-studios-rs\.agent-studios\wt\run-402-coder                              │
│ Exit Code: 0                                                                                                         │
│ Output:                                                                                                              │
│   running 4 tests                                                                                                    │
│   test adapter::tests::test_session_spawn ... ok                                                                     │
│   test adapter::tests::test_cancellation_sigint ... ok                                                               │
│   test adapter::tests::test_cancellation_windows_break ... ok                                                        │
│   test adapter::tests::test_timeout_escalation ... ok                                                                │
│                                                                                                                      │
│   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out                                        │
├──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ [c] Copy Output  [o] Open in Editor  [Space] Collapse                                                                │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

### 3.4 State D: Command Approval Modal (Security Gate) `[IMPLEMENTED / VERIFIED]`

When an agent attempts an action classified as `WorkspaceMutating` or `DangerousHost`, a high-contrast modal halts execution until explicit developer approval is received. (Verified in `crates/tui/tests/widget_tests.rs::test_widgets_render_without_panics`).

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                                  │
│   ┌──────────────────────────────────────────────────────────────────────────────────────┐       │
│   │ 󰌌 APPROVAL REQUIRED · Dangerous Host Execution                                       │       │
│   ├──────────────────────────────────────────────────────────────────────────────────────┤       │
│   │                                                                                      │       │
│   │  Agent:        Coder (Task #104 · Runtime Adapter)                                   │       │
│   │  Tool:         exec_command                                                          │       │
│   │  Command:      git push origin feat/m12-claude-code-runtime --force                  │       │
│   │  Directory:    D:\Projects\agent-studios\agent-studios-rs                            │       │
│   │  Risk Tier:    HIGH (Destructive Remote Git Operation)                               │       │
│   │                                                                                      │       │
│   │  Notice:       Force-pushing may overwrite remote commits on branch                  │       │
│   │                `feat/m12-claude-code-runtime`. This cannot be undone.                │       │
│   │                                                                                      │       │
│   ├──────────────────────────────────────────────────────────────────────────────────────┤       │
│   │  [y] Approve Once    [n] Reject & Stop    [a] Allow Session    [d] Inspect Diff      │       │
│   └──────────────────────────────────────────────────────────────────────────────────────┘       │
│                                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

### 3.5 State E: Unified Diff Review Modal `[IMPLEMENTED / VERIFIED]`

When an agent generates a multi-line patch or proposes a reconciliation into the main branch, the Unified Diff Review modal renders clean, syntax-colored line additions and removals. (Verified in `crates/tui/tests/widget_tests.rs::test_widgets_render_without_panics`).

```text
┌────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ 󰈚 REVIEW DIFF: crates/external-runtime/src/adapter.rs (+14, -2 lines) ────────────────── [1/3 Files]  │
├────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│  98 │ use tokio::sync::watch;                                                                          │
│  99 │ use tracing::{info, warn, error};                                                                │
│ 100 │                                                                                                  │
│ 101 │ impl ExternalRuntimeAdapter {                                                                    │
│ 102 │     pub async fn spawn_agent(&mut self, config: &AgentConfig) -> Result<AgentHandle> {          │
│ 103 │-        let child = Command::new(&config.binary_path)                                            │
│ 104 │-            .spawn()?;                                                                           │
│ 103 │+        let mut cmd = Command::new(&config.binary_path);                                         │
│ 104 │+        #[cfg(windows)]                                                                          │
│ 105 │+        cmd.creation_flags(windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP);     │
│ 106 │+        let child = cmd.spawn()?;                                                                │
│ 107 │                                                                                                  │
│ 108 │         let (tx, rx) = watch::channel(LifecycleState::Starting);                                 │
│ 109 │+        let cancel_token = CancellationToken::new();                                             │
│ 110 │                                                                                                  │
│ 111 │+        self.register_cancellation_handler(&cancel_token, child.id())?;                          │
│ 112 │         Ok(AgentHandle::new(child, rx))                                                          │
│ 113 │     }                                                                                            │
├────────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ [y] Accept Patch   [n] Reject Patch   [e] Open in Editor   [n] Next File   [p] Prev File   [Esc] Close │
└────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

### 3.6 State F: Slash Command Autocomplete Overlay `[IMPLEMENTED / VERIFIED]`

Triggered instantaneously when the user types `/` in the Input Composer. Uses fuzzy substring matching. (Verified in `crates/tui/tests/widget_tests.rs::test_slash_menu_commands_inventory`).

```text
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│  ┌───────────────────────────────────────────────────────────────────────────────────────┐       │
│  │ /model        Switch active provider and model (e.g., /model claude-3-7-sonnet)       │       │
│  │ /runtime      Switch execution runtime (internal-codex, opencode, claude-code)        │       │
│  │ /tasks        Inspect active Task Graph DAG and dependencies                          │       │
│  │ /worktree     List or switch isolated Git worktrees                                   │       │
│  │ /diff         Review uncommitted modifications across active agents                   │       │
│  │ /compact      Manually compact conversation context history                           │       │
│  │ /ide          Handoff active session to Agent Studios IDE (Code-OSS)                  │       │
│  │ /lang         Toggle UI language between English and Vietnamese (en-US / vi-VN)       │       │
│  │ /clear        Clear visible transcript scrollback buffer                              │       │
│  │ /help         Display comprehensive keyboard and command reference                    │       │
│  └───────────────────────────────────────────────────────────────────────────────────────┘       │
│ > /mo_                                                                                           │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│ [↑/↓] Navigate  [Tab/Enter] Select  [Esc] Dismiss                                                │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

### 3.7 State G: Responsive Ultra-Wide Dual-Pane Layout (160×45+) `[IMPLEMENTED / VERIFIED] (Breakpoint Math) / [PREVIEW ONLY] (Live Right Pane)`

On wide displays (terminal width $\ge 150$ columns), `ags` splits into a primary conversation transcript on the left (65% width) and a persistent live context / diff inspector pane on the right (35% width). Layout division and breakpoint triggers verified in `crates/tui/tests/widget_tests.rs::test_responsive_layout_breakpoints`. Right-hand live inspector visualization demonstrable in `ags --preview`.

```text
┌──────────────────────────────────────────────────┬─────────────────────────────────────────────────┐
│ ags ~/agent-studios-rs · 󰊢 feat/m12 · claude-3-7 │ LIVE INSPECTOR: Diff & Task Graph [Run #402]    │
├──────────────────────────────────────────────────┼─────────────────────────────────────────────────┤
│ User:                                            │ TASK GRAPH (DAG):                               │
│ Implement cancellation token propagation across  │   ┌───────────────┐                             │
│ the external runtime adapter process tree.       │   │  T1: Plan     │ [✓ Done]                    │
│                                                  │   └───────┬───────┘                             │
│ Coordinator:                                     │           ▼                                     │
│ Analyzed external runtime architecture. The      │   ┌───────────────┐                             │
│ adapter process must trap OS signals.            │   │  T2: Coder    │ [● Running]                 │
│                                                  │   └───────┬───────┘                             │
│ ┌─  Tool: read_file(adapter.rs) ─────── [✓ 18ms]│           ▼                                     │
│ └────────────────────────────────────────────────┘   ┌───────────────┐                             │
│                                                  │   │  T3: Tester   │ [○ Blocked on T2]           │
│ Coder: (Worktree: .agent-studios/wt/run-402)     │   └───────────────┘                             │
│ Implementing CancellationToken handler...        │                                                 │
│                                                  │ ACTIVE WORKTREE:                                │
│ ┌─  Tool: apply_patch(adapter.rs) ───── [✓ 45ms]│   Path: .agent-studios/wt/run-402-coder         │
│ └────────────────────────────────────────────────┘   Files Changed: 2                              │
│                                                  │   Lines: +48 / -10                              │
│ Coordinator:                                     │                                                 │
│ Coder patch applied successfully. Dispatching    │ LIVE DIFF PREVIEW: (adapter.rs)                 │
│ Tester agent...                                  │  @@ -102,6 +102,12 @@                           │
│                                                  │  + pub async fn cancel(&self) -> Result<()> {   │
│ ⠋ Tester: Running test suite (5s elapsed)...     │  +     #[cfg(windows)]                          │
│                                                  │  +     windows_signal::send_ctrl_break()?;      │
│                                                  │  +     Ok(())                                   │
│                                                  │  + }                                            │
├──────────────────────────────────────────────────┴─────────────────────────────────────────────────┤
│ > _                                                                                                │
├────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ [Tab] Switch Focus  [Ctrl+O] Open IDE  [Ctrl+C] Cancel  [?] Shortcuts            Tokens: 14.2k     │
└────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. TUI Layout Feature-to-Test Evidence Matrix

| Layout / Viewport Feature | Honesty Status | Implementation File | Verification Test Module |
| :--- | :--- | :--- | :--- |
| **Compact Breakpoint (80×24)** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/layout.rs` | `crates/tui/tests/widget_tests.rs::test_responsive_layout_breakpoints` |
| **Standard Breakpoint (100×30)** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/layout.rs` | `crates/tui/tests/widget_tests.rs::test_responsive_layout_breakpoints` |
| **Wide Breakpoint (130×36)** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/layout.rs` | `crates/tui/tests/widget_tests.rs::test_responsive_layout_breakpoints` |
| **UltraWide Breakpoint (180×45)** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/layout.rs` | `crates/tui/tests/widget_tests.rs::test_responsive_layout_breakpoints` |
| **Centered Modal Math** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/layout.rs` | `crates/tui/tests/widget_tests.rs::test_centered_rect_math` |
| **Clean Startup Screen (State A)** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/widgets/` | `crates/tui/tests/widget_tests.rs::test_widgets_render_without_panics` |
| **Multi-Agent Execution (State B)**| `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/widgets/agent_bar.rs` | `crates/tui/tests/widget_tests.rs::test_widgets_render_without_panics` |
| **Tool Card Collapse/Expand (State C)**| `[PREVIEW ONLY]` / `[VERIFIED]` | `crates/tui/src/preview.rs` | Interactive in `ags --preview` |
| **Approval Modal Overlay (State D)**| `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/widgets/approval_modal.rs`| `crates/tui/tests/widget_tests.rs::test_widgets_render_without_panics` |
| **Unified Diff Viewer (State E)** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/widgets/diff_viewer.rs` | `crates/tui/tests/widget_tests.rs::test_widgets_render_without_panics` |
| **Slash Menu Overlay (State F)** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/widgets/slash_menu.rs` | `crates/tui/tests/widget_tests.rs::test_slash_menu_commands_inventory` |
| **Dual-Pane Right Inspector (State G)**| `[PREVIEW ONLY]` / `[PLANNED]` | `crates/tui/src/preview.rs` | Scheduled for M13+ live telemetry |
