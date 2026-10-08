# Agent Studios IDE Architecture: Code-OSS Foundation

> **Precedence Note**: This subsystem specification is governed by [`MASTER_VISION.md`](MASTER_VISION.md) and [`PRODUCT_PRINCIPLES.md`](PRODUCT_PRINCIPLES.md). In the event of any discrepancy, `MASTER_VISION.md` is authoritative.

## 1. Architectural Decision: Code-OSS Foundation

Agent Studios adopts **Code-OSS** as its permanent IDE foundation, replacing previous concepts of a custom Tauri + Monaco desktop shell.

### Why Code-OSS?

Building a production-grade code editor from scratch or wrapping Monaco Editor inside a custom shell introduces severe fragility in file management, language servers, terminal emulation, and plugin ecosystems. Code-OSS provides out-of-the-box, industry-standard stability across:
- **Core Editor**: Syntax highlighting, code navigation, multi-cursor, folding, breadcrumbs, minimap.
- **Workbench Subsystems**: File Explorer, global search, Git / Source Control, integrated terminal (PTY), interactive debugger.
- **Language Intelligence**: Language Server Protocol (LSP), diagnostics / Problems view, Output channels.
- **User Customization**: Keybinding system, themes, settings schema, command palette.
- **Ecosystem**: Standard VS Code extension architecture and workspace configuration.

### Implementation Timeline:
- **Current Architecture**: Code-OSS foundation locked as permanent IDE core. No Code-OSS code imported until backend kernel stabilization.
- **Milestone M13**: First maintained Code-OSS source snapshot/fork imported, targeting Windows x64.

---

## 2. Built-in Agent Studios AI Extension

The future Agent Studios Code-OSS distribution will ship with a native, built-in **Agent Studios AI Extension**:

- **Default AI Surface**: Agent Studios is the primary and default AI/chat interface, directly replacing the space typically occupied by GitHub Copilot.
- **Zero Prefix Requirement**: Users interact directly without requiring `@agentstudios` mentions for standard operations.
- **No Copilot Dependency**: The system operates completely autonomously from GitHub Copilot or OpenAI/ChatGPT account requirements.
- **Direct Runtime Routing**: Built-in chat, composer, and inline actions route directly to the local Agent Studios / Codex runtime via high-performance IPC.

---

## 3. Unified Session Architecture: Dual UX on Single Runtime

IDE Mode and Agent Mode are two layout perspectives of the **same underlying runtime session**:

```text
┌─────────────────┐       ┌─────────────────┐
│    IDE Mode     │       │   Agent Mode    │
│ (Code-OSS IDE)  │       │(Task/Agent UX)  │
└────────┬────────┘       └────────┬────────┘
         │                         │
         └───────────┬─────────────┘
                     │
                     ▼
       ┌───────────────────────────┐
       │   Agent Studios Session   │
       │   - Session ID            │
       │   - Working Directory     │
       │   - Tool & Command History│
       │   - Interactive Approvals │
       │   - Task Graphs & Workers │
       │   - Model & Provider State│
       │   - Git Worktree States   │
       └─────────────┬─────────────┘
                     │
                     ▼
       ┌───────────────────────────┐
       │    Codex Runtime Core     │
       └───────────────────────────┘
```

### Full Operational Parity:
IDE Chat is **not** a restricted question-answer chatbot. It has full operational parity with Agent Mode:
- File inspection, creation, and full editing (`apply_patch`, surgical edits)
- Terminal execution, running builds, running tests, and capturing output
- Explicit interactive approval dialogs for commands, edits, and network access
- Diff visualizer and patch staging
- Full access to MCP servers, Skills (`SKILL.md`), Plugins, hooks, and `AGENTS.md`
- Task orchestration, subagents, and Git worktree isolation
- Deterministic session interruption and resumption

Users can toggle between the IDE layout and the Agent layout at any time without losing execution context, restarting the agent loop, or resetting active worktrees.

---

## 4. Visual Direction & Branding Boundaries

### Visual Direction:
- **Monochrome & High Contrast**: Default styling emphasizes clean monochrome (black, white, and neutral grays) for maximum legibility and developer focus.
- **No Gratuitous AI Styling**: Avoids aggressive neon gradients, glowing AI borders, or intrusive animations that detract from coding.
- **Workbench Familiarity**: Preserves standard VS Code workbench layout and Codicon symbols so users feel immediately at home.

### Branding Boundaries:
- **Agent Studios Surfaces**: Only Agent Studios-specific panels (Agent Manager, Task Timeline, Approval HUD, Provider Selector) receive custom iconography.
- **No Proprietary Assets**: Zero proprietary OpenAI Codex desktop branding, logos, or visual assets.
- **No Trademark Infringements**: Zero GitHub Copilot logos or Microsoft Visual Studio Code trademark assets.
- **Icon Design Deferral**: Icon design is intentionally deferred until Milestone M13. No icons are designed during protocol adapter milestones. When M13 begins, icon design will be explicitly scheduled.

---

## 5. Milestone Progression for Code-OSS Integration

- **M13: Code-OSS Integration Foundation**: Maintained source snapshot/fork, Windows x64 build, independent branding, runtime IPC, initial custom iconography pass.
- **M14: Agent Studios Built-in AI Extension**: Integrated chat/composer surface, Codex session bridge, tool execution, approvals, diff visualization.
- **M15: Unified Agent / IDE Workbench**: Dual layout switcher (IDE vs Agent view), shared runtime state, seamless layout toggling without state loss.
- **M16: Full Codex Feature Surface**: Skills, MCP, Plugins, hooks, `AGENTS.md`, worktree isolation, subagents, advanced sandboxing.
- **M17–M19**: Windows packaging (MSI/NSIS), hardening, and stable release.
