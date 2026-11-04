# Reference Research & Architectural Precedents: Mission Control
**Milestone:** M14 Mission Control Workbench  
**Status:** Approved Architectural Document  
**Date:** 2026-10-30  

---

## 1. Verified Open-Source Reference Inventory

These paths were inspected as directory entries in the referenced public GitHub repositories on 2026-10-30. Explicit upstream commits must be tracked before copying or adapting code.

| Source (License) | Code to Study | Main Lesson | Usage Decision |
| :--- | :--- | :--- | :--- |
| **Orchestrator.inc**<br>*(Apache-2.0)* | `frontend/src/renderer/components/SessionsBoard.tsx`, `SessionInspector.tsx`, `SessionView.tsx`, `WorkspaceDiffView.tsx`, `PRSummaryDisplay.tsx` | Fleet overview, needs-attention ordering, agent/branch/PR/CI linkage, per-session inspector, diff review | **Highest-priority reference**; adapt concepts and individual components only after license/dependency audit. |
| **Vibe Kanban**<br>*(Apache-2.0)* | `packages/web-core/src/features/kanban/ui`, `packages/web-core/src/pages/workspaces` | Task columns, board filters, workspace-per-task, review stages | Reference for Kanban and review flow; note upstream sunset announcement. |
| **OpenHands**<br>*(MIT)* | `src/routes/conversation.tsx`, `src/routes/planner-tab.tsx`, `src/routes/task-list-tab.tsx`, `src/routes/usage-tab.tsx`, `src/components/conversation-events/chat` | Conversation-to-task navigation, tool events, files, terminal, usage | Study presentation and event grouping, no duplicate backend. |
| **Cline**<br>*(Apache-2.0)* | `apps/vscode/src/core/webview`, `apps/vscode/webview-ui/src/components` | VS Code host ↔ webview patterns, approval, step-oriented tool cards, checkpoints and diff review | Adapt safe extension-host patterns; keep original branding separate. |
| **Continue**<br>*(Apache-2.0)* | `gui/src/components/History`, `gui/src/components/modelSelection`, `gui/src/components/AcceptRejectDiffButtons.tsx` | History, provider/model selection, diff decisions | Prefer ideas/isolated widgets; backend must remain source of provider/model truth. |
| **OpenCode**<br>*(MIT)* | `packages/app/src/session`, `packages/app/src/session/timeline`, `packages/app/src/providers` | Session details, event timeline, provider choice, streaming presentation | Study UX; do not copy runtime architecture over AGS Control Plane. |
| **Kilo Code**<br>*(MIT)* | `packages/kilo-vscode/webview-ui/src/components` | Webview UX patterns, modes, compact agent controls | Reference only unless component dependencies fit existing extension. |
| **XYFlow / React Flow**<br>*(MIT)* | `examples/react/src/examples/CustomNode`, `Layouting`, `Stress` | Zoom/pan DAG, custom nodes, layout, scaling | Candidate direct dependency if React-based graph panel is justified; otherwise create lightweight SVG read-only graph. |
| **VS Code extension samples**<br>*(MIT)* | `webview-sample`, `webview-view-sample`, `tree-view-sample`, `virtual-document-sample` | Native IDE integration, WebviewPanel lifecycle, CSP, virtual documents | First-party reference for VS Code APIs. |
| **Roo Code**<br>*(Apache-2.0; archived)* | `webview-ui/src/components` | Agent modes and earlier chat/workflow UX | Historical design reference only; repository archived May 2026. |

### License Compliance & Governance Rules
- Inspect exact upstream `LICENSE`, `NOTICE`, copyright headers, and transitive dependency licenses before copying.
- Preserve attribution, track upstream commit, identify modified portions, and never copy logos or trademarks.
- Prioritize clean-room adaptation of architectural patterns.
- Do not copy GPL/AGPL source code into this extension without explicit legal and compatibility review.

---

## 2. Technical Synthesis for Agent Studios

| Concern | Typical Antipattern | Agent Studios Architectural Solution |
| :--- | :--- | :--- |
| **Styling** | Hardcoded hex colors / custom themes | Pure VS Code CSS variables (`var(--vscode-editor-background)`, etc.) |
| **Diff Viewing**| Primitive text diff in webview | Invokes native `vscode.diff(original, modified)` via `DiffReviewManager` |
| **DAG Graph** | Massive 500KB JS graph bundle | Zero-dependency pure SVG DAG with topological layering & interactive pan/zoom |
| **State Sync** | Competing webview-internal state | Unidirectional data flow from Extension Host via `postMessage` |
| **Security** | Permissive CSP with `'unsafe-inline'` | Strict CSP with cryptographic nonce and explicit resource origins |
| **Performance**| Unbounded DOM event appending | Bounded timeline projection (last 2000 events) with filtering |
| **Credentials** | Storing API keys in webview state | Extension Host secret scrubbing filter stripping `apiKey` and `secret` |
| **Fail-Closed** | Executing approvals on unverified bridge | Mutations disabled; modal warnings displayed in read-only slice |

---

## 3. Source Validation and Citation URLs

- Orchestrator source and product docs: `https://github.com/OrchestratorInc/agent-orchestrator` ; `https://docs.orchestrator.inc/`
- Vibe Kanban source: `https://github.com/BloopAI/vibe-kanban` (upstream README announces sunset; maintenance evaluated)
- OpenHands source: `https://github.com/OpenHands/OpenHands`
- Cline source: `https://github.com/cline/cline`
- Continue source: `https://github.com/continuedev/continue`
- OpenCode source: `https://github.com/anomalyco/opencode`
- Kilo source: `https://github.com/Kilo-Org/kilocode`
- React Flow/XYFlow: `https://github.com/xyflow/xyflow`
- Official VS Code extension examples: `https://github.com/microsoft/vscode-extension-samples`
- Roo historical reference: `https://github.com/RooCodeInc/Roo-Code` (archived May 2026)
