# Milestone M15 UI-01.1 Review: Dynamic Hydration Audit

**Audited Component**: `ChatWebviewViewProvider` & `ChatHtmlBuilder`  
**Purpose**: Elimination of hardcoded UI selections in favor of authoritative backend daemon data.  

---

## 1. Eliminated Static Selectors

Previously, the workbench UI relied on hardcoded option strings:
- Runtimes: static strings `claude_code`, `codex_external`, `custom_shell`.
- Studios: hardcoded IDs `default-studio`, `agent-studios-core`.

## 2. Dynamic Hydration Implementation

### 2.1 Runtime Hydration from Readiness Report
When resolving the webview or synchronizing state, `ChatWebviewViewProvider` queries `bridge.getExecutionReadiness()`:
- If `readiness.runtime_instances` are present, runtime options are constructed directly from authoritative backend instances, indicating instance ID and health status:
  ```typescript
  runtimeOptions = readiness.runtime_instances.map((inst) => ({
    id: inst.implementation,
    name: `${inst.implementation} (${inst.instance_id})`,
    enabled: inst.healthy,
    tooltip: inst.healthy ? 'Healthy runtime instance' : 'Degraded runtime instance',
  }));
  ```
- If `readiness.available_runtimes` are provided without instance breakdowns, options are mapped from `available_runtimes`.
- Default selection is authoritatively chosen via `readiness.defaults.configured_instance_id` or the primary available runtime.

### 2.2 Studio Hydration from Real Sessions
- The Studio selector is dynamically populated from `bridge.listSessions()`:
  ```typescript
  studioOptions = sessions.map((s) => ({
    id: s.id,
    name: s.title ? `${s.title} (${s.id})` : s.id,
  }));
  ```
- If an active session exists (`bridge.getCurrentSession()`), it is selected by default; if no prior sessions exist, a fallback entry is rendered and clearly attributed.

### 2.3 Incremental Sync Support
When state updates occur, `syncState()` emits the latest `runtimes`, `studios`, `selectedRuntime`, and `selectedStudioId` in the `syncState` webview message. The webview client reconciles the `<select>` options in-place without tearing down DOM elements or losing focus.
