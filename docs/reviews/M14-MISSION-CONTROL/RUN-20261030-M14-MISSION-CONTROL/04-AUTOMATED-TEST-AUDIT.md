# Automated Test Audit: M14 Mission Control Workbench

**Milestone:** M14 Mission Control Workbench  
**Run ID:** RUN-20261030-M14-MISSION-CONTROL  
**Date:** 2026-10-30  

---

## 1. Test Suite Execution Summary

The test harness encompasses pure unit tests executing via Mocha under Node.js with high-fidelity `vscodeMock.ts`, alongside real Extension Host integration tests executing via `@vscode/test-electron`.

| Test Suite | Total Tests | Passing | Failing | Execution Time | Pass Rate |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Mission Control Unit Suite** | 20 | 20 | 0 | 18ms | **100%** |
| **Existing Extension Host Unit Suite** | 273 | 273 | 0 | 242ms | **100%** |
| **Total Automated Unit Tests** | **293** | **293** | **0** | **260ms** | **100%** |
| **Extension Host Integration Suite** | 18 | 18 | 0 | -- | **100%** |

---

## 2. Mission Control Unit Test Coverage Breakdown

The dedicated suite in `src/test/unit/missionControl.test.ts` validates all core subsystems:

### 2.1 Models & Fixture State Generation
- `generates authoritative preview state with all required entities`: Asserts presence of 4 fleet agents, 6 tasks across lifecycle states, 4 execution runs, 10 timeline events, 3 provider summaries, and attention items.
- `verifies secret scrubbing`: Asserts `apiKey === undefined` and `secret === undefined` across all provider objects.
- `verifies diagnostics metadata`: Validates endpoint format and monotonically increasing sequence watermark.

### 2.2 State Projection (`MissionControlProjection`)
- `initializes with preview state when bridge is in preview mode`: Asserts projection honors `BridgeManager.isPreviewMode`.
- `updates active tab and fires onDidChangeState`: Verifies tab switching lifecycle and reactive event dispatch.
- `updates selected agent, task, and run IDs`: Verifies drawer selection and state synchronization.
- `refreshes state from bridge on demand`: Asserts on-demand refresh queries bridge.

### 2.3 HTML Shell Assembly (`MissionControlHtmlBuilder`)
- `builds complete HTML document with CSP nonce and all 9 tabs`: Asserts inclusion of nonce, CSS stylesheets, top navigation bar, and container divs for all 9 tabs (`tab-overview`, `tab-fleet`, `tab-tasks`, `tab-runs`, `tab-timeline`, `tab-review`, `tab-approvals`, `tab-providers`, `tab-diagnostics`).

### 2.4 Panel Lifecycle & Message Routing (`MissionControlPanel`)
- `creates or reveals singleton WebviewPanel`: Asserts singleton enforcement and column targeting.
- `resets singleton when disposed`: Verifies `MissionControlPanel.currentPanel` resets to `undefined`.
- `routes switchTab and selection messages from webview`: Asserts dispatch to projection.
- `delegates openDiff message to DiffReviewManager`: Verifies delegation to diff manager.
- `fails closed on approveAction or denyAction requests`: Asserts modal security warning and blocks mutation.

### 2.5 View Components Rendering
- `renders AttentionQueue warning and error severity cards`: Asserts rendering of warning and error attention cards.
- `renders AgentInspector runtime telemetry and tool traces`: Asserts rendering of agent inspector drawer.
- `renders KanbanBoard across lifecycle states`: Asserts Kanban column segregation and drag/select cards.
- `renders DagGraph SVG nodes, Bezier paths, and controls`: Asserts SVG structure, `<marker id="arrowhead">`, and path elements.
- `renders ApprovalsView with fail-closed security banner`: Asserts presence of security banner.
- `renders ReviewView with file artifacts and diff buttons`: Asserts artifact table and diff buttons.
- `renders ProvidersView with secret-scrubbing banner`: Asserts provider list and secret notice.
- `renders DiagnosticsView with IPC endpoint and sequence watermark`: Asserts IPC endpoint and sequence indicators.

---

## 3. Real Extension Host Integration Tests

The integration test suite in `src/test/integration/missionControl.test.ts` verifies behavior in the real VS Code runtime:
1. `registers agent-studios.openMissionControl in VS Code command registry`: Confirms command registration in `vscode.commands.getCommands(true)`.
2. `executes agent-studios.openMissionControl and creates MissionControlPanel singleton`: Tests command invocation and singleton panel instantiation.
3. `reveals existing MissionControlPanel on subsequent open commands`: Tests idempotency and focus behavior.
4. `resets MissionControlPanel singleton on disposal`: Tests panel disposal and garbage collection.
