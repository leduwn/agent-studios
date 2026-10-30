# Reference Research & Architectural Precedents: Mission Control
**Milestone:** M14 Mission Control Workbench  
**Status:** Approved Architectural Document  
**Date:** 2026-10-30  

---

## 1. Industry Precedents & Multi-Agent Visualization

Modern AI multi-agent software engineering systems present complex observability challenges: coordinating dozens of autonomous subagents across git worktrees, tracking asynchronous task dependency graphs, auditing file diffs, and managing human approvals.

We investigated several leading open-source and proprietary architectures:

### 1.1 VS Code Native WebviewPanels (GitLens, Copilot, Test Explorer)
- **Key Insight:** VS Code WebviewPanels provide a rich HTML/CSS/JS runtime in the editor area, but come with strict constraints:
  - Sandboxed `vscode-webview://` protocol.
  - Strict Content Security Policy (CSP) forbidding unvetted remote scripts and inline eval.
  - Heavy DOM structures cause high memory usage if not carefully virtualized.
  - Background panels can be discarded unless `retainContextWhenHidden: true` is configured or state is cleanly restored via `getState()` / `setState()`.
- **Adopted Pattern:** We utilize `retainContextWhenHidden: true` for responsive tab switching while preserving a serialized state snapshot for clean reconstruction across VS Code restarts.

### 1.2 OrchestratorInc/agent-orchestrator
- **Inspected Components:**
  - `WorkspaceDiffView.tsx`: Displays unified git status (staged/unstaged/modified/added) with quick launch to IDE diff editors.
  - `SessionArtifactsView.tsx`: Groups generated artifacts (patches, documentation, schemas) by task and run.
  - `PRSummaryDisplay.tsx`: Summarizes multi-agent work into a clean checklist for GitHub pull requests.
- **Adopted Pattern:** Clean separation between modified file lists and actual diff inspection. Rather than reinventing a custom diff viewer inside the webview, we trigger the native VS Code `vscode.diff` editor via extension host messages, providing users with the familiar side-by-side editing experience with syntax highlighting and merge controls.

### 1.3 xyflow / React Flow (`xyflow/xyflow`)
- **Key Insight:** Interactive DAG graphs require smooth panning, zooming, node selection, and dependency routing.
- **Architectural Trade-Off:** Bundling heavy React libraries (such as full React Flow + React DOM) into the VS Code extension webview adds ~400 KB of minified JS and incurs a ~250ms cold-start penalty.
- **Agent Studios Design Choice:** We implement a high-performance, lightweight SVG-based DAG graph engine tailored to VS Code CSS tokens. It supports:
  - Infinite pan and zoom with mouse wheel and drag.
  - Directed cubic Bezier curves (`M x1 y1 C cx1 cy1, cx2 cy2, x2 y2`).
  - Automatic topological rank layout (Sugiyama-style layer assignment).
  - Native VS Code theme colors for node states.
  - Initial render time of <15ms with zero external runtime dependencies.

### 1.4 Claude Code Multi-Agent Observability
- **Key Insight:** Claude Code surfaces subagent actions through hierarchical progress lines and phase blocks.
- **Adopted Pattern:** We adapt this into the **Attention Queue** and **Activity Timeline**, highlighting blocked agents, tool calls, and pending human approvals at the top of the workbench.

---

## 2. Technical Synthesis for Agent Studios

| Concern | Typical Antipattern | Agent Studios Architectural Solution |
| :--- | :--- | :--- |
| **Styling** | Hardcoded hex colors / custom themes | Pure VS Code CSS variables (`var(--vscode-editor-background)`, etc.) |
| **Diff Viewing**| Primitive text diff in webview | Invokes native `vscode.diff(original, modified)` in editor group |
| **DAG Graph** | Massive 500KB JS graph bundle | Pure SVG DAG with topological layering & interactive pan/zoom |
| **State Sync** | Competing webview-internal state | Unidirectional data flow from Extension Host via `postMessage` |
| **Security** | Permissive CSP with `'unsafe-inline'` | Strict CSP with cryptographic nonce and explicit resource origins |
| **Performance**| Unbounded DOM event appending | Bounded timeline projection (last 2000 events) with filtering |
