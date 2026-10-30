# Agent Studios — Milestone M14-UI-01: UX Gap Analysis

## Executive Summary
Milestone M14-UI-00 established the Code-OSS extension foundation with six passive TreeView panels (Session, Tasks, Approvals, Artifacts, Timeline, Providers) and standalone Webview panels. While functionally complete for passive monitoring, this layout required constant context switching and split attention across multiple panels.

Milestone M14-UI-01 shifts Agent Studios to a **chat-first AI coding assistant architecture**, directly drawing from industry-proven interaction paradigms established by Claude Code, Codex CLI, and modern conversational developer environments.

---

## 1. Architectural Evolution: M14-UI-00 vs M14-UI-01

| Dimension | M14-UI-00 (Tree-Centric Foundation) | M14-UI-01 (Chat-First Coding Assistant) |
|---|---|---|
| **Primary Interaction Surface** | 6 separate TreeViews in Activity Bar | Single unified WebviewView (`agent-studios.chat`) |
| **User Initiation** | Editor context menus or Command Palette | Persistent, auto-growing multiline Composer input |
| **Execution Feedback** | Incremental TreeView updates & Timeline log | Live token streaming with inline blinking cursor |
| **Tool Execution Visibility** | Timeline TreeView list item | Structured `ToolCallCard` with duration, status badge, collapsible I/O |
| **Human Approvals** | Approvals TreeView with context item buttons | In-conversation `ApprovalCard` at point of intervention |
| **Diff & Code Inspection** | Dedicated Artifacts TreeView -> virtual diff | `ChangedFilesCard` with +X/-Y summary & one-click diff review |
| **Multi-Agent Coordination** | Separate Tasks TreeView and Task Graph Webview | In-conversation `TaskProgressCard` linking directly to DAG |
| **Mode Selection** | None (implicit full execution) | Explicit Mode Selector: **Ask** (read-only), **Plan** (DAG breakdown), **Agent** (full tool execution) |
| **Context Management** | File path arguments or active editor commands | Context Chips bar (@-files, line-range selections, compiler diagnostics) |
| **Model/Provider Discovery** | Passive read-only TreeView | Integrated header and toolbar dropdown with token window badges |

---

## 2. Industry UX Pattern Alignment

### 2.1 Claude Code UX Patterns
- **Conversational Command Loop**: Developers express intent naturally; the assistant responds with progressive disclosures rather than dumping entire execution trees.
- **In-Conversation Activity Stream**: Tool calls (file reads, edits, bash execution) are rendered inline as structured folding blocks rather than scattered notifications.
- **Fail-Closed Intervention**: Risky terminal operations require explicit confirmation before execution proceeds, keeping the developer in command.
- **Fast Mode Switching**: Read-only queries ("Ask") avoid unnecessary tool spin-up, while "Plan" isolates architecture formulation before executing code changes.

### 2.2 Codex Developer Experience Patterns
- **Diff-Centric Verification**: Code proposals are never applied blindly to disk; they are presented as unified diffs with line-level accounting (+X/-Y) and one-click side-by-side inspection.
- **Compact Monochrome Aesthetics**: Avoids distracting neon palettes; conforms to native editor theme tokens for zero visual fatigue during extended coding sessions.
- **Context Attachment Economy**: Strict bounding on attached files and snippets (20KB / 200 lines max per chip) prevents prompt bloat and context window exhaustion.

---

## 3. Detailed Friction Points Resolved

### 3.1 Cognitive Load from Panel Fragmentation
- **Old (M14-UI-00)**: A developer delegating a task had to observe Session to check connection, watch Timeline for progress, switch to Approvals to unblock a command, and navigate to Artifacts to see generated diffs.
- **New (M14-UI-01)**: All activities are chronologically interwoven in the chat stream. As the agent acts, tool invocations, approval prompts, and generated diffs arrive sequentially in context.

### 3.2 Loss of Conversational History & Draft State
- **Old (M14-UI-00)**: No interactive chat input; prompts could only be passed via single-line input boxes or predefined context menu commands.
- **New (M14-UI-01)**: Fully preserved conversation history with session switching, draft persistence across tab switches, and multi-turn conversational memory.

### 3.3 Ambiguous Agent Execution Authority
- **Old (M14-UI-00)**: All actions were assumed to be full execution.
- **New (M14-UI-01)**: Explicit mode contract (Ask, Plan, Agent) guarantees whether the agent is permitted to edit files or run commands, preventing unintentional codebase mutations.

---

## 4. Retained Secondary Navigation
The 6 TreeViews from M14-UI-00 are retained as secondary, collapsible views beneath the primary Chat Webview:
1. `agent-studios.session`: Quick glance at daemon connection, agent roster, run duration.
2. `agent-studios.tasks`: Complete hierarchical task inventory.
3. `agent-studios.approvals`: Global pending inbox across all running agents.
4. `agent-studios.artifacts`: Central repository of all generated patches and artifacts.
5. `agent-studios.timeline`: Global audit log of raw daemon events.
6. `agent-studios.providers`: Read-only provider configurations and status.

This hybrid approach ensures rapid conversational execution while preserving deep system observability.
