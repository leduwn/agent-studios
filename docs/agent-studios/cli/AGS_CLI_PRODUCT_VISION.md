# Agent Studios CLI (`ags`) — Product Vision & UX Philosophy

> **Document**: `AGS_CLI_PRODUCT_VISION.md`  
> **Status**: Canonical CLI Product Vision & UX Specification  
> **Target Subsystem**: `ags` / `agent-studios-ags-cli` / Ratatui TUI Front-End  
> **Precedence**: Aligned with `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Executive Summary

`ags` (Agent Studios CLI) is the official terminal-native development environment for Agent Studios. It is a high-performance, developer-first command-line tool built in Rust on top of Ratatui and Crossterm, designed to operate both as an autonomous high-velocity coding agent and as the lightweight terminal counterpart to the full Code-OSS-based Agent Studios desktop IDE.

Where typical AI terminal tools oscillate between over-simplistic raw terminal scrolling (losing context, approvals, and multi-agent coordination) and garish, neon-saturated dashboards that mimic science-fiction movie consoles, `ags` defines a third path: **Quiet Precision**.

```
   ┌────────────────────────────────────────────────────────────────────────┐
   │ ags ~/work/auth-svc · feat/oauth-pkce* · anthropic/claude-3-7-sonnet   │
   ├────────────────────────────────────────────────────────────────────────┤
   │                                                                        │
   │  What do you want to build or refactor?                                │
   │                                                                        │
   │  > Implement PKCE validation in token verification route               │
   │                                                                        │
   └────────────────────────────────────────────────────────────────────────┘
```

`ags` is fast, high-contrast, monochrome-led, deterministic, and engineered specifically for software engineers who live in `tmux`, `alacritty`, `wezterm`, or Windows Terminal.

---

## 2. Core Philosophy & Design Tenets

The design of `ags` is governed by seven foundational tenets:

### Tenet 1: Conversation and Code Over Dashboard Circus
Agents exist to inspect, modify, compile, test, and deliver verified code. The primary surface of `ags` is always the active technical dialogue and the resulting code diffs.
- **No permanent multi-panel monitoring grids** consuming half the screen by default.
- **No simulated animated avatar heads or chat bubbles**.
- **No multi-colored decorative ASCII art banners** pushing user context off-screen.
- **Progressive Disclosure**: Detailed orchestration structures (Task Graphs, Worktrees, Tool logs) remain invisible until relevant or explicitly summoned via hotkey or slash command.

### Tenet 2: Quiet Precision (Useful / Native / Monochrome)
Adhering to Agent Studios Product Principle #27:
- The base visual language is built around dark slate and neutral carbon surfaces with high-contrast text.
- Accent colors (Emerald `#2ea043`, Amber `#d29922`, Crimson `#f85149`, Sky `#58a6ff`) are strictly functional tokens indicating execution state, security gates, or git diff lines.
- No flashing cursors outside the active input, no continuous pulse loops when idle, and no neon gradients.

### Tenet 3: Deterministic Under the Hood, Natural at the Surface
While the user interacts through natural language conversation, every action taken by `ags` translates into typed, deterministic state transitions inside the Agent Studios Control Plane:
- The TUI never invents state by parsing its own stdout.
- The TUI is a **pure projection** of the reactive Control Plane event stream.
- Replaying the Control Plane event log produces the identical screen state byte-for-byte.

### Tenet 4: Seamless Workspace & Worktree Isolation
Following Product Principle #5 and #14:
- In multi-agent scenarios, mutating worker agents operate inside dedicated Git worktrees.
- `ags` surfaces worktree locations cleanly without cluttering the screen (`[Coder: .agent-studios/wt/task-102]`).
- The developer always knows which branch and folder are being modified.

### Tenet 5: Zero-Interruption Dual-Mode Mobility
Following Product Principle #18:
- `ags` and the Agent Studios Desktop IDE share the identical `session_id`.
- A developer can start a task in `ags` on the command line, run `ags open-ide` or press `Ctrl+O`, and immediately resume the exact same session inside Code-OSS without restarting agents, losing task graphs, or resetting working memory.

### Tenet 6: First-Class Internationalization (Vietnamese & English)
Following Product Principle #26 and `LOCALIZATION.md`:
- English (`en-US`) and Vietnamese (`vi-VN`) are co-equal first-class interface languages.
- Complete Unicode NFC handling across input composer, status line, and scrollback.
- Vietnamese diacritics (`á, ắ, ặ, ấ, ầ, ổ, ợ, ỹ...`) render with zero character clipping, zero cursor desynchronization, and zero double-width layout breaks.

### Tenet 7: Transparent Security & Execution Boundaries
Following Product Principle #22:
- Destructive commands and file modifications never hide behind generic "Executing..." labels.
- `ags` categorizes all actions into three explicit risk tiers:
  1. `ReadOnly` (inspecting files, grepping, querying git status).
  2. `WorkspaceMutating` (editing files, creating branches, running linters).
  3. `DangerousHost` (network egress, external script execution, rm/drop operations).
- The approval UI highlights dangerous parameters in crisp amber/crimson modals before execution.

---

## 3. Product Differentiation & Competitive Positioning

### 3.1 Comparison Matrix

