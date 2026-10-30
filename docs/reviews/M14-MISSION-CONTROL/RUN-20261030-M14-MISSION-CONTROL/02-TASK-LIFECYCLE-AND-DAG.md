# Task Lifecycle & Interactive DAG Engine: M14 Mission Control

**Milestone:** M14 Mission Control Workbench  
**Run ID:** RUN-20261030-M14-MISSION-CONTROL  
**Date:** 2026-10-30  

---

## 1. Verified Task Lifecycle State Model

The Mission Control workbench maps all task execution states into a strict 7-state finite state machine (with an explicit `ready` state for runnable unassigned tasks):

```
       ┌───────────┐
       │  Pending  │
       └─────┬─────┘
             │ Dependencies Met
             ▼
       ┌───────────┐
       │   Ready   │◄──────────────┐
       └─────┬─────┘               │
             │ Agent Assigned      │
             ▼                     │
       ┌───────────┐         ┌─────┴─────┐
  ┌───►│  Running  │────────►│  Blocked  │ (Waiting on approval or lock)
  │    └─────┬─────┘         └───────────┘
  │          │
  │          ├────────────────────────┐
  │          ▼                        ▼
  │    ┌───────────┐            ┌───────────┐
  │    │  Review   │            │  Failed   │
  │    └─────┬─────┘            └───────────┘
  │          │ Review Approved        ▲
  │          ▼                        │
  │    ┌───────────┐                  │
  └───-│ Succeeded │──────────────────┘
       └───────────┘
```

| Lifecycle State | Description | UI Badge Token | Action / Next Step |
| :--- | :--- | :--- | :--- |
| `pending` | Waiting for upstream task dependencies to complete. | `var(--vscode-descriptionForeground)` | Queued |
| `ready` | All upstream dependencies met; ready for agent scheduling. | `var(--vscode-charts-blue)` | Eligible for dispatch |
| `running` | Actively being executed by assigned agent process. | `var(--status-running, #3794ff)` | Live telemetry stream |
| `blocked` | Execution halted waiting for external human approval or resource. | `var(--status-warning, #cca700)` | Surface in Attention Queue |
| `review` | Artifacts produced; awaiting developer inspection/diff review. | `var(--vscode-charts-orange)` | Delegate to Review View |
| `succeeded` | Execution completed successfully; artifacts verified. | `var(--status-success, #4ec9b0)` | Unblocks downstream nodes |
| `failed` | Task execution encountered runtime or evaluation error. | `var(--status-error, #f14c4c)` | Surface in Attention Queue |
| `cancelled` | Execution cancelled by developer or coordinator. | `var(--vscode-disabledForeground)` | Terminal state |

---

## 2. Interactive SVG DAG Graph Engine

### 2.1 Zero-Dependency Topological Layout
The DAG renderer utilizes the shared layout algorithms from `src/webviews/graphLayout.ts`:
- **`normalizeEdges`**: Normalizes upstream and downstream dependency references, filtering dangling IDs.
- **`detectCycle`**: Performs depth-first cycle detection, catching circular dependencies before layout calculation.
- **`computeTopologicalLayout`**: Computes topological depth tiers (ranks) for each task node, assigning `(x, y)` coordinates, node widths (260px), heights (90px), horizontal gaps (70px), and vertical padding.

### 2.2 Cubic Bezier Spline Edge Routing
Edges are rendered as smooth SVG cubic Bezier splines (`<path d="M x1,y1 C cx1,cy1 cx2,cy2 x2,y2" />`):
- Connects the right-center anchor of the upstream node `(x1, y1)` to the left-center anchor of the downstream node `(x2, y2)`.
- Control points `cx1 = x1 + horizontalGap * 0.5` and `cx2 = x2 - horizontalGap * 0.5` produce an elegant horizontal S-curve.
- Native SVG `<marker id="arrowhead">` provides crisp directional arrowheads aligned with native VS Code theme borders.

### 2.3 Interactive Pan & Zoom Controls
- Integrated zoom controls (`zoomDag(0.1)`, `zoomDag(-0.1)`, `resetDagZoom()`) scale the viewport without external libraries.
- Mouse drag panning updates the transform matrix on `<g id="dag-zoom-group">`.
- SVG nodes support click selection (`onclick="selectTask('...')"`), highlighting the active task across both the DAG view and the Kanban board.
