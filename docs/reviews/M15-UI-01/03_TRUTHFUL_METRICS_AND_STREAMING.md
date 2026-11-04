# M15 UI-01: Truthful Metrics & Live Streaming Architecture

**Document ID**: `M15-UI-01-DOC-03`  
**Milestone**: M15 UI-01 Interactive Agent Execution Workbench  
**Author**: Principal VS Code Extension Engineer  
**Date**: 2026-10-31  
**Status**: Completed & Verified  

---

## 1. Truthful Reporting Invariant

A central invariant of Agent Studios is **strict telemetry honesty**:
- The extension **never fabricates** token counts, cost calculations, duration metrics, or execution progress.
- If the Rust daemon or execution runner has not emitted authoritative telemetry for a task run, the UI displays:
  ```html
  <span class="metrics-unreported" title="Truthful reporting: daemon has not emitted token or cost telemetry for this run.">
    Metrics: Pending daemon telemetry
  </span>
  ```
- Synthetic "success" transitions based on stream disconnects or empty outputs are strictly prohibited. Completion is recognized only when the backend emits an authoritative `task.completed`, `task.failed`, or Serde `CommandResponse`.

---

## 2. Live Streaming Execution Pane

The streaming pane (`#executionStreamingPane`) visualizes real-time execution progress:

### 2.1 State & Elapsed Timer
- **Execution Status Badge (`#executionStatusBadge`)**: Reflects lifecycle states (`IDLE`, `SUBMITTING`, `RUNNING`, `SUCCEEDED`, `FAILED`, `CANCELLED`).
- **Elapsed Timer (`#executionTimer`)**: Formats elapsed seconds as `MM:SS` (e.g., `00:42`), updated monotonically every 1,000ms while a task is running.

### 2.2 Tool Activity Banner (`#activeToolBanner`)
Provides visibility into tool invocations (e.g. bash commands, file edits):
- Animated spinner indicates active execution.
- Tool name (e.g. `bash`, `read_file`).
- Tool status indicator: `(executing)`, `(completed)`, or `(failed)`.
- Error display (`#activeToolError`): Surfaced in error red when a tool execution fails.

### 2.3 Streaming Console Output (`#streamingConsoleOutput`)
- Monospace output buffer maintaining line-by-line delta streaming from daemon IPC chunks.
- Auto-scrolls to bottom as new tokens arrive.
- Clear button (`#clearStreamBtn`) allows resetting console buffer.

### 2.4 Authoritative Metrics Renderer
Located in `ChatHtmlBuilder.renderMetricsHtml`:
```typescript
public static renderMetricsHtml(metrics?: ExecutionMetrics): string {
  if (!metrics || !metrics.isReported) {
    return `
      <span class="metrics-unreported" title="Truthful reporting: daemon has not emitted token or cost telemetry for this run.">
        Metrics: Pending daemon telemetry
      </span>`.trim();
  }
  const items: string[] = [];
  if (metrics.totalTokens !== undefined) {
    items.push(`<span class="metric-item">Tokens: ${metrics.totalTokens.toLocaleString('en-US')}</span>`);
  }
  if (metrics.costUsd !== undefined) {
    items.push(`<span class="metric-item">Cost: $${metrics.costUsd.toFixed(4)}</span>`);
  }
  if (metrics.latencyMs !== undefined) {
    items.push(`<span class="metric-item">Duration: ${(metrics.latencyMs / 1000).toFixed(1)}s</span>`);
  }
  return items.join(' ');
}
```
All numbers are formatted with explicit US locale (`'en-US'`) to guarantee deterministic rendering across international environments.
