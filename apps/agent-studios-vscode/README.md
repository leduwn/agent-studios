# Agent Studios — Code-OSS Native Agents Workbench

[![M14 CI](https://github.com/leduwn/agent-studios/actions/workflows/agent-studios-m14-ui-ci.yml/badge.svg)](https://github.com/leduwn/agent-studios/actions/workflows/agent-studios-m14-ui-ci.yml)

**Agent Studios** is a Windows-first, open-source, VS Code-class AI development environment that pairs the portable capabilities of Codex with a deterministic multi-agent control plane, multi-provider model routing, isolated Git workspaces, and native IDE integration.

This package (`agent-studios-vscode`) provides the official Code-OSS / VS Code native extension foundation for Milestone **M14-UI-00**.

---

## Key Principles

1. **Code-OSS is the Permanent IDE Foundation**: Native integration using standard VS Code APIs (TreeView, WebviewPanel, Diff Editor, Commands, Theme Tokens, Nonce-based CSP).
2. **Exactly ONE Primary Activity Bar Entry**: Adds `Agents` to the Activity Bar without cluttering the workbench.
3. **Session-First and Goal-First**: Displays high-level session status, goals, and active workers by default.
4. **Progressive Orchestration Disclosure**: The interactive DAG (Task Graph) opens only on demand.
5. **Fail-Closed Security & Human-in-the-Loop**: Disconnected states prevent unverified approval execution or silent file modifications. Approvals for high/critical risk actions require explicit modal confirmation.
6. **Virtual Diff Documents**: Inspects generated patch artifacts using read-only virtual documents (`agent-studios-diff://`) in native `vscode.diff` without touching workspace files.

---

## Features

### 1. Activity Bar: `Agents`
The primary Activity Bar container consolidates all agent orchestration:

- **Current Session**: Connection state, workspace, active goal, run status, agent count, elapsed time, and developer preview mode toggle.
- **Tasks**: Hierarchical task tree with status ThemeIcons (`sync~spin`, `check`, `error`, `lock`, `circle-filled`), assigned agents, progress %, and contextual actions.
- **Approvals**: Pending permission inbox with risk badges (`LOW`, `MEDIUM`, `HIGH`, `CRITICAL`), target commands/paths, and inline Approve/Deny actions.
- **Artifacts**: Patch deliverables with SHA-256 identification and one-click native VS Code Diff inspection.
- **Timeline & Activity**: Chronological activity feed showing timestamps, agent assignments, tool executions, and reconciliation events.
- **Providers (Read-Only)**: Real-time summaries of available providers (Anthropic, OpenAI, Ollama, etc.) and model context windows, strictly owned by the backend.

### 2. Interactive Task Graph (DAG)
- Progressive disclosure view accessible via `Agent Studios: Show Task Graph (DAG)`.
- Deterministic topological layout with cycle detection (3-color DFS).
- Interactive pan, zoom, fit-to-view, and node selection with dependency inspector.
- Strict Content Security Policy with cryptographic nonces and theme-aware CSS variables.

### 3. Editor Context Menu Integration
- Right-click selections in trusted workspaces to invoke:
  - *Ask Agent Studios to Explain*
  - *Refactor with Agent Studios*
  - *Generate Unit Tests with Agent Studios*
- Validates selection ranges, checks workspace trust, and validates backend connectivity.

### 4. Developer Fixture Preview Mode
- Built-in preview fixture mode (`agentStudios.previewMode` or via toolbar beaker icon).
- Clearly labeled with `[MOCK / PREVIEW FIXTURE]` banners.
- Deterministic tasks, approvals, artifacts, and timeline events for testing UI behavior without requiring a running daemon.

---

## Extension Commands

| Command ID | Title | Description |
|---|---|---|
| `agent-studios.openAgents` | Open Agents Workbench | Focuses the Agents Activity Bar container |
| `agent-studios.openSession` | Open Session Details | Opens the deep session overview webview |
| `agent-studios.showTaskGraph` | Show Task Graph (DAG) | Opens the interactive DAG visualizer panel |
| `agent-studios.showApprovals` | Show Approvals Inbox | Focuses the Approvals tree view in the sidebar |
| `agent-studios.inspectArtifact` | Inspect Artifact Diff | Opens native diff editor for an artifact patch |
| `agent-studios.approveAction` | Approve Action | Approves a pending action (modal for high risk) |
| `agent-studios.denyAction` | Deny Action | Denies a pending action |
| `agent-studios.connectDaemon` | Connect to Daemon | Connects to the authoritative Agent Studios daemon |
| `agent-studios.disconnectDaemon` | Disconnect from Daemon | Safely disconnects and returns to fail-closed state |
| `agent-studios.toggleFixturePreview` | Toggle Developer Fixture Preview | Toggles between Disconnected and Fixture bridges |
| `agent-studios.refreshViews` | Refresh Views | Manually re-syncs state projection across all views |
| `agent-studios.explainSelection` | Ask Agent Studios to Explain | Sends selected code to the coordinator |
| `agent-studios.refactorSelection` | Refactor with Agent Studios | Requests refactoring plan for selection |
| `agent-studios.generateTests` | Generate Unit Tests with Agent Studios | Requests test generation for selection |

---

## Extension Settings

| Setting | Type | Default | Description |
|---|---|---|---|
| `agentStudios.daemonEndpoint` | `string` | `"ipc://agent-studios.sock"` | Authoritative Agent Studios daemon endpoint |
| `agentStudios.previewMode` | `boolean` | `false` | Enable developer fixture preview mode |
| `agentStudios.logLevel` | `enum` | `"info"` | Log verbosity (`trace`, `debug`, `info`, `warn`, `error`) |

---

## Development & Testing

### Prerequisites
- Node.js >= 20 (tested on Node 22 & 24)
- npm >= 10

### Commands
```bash
# Install dependencies
npm ci

# Compile TypeScript
npm run build

# Watch mode
npm run watch

# Typecheck
npm run typecheck

# Lint
npm run lint

# Run unit tests (in-memory VS Code mock runner)
npm test
```

### Manual Testing in Extension Development Host
1. Open this repository in VS Code or Code-OSS.
2. Open the debug view (`Ctrl+Shift+D` / `Cmd+Shift+D`).
3. Press `F5` to launch the **Extension Development Host**.
4. In the new window, click the **Agents** icon in the Activity Bar.
5. Toggle developer preview mode by clicking the beaker icon in the **Current Session** panel title to load the deterministic fixture dataset.
6. Test DAG inspection, approval decisions, diff viewing, and commands.

---

## Architecture Documentation

See detailed M14 milestone reviews in:
- `docs/reviews/M14-UI-00/RUN-20261030-M14-UI-00/ARCHITECTURE.md`
- `docs/reviews/M14-UI-00/RUN-20261030-M14-UI-00/UI_COMPONENTS.md`
- `docs/reviews/M14-UI-00/RUN-20261030-M14-UI-00/BRIDGE_CONTRACT.md`
- `docs/reviews/M14-UI-00/RUN-20261030-M14-UI-00/TEST_EXECUTION.md`
- `docs/reviews/M14-UI-00/RUN-20261030-M14-UI-00/SECURITY_REVIEW.md`
- `docs/reviews/M14-UI-00/RUN-20261030-M14-UI-00/FINAL_HANDOFF.md`
