# Feature Priority Matrix: Mission Control Workbench
**Milestone:** M14 Mission Control Workbench  
**Status:** Approved Architectural Document  
**Date:** 2026-10-30  

---

## 1. Feature Classification

| ID | Feature Description | Priority | Target Milestone | Rationale & Safety Boundary |
| :--- | :--- | :--- | :--- | :--- |
| **MC-01** | `MissionControlPanel` WebviewPanel Shell & Command | **P0** | M14-MC | Core host entry point; registered in editor area. |
| **MC-02** | Top Navigation Bar & Persistent Tab State | **P0** | M14-MC | Enables rapid switching without DOM destruction. |
| **MC-03** | Overview Dashboard & Attention Queue | **P0** | M14-MC | Primary landing screen summarizing fleet and blockers. |
| **MC-04** | Agents Fleet View & Agent Inspector Drawer | **P0** | M14-MC | Essential for supervising autonomous agent threads. |
| **MC-05** | Task Searchable List View | **P0** | M14-MC | Dense tabular search for tasks across large runs. |
| **MC-06** | Task Kanban Board (Verified Lifecycle States) | **P0** | M14-MC | Visual pipeline tracking without synthetic states. |
| **MC-07** | Interactive SVG DAG Dependency Graph | **P0** | M14-MC | Pan, zoom, node selection, and theme coloring. |
| **MC-08** | Run Inspector with Tool Invocations | **P0** | M14-MC | Detailed diagnostic view for individual runs. |
| **MC-09** | Activity Timeline (Watermarking & Filtering) | **P0** | M14-MC | Comprehensive chronological audit trail. |
| **MC-10** | Review View with Native `vscode.diff` Launch | **P0** | M14-MC | Native diff editor trigger without custom webview diff. |
| **MC-11** | Approval Center (Read-Only / Guarded) | **P0** | M14-MC | Transparent risk presentation; fail-closed buttons. |
| **MC-12** | Model Providers & Usage View | **P0** | M14-MC | Secret-scrubbed overview of active AI models. |
| **MC-13** | Diagnostics & IPC Connectivity View | **P0** | M14-MC | Uptime, watermark, protocol version, error logs. |
| **MC-14** | Chat ↔ Mission Control State Synchronization | **P0** | M14-MC | Mandatory bridge-level bidirectional sync. |
| **MC-15** | Truthful Preview vs. Live Demarcation | **P0** | M14-MC | Visible banner and badges for fixture data. |
| **MC-16** | Full Test Coverage (Unit + Host Integration) | **P0** | M14-MC | Clean automated test gate before release. |
| **MC-17** | Timeline Search & Category Multi-Filter | **P1** | M14-MC | Enhanced usability for large event volumes. |
| **MC-18** | DAG Graph Cycle Detection & Warning | **P1** | M14-MC | Identifies invalid cyclic dependencies visually. |
| **MC-19** | Keyboard Shortcuts for Workbench Navigation | **P1** | M14-MC | Alt+1..9 tab switching for power users. |
| **MC-20** | Export Session Timeline to JSON/Markdown | **P2** | Future | Local file dump for external audits. |
| **MC-21** | Real-Time Token Cost Estimation | **P2** | Future | Requires sourced user pricing configuration. |

---

## 2. Explicitly Forbidden & Out of Scope

1. **Mutating Task States via Drag-and-Drop:** Dragging cards on the Kanban board must not mutate task states in the backend unless authorized RPC endpoints exist.
2. **Mutating Approvals Without Verified Backend Bridge:** In read-only or disconnected states, Approve and Deny actions must remain visibly disabled.
3. **External Electron Application:** Mission Control must live entirely inside the VS Code / Code-OSS Extension Host.
4. **Code-OSS Core Forks:** No modifications to upstream Code-OSS core binaries.
5. **Direct Backend Crates Mutation:** Absolutely zero modifications to `agent-studios-rs/crates/` or `codex-rs/`.