| Dimension | OpenAI Codex CLI | OpenCode CLI | Agent Studios CLI (`ags`) |
| :--- | :--- | :--- | :--- |
| **Identity & Branding** | OpenAI branded, closed-ecosystem biased | Community project, multiple UI iterations | Independent Agent Studios identity: Studio Minimalist |
| **Provider Support** | OpenAI model endpoints primarily | Multi-provider via configuration | Multi-Provider Instance + Model target routing (`ProviderInstanceId` + `ModelId`) |
| **Multi-Agent Runtime** | Internal single-thread or basic subagents | Independent conversational windows | Deterministic Control Plane DAG, Manager-Worker, isolated worktrees |
| **External Runtimes** | None | Limited plugin adapters | First-class external runtime adapters (OpenCode, Claude Code) |
| **UI Framework** | Upstream Codex TUI (Ratatui fork) | Bubbletea / Go or custom TUI | Optimized Ratatui + Crossterm Rust architecture |
| **Visual Philosophy** | Functional terminal CLI | Modern terminal aesthetics | Quiet Precision, distraction-free, zero neon clutter |
| **IDE Bridge** | VS Code extension companion | Desktop app companion | 1:1 Shared session with Code-OSS Workbench (`Ctrl+O`) |
| **Language Parity** | English only | English predominantly | English & Vietnamese day-one co-equal parity |
| **Windows Support** | Second-class / MSYS / WSL focus | Unix-first | Windows 11 x64 native first-class tier-1, portable core |

### 3.2 Distinct Brand Signature
`ags` introduces an unmistakable visual signature:
- **The Micro-Mark**: `ags` displayed in subtle low-profile reverse video or bracketed emerald text (`[ags]`), never giant 10-line ASCII fonts.
- **The Context Strip**: A single consolidated header row communicating path, git branch, model, and runtime at a glance.
- **The Execution Ribbon**: A compact 1-line horizontal chip container that only appears during active multi-agent runs, showing real-time agent status without taking over the transcript.

---

## 4. User Personas & Primary Workflows

### 4.1 Target Personas

1. **The Fast Terminal Hacker**:
   - Spends 90% of time in Neovim/Alacritty/tmux.
   - Demands instantaneous startup (<50ms), lightning-fast keyboard navigation, and zero mouse requirements.
   - Uses `ags` for surgical refactoring, commit generation, and single-prompt autonomous tasks.

2. **The Senior System Architect / Lead**:
   - Orchestrates multi-agent features across complex repositories.
   - Requires clear visibility into task decomposition, worktree diffs, and verification tests.
   - Demands tight control over dangerous commands and external dependencies.

3. **The Multi-Runtime Engineer**:
   - Leverages Anthropic Claude 3.7 Sonnet for complex architectural reasoning, OpenAI o3 for logic-heavy verification, and local Ollama DeepSeek-R1 for air-gapped internal code.
   - Needs seamless switching between providers and external runtimes without reconfiguring tools.

### 4.2 Core Workflows

#### Workflow A: The Surgical Prompt & Edit
```
$ ags "Add validation for Vietnamese phone numbers (+84) in src/validators.rs"
→ ags launches in cwd
→ Analyzes src/validators.rs
→ Shows collapsed tool: read_file(src/validators.rs)
→ Shows proposal diff modal
→ Developer presses [y] to approve
→ Runs cargo test
→ Summarizes output with commit message suggestion
```

#### Workflow B: The Autonomous Multi-Agent Sprint
```
$ ags
> /task "Refactor auth middleware to async and add JWT blacklist cache"
→ Coordinator breaks request into Task Graph DAG (3 tasks)
→ Coder agent assigned to Task 1 in isolated worktree .agent-studios/wt/auth-async
→ Cache agent assigned to Task 2 in isolated worktree .agent-studios/wt/jwt-cache
→ Multi-agent chip bar shows real-time progress:
  [Coordinator: Routing] [Coder: auth_mw.rs +42 -12] [Cache: redis.rs +88 -0]
→ Coordinator validates outputs, reconciles patches into main branch
→ All tests pass → Ready for final review
```

#### Workflow C: The Seamless IDE Handoff
```
$ ags
> Implementing complex AST transform...
> ags generates multi-file diff across 14 files
> Developer presses Ctrl+O (or types /ide)
→ Agent Studios Code-OSS opens instantly with the active session pre-loaded
→ Three-way merge editor displays patch artifacts
→ Developer finishes manual tweaks in IDE, returns to terminal seamlessly
```

---

## 5. Architectural Quality Attributes

1. **Responsiveness**:
   - Zero input latency. Keypress to screen update must occur within 16ms (60 FPS cap).
   - Event-driven rendering via Crossterm event stream and Tokio async channels.
2. **Terminal Portability**:
   - Fully functional across Windows Terminal (conpty), Alacritty, WezTerm, iTerm2, Kitty, and standard Linux/macOS consoles.
   - Graceful fallback: 24-bit TrueColor $\rightarrow$ 256-color ANSI $\rightarrow$ 16-color ANSI $\rightarrow$ `NO_COLOR` plain monochrome.
3. **Memory Footprint**:
   - Idle resident memory under 25MB.
   - Virtualized transcript list rendering: only visible lines + scroll buffer are drawn; handles 100,000-line session logs without memory leaks or frame drops.
4. **Safety by Default**:
   - Shell commands default to restricted execution unless explicitly approved.
   - Dangerous file writes display unified colored diffs before touching disk.
