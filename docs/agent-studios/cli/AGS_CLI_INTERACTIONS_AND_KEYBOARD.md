# Agent Studios CLI (`ags`) — Interactions, Keyboard Architecture & State Machine

> **Document**: `AGS_CLI_INTERACTIONS_AND_KEYBOARD.md`  
> **Status**: Canonical Interaction Model & Event Specification  
> **Target Subsystem**: `ags` / Event Loop / Keymap Engine / Focus State Machine  
> **Precedence**: Aligned with `AGS_CLI_COMPONENT_INVENTORY.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Interaction Model Overview

`ags` is engineered for keyboard-first efficiency. Every action—from writing prompts and navigating history to inspecting multi-agent DAGs, approving dangerous commands, and reviewing unified diffs—is executable without touching a mouse.

### Core Interaction Invariants
1. **Zero Trapped Focus**: The user can escape any modal, suggestion popup, or inspection pane back to the Input Composer by pressing `Esc`.
2. **Predictable Submission**: `Enter` submits the prompt; `Shift+Enter` (or `Alt+Enter` in legacy terminals) inserts a newline.
3. **Instant Interruption**: `Ctrl+C` immediately sends an asynchronous cancellation signal to the active run via the Control Plane, halting tool execution without terminating the `ags` process.
4. **Clean Terminal Cleanup**: Terminal raw mode and alternate screen are guaranteed to reset cleanly on normal exit, signal termination (`SIGINT`/`SIGTERM`), or panics.

---

## 2. Comprehensive Keyboard Shortcut Map

### 2.1 Global Hotkeys (Active in all non-modal contexts)

| Shortcut | Action Description | Control Plane / Subsystem Effect |
| :--- | :--- | :--- |
| `Ctrl+C` | Interrupt active agent run / Cancel prompt | Emits `CancelRun(active_run_id)`. If idle, clears current input. |
| `Ctrl+D` | Terminate session / Exit CLI | Cleanly drops leases, disconnects session, resets terminal. |
| `Ctrl+O` | Handoff to Agent Studios IDE (Code-OSS) | Invokes `code-oss --session <id>`; opens current shared session. |
| `Ctrl+L` | Clear screen & force full redraw | Invalidates Ratatui render cache and redraws all widgets. |
| `F1` or `?` | Toggle Help Overlay | Displays full hotkey cheat sheet and version info. |
| `Esc` | Universal Dismiss / Focus Return | Closes active modal, clears selection, returns focus to Composer. |

---

### 2.2 Input Composer Mode (Default Focus)

| Shortcut | Action Description | Implementation Notes |
| :--- | :--- | :--- |
| `Enter` | Submit prompt / Execute command | Sends prompt to Coordinator or runs selected slash command. |
| `Shift+Enter` | Insert newline | Expands composer height up to maximum (8 lines). |
| `Alt+Enter` | Insert newline (terminal fallback) | Fallback for terminals lacking CSI u bracketed key support. |
| `Up` | Previous history prompt / Move up | If multiline: moves cursor up; if first line: cycles history. |
| `Down` | Next history prompt / Move down | If multiline: moves cursor down; if last line: cycles history. |
| `Ctrl+A` / `Home` | Move to start of line | Unicode grapheme boundary aligned. |
| `Ctrl+E` / `End` | Move to end of line | Aligns to end of current line. |
| `Ctrl+U` | Delete from cursor to start of line | Truncates grapheme buffer before cursor. |
| `Ctrl+K` | Delete from cursor to end of line | Truncates grapheme buffer after cursor. |
| `Ctrl+W` | Delete word backwards | Word boundary segmented via `unicode-segmentation`. |
| `Tab` | Trigger Autocomplete / Next item | Autocompletes slash command (`/`) or file mention (`@`). |
| `BackTab` | Previous autocomplete suggestion | Cycles backwards in autocomplete popup. |
| `Alt+Up` / `Ctrl+P` | Transfer focus to Transcript | Enables scrollback inspection mode. |

---

### 2.3 Transcript Scrollback & Inspection Mode (Focus: Transcript)

When the developer presses `Alt+Up` or clicks inside the conversation transcript, the viewport enters **Transcript Mode**:

| Shortcut | Action Description | Implementation Notes |
| :--- | :--- | :--- |
| `j` / `Down` | Scroll down 1 line | Increments `scroll_offset`. |
| `k` / `Up` | Scroll up 1 line | Decrements `scroll_offset`. |
| `PageDown` / `Ctrl+F` | Scroll down 10 lines (Page down) | Fast paging through long logs. |
| `PageUp` / `Ctrl+B` | Scroll up 10 lines (Page up) | Fast paging backwards. |
| `G` / `End` | Jump to newest message (Bottom) | Resumes auto-scroll lock. |
| `gg` / `Home` | Jump to oldest message (Top) | Pauses auto-scroll lock. |
| `Tab` / `Shift+Tab` | Jump between tool cards | Focuses next / previous collapsed tool invocation. |
| `Space` / `Enter` | Expand / Collapse focused tool | Toggles full stdout/stderr and JSON arguments view. |
| `c` | Copy focused block | Copies tool output or markdown code block to OS clipboard via `arboard`. |
| `o` | Open file in editor | Launches file referenced by tool in `$EDITOR` or default editor. |
| `/` | Search in transcript | Opens inline search query bar at bottom. |
| `n` / `N` | Next / Previous search match | Jumps viewport to matching transcript item. |
| `i` / `Esc` | Return to Input Composer | Restores focus to text prompt. |

---

### 2.4 Security Approval Modal (Modal: Tool / Command Approval)

When an agent attempts a sensitive operation (`WorkspaceMutating` or `DangerousHost`):

| Shortcut | Action Description | Resulting Action |
| :--- | :--- | :--- |
| `y` or `Enter` | Approve Once | Grants one-time execution permission for this specific call. |
| `n` or `Esc` | Reject & Deny | Denies tool execution; returns `PermissionDenied` to agent. |
| `a` | Always Allow for Session | Whitelists this tool/command pattern for the lifetime of current session. |
| `d` | Inspect Diff / Details | Expands full unified diff preview before deciding. |

---

### 2.5 Unified Diff Review Modal (Modal: Patch Review)

When reviewing code modifications proposed by `apply_patch` or worktree reconciliation:

| Shortcut | Action Description | Resulting Action |
| :--- | :--- | :--- |
| `y` or `Enter` | Accept Patch | Reconciles patch into workspace working tree. |
| `n` | Reject Patch | Discards patch; instructs agent to retry or abort. |
| `e` | Edit in External Editor | Opens modified file in `$EDITOR` before final reconciliation. |
| `]` / `n` | Next file / hunk | Advances viewport to next modified file. |
| `[` / `p` | Previous file / hunk | Returns to previous modified file. |
| `Esc` | Close Diff Viewer | Closes modal without approving or rejecting. |

---

## 3. Focus State Machine

The application state machine governs which widget receives keyboard events and renders active visual borders.

```text
                     ┌─────────────────────────┐
                     │     ComposerFocused     │◄────────────┐
                     │ (Default Input State)   │             │
                     └────────────┬────────────┘             │
                         │        ▲                          │
              Alt+Up /   │        │ Esc / i                  │ Esc / [y/n]
              Ctrl+P     │        │                          │
                         ▼        │                          │
                     ┌────────────┴────────────┐             │
                     │    TranscriptFocused    │             │
                     │  (Scrollback Nav)       │             │
                     └────────────┬────────────┘             │
                                  │                          │
                         Trigger  │ Tool Event               │
                         Modal    ▼                          │
                     ┌─────────────────────────┐             │
                     │      ModalActive        │─────────────┘
                     │ (Approval / Diff / Cmd) │
                     └─────────────────────────┘
```

### Visual Focus Treatment
- **Focused Container**: Border rendered in `border_focus` (Studio Emerald `#3fb950` / Forest Emerald `#1a7f37`), border style bold.
- **Unfocused Container**: Border rendered in `border_base` (Dark Gray `#30363d` / Light Gray `#d0d7de`), border style normal.
- **Dangerous Modal**: Border rendered in `border_danger` (Crimson `#f85149` / Red `#cf222e`) with double-line box glyphs.

---

## 4. Crossterm Event Pipeline & Tokio Runtime Architecture

### 4.1 Event Loop Architecture
`ags` uses a hybrid Tokio async runtime and Crossterm raw mode event loop:

```rust
// Event loop architecture skeleton
pub async fn run_ags_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    mut event_rx: mpsc::Receiver<ControlPlaneEvent>,
    mut key_event_stream: EventStream,
) -> Result<()> {
    let mut render_interval = tokio::time::interval(Duration::from_millis(16)); // ~60 FPS cap
    let mut app_state = AppState::new();

    loop {
        tokio::select! {
            _ = render_interval.tick() => {
                if app_state.needs_redraw {
                    terminal.draw(|frame| app_state.render(frame))?;
                    app_state.needs_redraw = false;
                }
            }
            Some(Ok(event)) = key_event_stream.next() => {
                match event {
                    Event::Key(key) => {
                        app_state.handle_key(key)?;
                        app_state.needs_redraw = true;
                    }
                    Event::Resize(cols, rows) => {
                        app_state.handle_resize(cols, rows);
                        app_state.needs_redraw = true;
                    }
                    Event::Paste(text) => {
                        app_state.handle_paste(text);
                        app_state.needs_redraw = true;
                    }
                    _ => {}
                }
            }
            Some(cp_event) = event_rx.recv() => {
                app_state.apply_control_plane_event(cp_event)?;
                app_state.needs_redraw = true;
            }
        }

        if app_state.should_exit {
            break;
        }
    }

    Ok(())
}
```

### 4.2 Terminal Initialization & Teardown Guard
To ensure terminal sanity under all conditions:

```rust
pub struct TerminalGuard;

impl TerminalGuard {
    pub fn init() -> Result<Self> {
        terminal::enable_raw_mode()?;
        crossterm::execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            cursor::Hide
        )?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::execute!(
            io::stdout(),
            cursor::Show,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}
```
Panic hooks are registered at startup (`std::panic::set_hook`) to guarantee `TerminalGuard::drop` runs before printing panic stack traces.
