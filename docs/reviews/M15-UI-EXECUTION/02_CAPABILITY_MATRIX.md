# M15 — Security & Capability Matrix

**Document Reference**: `docs/reviews/M15-UI-EXECUTION/02_CAPABILITY_MATRIX.md`  
**Milestone**: M14 Final Integration Freeze & M15 UI Execution Preparation  
**Auditor**: Principal VS Code Extension Engineer & Security Auditor  
**Date**: 2026-10-30  
**Status**: ACTIVE RUNTIME SPECIFICATION  

---

## 1. Six Authoritative Capability Tiers

To prevent premature claims of end-to-end execution and ensure strict fail-closed safety, the VS Code client defines 6 distinct capability tiers:

| Tier | Name | Backend Status | Client Gating & Behavior |
| :--- | :--- | :--- | :--- |
| **1** | `READ_ONLY_LIVE` | `IMPLEMENTED_AND_TESTED` | Fully enabled. Authenticated queries for health, ping, studios, tasks, runs, providers, models, and monotonic event streaming. |
| **2** | `TASK_SUBMISSION_READY` | `IMPLEMENTED_NOT_E2E_TESTED` | Contract verified. Gated behind explicit studio ID, prompt validation, operator confirmation, and daemon-default model acknowledgment. |
| **3** | `APPROVAL_READY` | `IMPLEMENTED_AND_TESTED` | Fully supported. Two-phase delivery (2PC), session cryptographic origin rebinding, fail-closed on timeout. |
| **4** | `CANCEL_READY` | `IMPLEMENTED_AND_TESTED` | Fully supported. Pre-execution stop gate in daemon with cryptographic origin check. |
| **5** | `ARTIFACT_REVIEW_READY` | `CONTRACT_ONLY` | Gated / Disabled. `ArtifactRecord` exists in protocol, but `capture_changes` is not wired in daemon lifecycle and no `GetDiff` IPC command exists. |
| **6** | `UNSUPPORTED` | `MISSING` | Blocked / Disabled. Features lacking backend infrastructure (per-task model overrides, multi-turn follow-ups, daemon restart persistence). |

---

## 2. Granular Capability Registry

```typescript
export const CAPABILITY_REGISTRY: Record<string, CapabilityDescriptor> = {
  'daemon.health': {
    tier: 'READ_ONLY_LIVE',
    enabled: true,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    requirements: ['Authenticated IPC connection'],
  },
  'studio.list': {
    tier: 'READ_ONLY_LIVE',
    enabled: true,
    rustCrate: 'agent-studios-control-plane',
    prReference: 'PR #19',
    requirements: ['Authenticated IPC connection'],
  },
  'task.list': {
    tier: 'READ_ONLY_LIVE',
    enabled: true,
    rustCrate: 'agent-studios-control-plane',
    prReference: 'PR #19',
    requirements: ['Authenticated IPC connection', 'Explicit active studio ID'],
  },
  'provider.list': {
    tier: 'READ_ONLY_LIVE',
    enabled: true,
    rustCrate: 'agent-studios-provider',
    prReference: 'PR #19',
    requirements: ['Authenticated IPC connection', 'Secret scrubbing'],
  },
  'event.stream': {
    tier: 'READ_ONLY_LIVE',
    enabled: true,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    requirements: ['Monotonic sequence watermark', 'ReplayBuffer backfill'],
  },
  'task.submit': {
    tier: 'TASK_SUBMISSION_READY',
    enabled: false, // Requires explicit user confirmation gate
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    limitation: 'Backend routes exclusively via daemon defaults (AGENT_STUDIOS_DEFAULT_MODEL). Per-task model overrides in BatchTaskSpec are unsupported.',
    requirements: [
      'Explicit studio ID',
      'Operator confirmation for mutating tasks',
      'Daemon default model acknowledgment',
      'Authenticated client credentials',
    ],
  },
  'approval.resolve': {
    tier: 'APPROVAL_READY',
    enabled: true,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    requirements: ['Cryptographic session origin binding', 'Fail-closed on delivery failure'],
  },
  'task.cancel': {
    tier: 'CANCEL_READY',
    enabled: true,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    requirements: ['Cryptographic origin authorization', 'Target task active/queued'],
  },
  'artifact.review': {
    tier: 'ARTIFACT_REVIEW_READY',
    enabled: false,
    rustCrate: 'agent-studios-workspace',
    prReference: 'PR #19',
    limitation: 'ArtifactRecord exists in protocol crate, but capture_changes is not wired in daemon lifecycle and no GetDiff command exists in IpcCommand.',
    requirements: ['Backend PR-M15-03: Wire capture_changes and expose GetTaskDiff over IPC'],
  },
  'model.per_task_override': {
    tier: 'UNSUPPORTED',
    enabled: false,
    rustCrate: 'agent-studios-protocol',
    prReference: 'PR #19',
    limitation: 'BatchTaskSpec lacks model_ref and provider_instance_id. All tasks inherit daemon default model.',
    requirements: ['Backend PR-M15-01: Add model_ref to BatchTaskSpec'],
  },
  'runtime.per_task_adapter': {
    tier: 'UNSUPPORTED',
    enabled: false,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    limitation: 'IpcServerBuilder configures a singleton runtime adapter. Multi-adapter dynamic routing requires backend changes.',
    requirements: ['Backend PR-M15-02: Runtime adapter registry in ServerState'],
  },
  'chat.multi_turn_prompt': {
    tier: 'UNSUPPORTED',
    enabled: false,
    rustCrate: 'agent-studios-ipc-transport',
    prReference: 'PR #19',
    limitation: 'IpcCommand only supports SubmitTask (initial prompt). Follow-up conversational turns lack a SendPrompt command.',
    requirements: ['Backend PR-M15-01: Add IpcCommand::SendPrompt'],
  },
  'daemon.restart_recovery': {
    tier: 'UNSUPPORTED',
    enabled: false,
    rustCrate: 'agent-studios-control-plane',
    prReference: 'PR #19',
    limitation: 'Control Plane exclusively uses InMemoryStore. Daemon restart permanently loses all tasks, runs, and session bindings.',
    requirements: ['Backend PR-M15-04: SQLite persistent store with WAL'],
  },
};
```

---

## 3. Preflight Evaluation & Limitation Enforcement

When a task submission is requested from the UI:
1. **Studio Identity Validation**: Fails closed if `studioId` is null, empty, or whitespace.
2. **Prompt Integrity**: Rejects empty prompts; preserves text, newlines, and indentation verbatim.
3. **Operator Confirmation Gate**: If `confirmedByUser === false`, submission is refused with:
   `"Operator confirmation is required before starting mutating coding tasks in the repository workspace."`
4. **Per-Task Model Limitation Warning**: If the operator selected a specific model (e.g. `claude-3-7-sonnet`) while the backend only routes via daemon defaults (`AGENT_STUDIOS_DEFAULT_MODEL`), the preflight returns an explicit non-blocking warning:
   `"The Rust backend daemon (agsd) currently routes tasks using daemon-level defaults (AGENT_STUDIOS_DEFAULT_MODEL). Per-task model override ('...') is not yet supported in BatchTaskSpec (PR-M15-01). Task will execute under the daemon configured default model."`
   The UI renders this warning prominently rather than pretending the override succeeded.
