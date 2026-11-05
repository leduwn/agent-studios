# M15 UI-06 — Report 03: Agent-First Experience & Welcome Surface UX

## Metadata
- **Milestone**: M15 UI-06: Agent-First Experience, Shared Session Continuity & Real Daemon Integration Acceptance
- **Worktree**: `D:\Projects\agent-studios-active\m15-ui-06`
- **Component**: `ChatHtmlBuilder.ts`, `ChatWebviewViewProvider.ts`
- **Date**: 2026-11-05
- **Status**: **VERIFIED & ACCEPTED**

---

## 1. Design Philosophy: Agent-First Engineering Workbench

Agent Studios adopts an "Agent-First" mental model:
- Rather than an editor with an AI plugin docked on the side, the primary workspace entry point is the conversation-driven **Agent Mode** (`agent-studios.chat`), where engineers formulate high-level intent, review proposals, and supervise execution.
- When detailed code editing, file navigation, or debugging is required, seamless transition to **IDE Mode** provides the full Code-OSS workbench while retaining the active agent session in shared memory.

---

## 2. Rich Welcome Surface Structure

When a conversation container has no active message thread, `ChatHtmlBuilder.renderEmptyState` renders a rich, structured welcome surface:

```
┌────────────────────────────────────────────────────────────────────────┐
│                               ⚡                                      │
│                         Agent Studios                                  │
│               Agent-First AI Engineering Workbench                     │
├───────────────────────────────────┬────────────────────────────────────┤
│ 📁 Active Workspace               │ 🔌 Daemon & Runtime                │
│    Location: agent-studios        │    Status: connected               │
│    [Open in IDE] [Mission Control]│    Runtime: Native Daemon (agsd)   │
│                                   │    Model Catalog: Pending M15.1    │
├───────────────────────────────────┴────────────────────────────────────┤
│ 💾 Session Continuity                                                  │
│    Active Session: sess-7f8a9b... [Restore Session]                    │
├────────────────────────────────────────────────────────────────────────┤
│ Goal Formulation Starters:                                             │
│  [→ Audit repository architecture and verify dependency graph        ] │
│  [→ Diagnose failing integration tests and formulate surgical fix    ] │
│  [→ Explore git branch changes and summarize active worktree diff    ] │
│  [→ Prepare release verification report and quality checklist        ] │
└────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Component Elements
1. **Hero Title & Subtitle**: Clean neutral branding with optional `[Preview Fixture]` pill when operating under FixtureBridge preview mode.
2. **Active Workspace Card**:
   - Displays bound workspace folder name and full path tooltip.
   - Quick action `Open in IDE Mode`: Triggers `agent-studios.openInIde`.
   - Quick action `Mission Control`: Triggers `agent-studios.openMissionControl`.
3. **Daemon & Runtime Card**:
   - Live connection state pill (`connected`, `disconnected`, `degraded`).
   - Selected runtime indicator (`Native Daemon (agsd)`).
   - **Honest Model Catalog Notice**: Discloses that dynamic model discovery is pending backend M15.1, while enumerating currently supported static adapters (`Claude Code`, `Internal Codex`, `Codex`).
4. **Session Continuity Card**:
   - Rendered whenever prior sessions exist in projection or bridge.
   - Presents truncated session ID with full ID tooltip.
   - One-click `Restore Session` action restoring conversation history and tasks.
5. **Goal Formulation Starters**:
   - 4 pre-formulated engineering starter prompts rendered as interactive action chips (`data-action="prompt-suggestion"`).
   - Clicking fills the composer textarea and focuses the input without accidental submission.

---

## 3. Neutral & Monochrome Theming

All visual styles strictly conform to VS Code's native theme token system:
- Backgrounds: `var(--vscode-sideBar-background)`, `var(--vscode-editor-background)`.
- Borders: `var(--vscode-widget-border)`, `var(--vscode-sideBar-border)`.
- Text & Muted Labels: `var(--vscode-foreground)`, `var(--vscode-descriptionForeground)`.
- Interactive Chips: `var(--vscode-badge-background)`, `var(--vscode-badge-foreground)`.
- Buttons: `var(--vscode-button-background)`, `var(--vscode-button-secondaryBackground)`.

---

## 4. Accessibility & Safety
- Full keyboard navigation: every starter chip and action button is a semantic `<button type="button">` with visible focus rings (`var(--vscode-focusBorder)`).
- Screen reader accessibility: container has `role="region"` and `aria-label="Agent Studios Welcome"`.
- Zero billable calls: starter chips populate client composer state only; no network dispatch occurs until operator presses Send or confirms execution.
