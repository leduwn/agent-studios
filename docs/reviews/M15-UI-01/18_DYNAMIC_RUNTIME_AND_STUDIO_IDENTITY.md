# Milestone M15 UI-01.2 Review: Dynamic Runtime & Studio Identity Audit

**Milestone**: M15 UI-01.2 — Real Rust Execution Integration & Final Safety Gates  
**Area**: Dynamic Runtime / Studio Identity, Elimination of Hardcoded Fallbacks  
**Verdict**: APPROVED  

---

## 1. Executive Summary

Milestone M15 UI-01.2 eliminates all hardcoded production fallbacks for runtime engines and studio sessions:
- `claude_code` enabled by default: REMOVED.
- `default-studio`: REMOVED.
- `agent-studios-core`: REMOVED.

The UI distinguishes between `runtime_preference` (the implementation kind, e.g. `opencode`, `claude_code`, `codex`) and `runtime_instance_id` (a specific daemon-configured runtime instance). Studio identity is derived dynamically from genuine Rust `ListStudios` responses or authorized `CreateStudio` flows.

---

## 2. Dynamic Discovery Architecture

### 2.1 Runtime Preference vs Instance ID

In `CapabilityMatrix.ts`:
- Challenge token verification enforces strict presence/absence equality and exact matching for both `runtimePreference` and `runtimeInstanceId`.
- If the daemon reports `opencode` as the only operational runtime, the UI presents `opencode` and marks other runtimes as unconfigured or unavailable.
- Future runtimes (such as Codex external runner) remain visibly disabled with an explicit M16 indicator.

### 2.2 Studio Session Lifecycle

- In `DaemonConnectedBridge.getCurrentSession()`, `_activeStudioId` is assigned only from the first studio returned by `listStudiosInternal()` or an explicit `switchSession(studioId)` call.
- In `SubmitTaskAdapter.submitTask()`, a missing or empty `studioId` triggers an immediate fail-closed rejection: `Cannot submit task: studioId is required and must not be empty.`

---

## 3. Truthful Webview Population

- Toolbar selectors dynamically populate available runtimes from `readiness.available_runtimes` and `readiness.runtime_instances`.
- Studio dropdown dynamically populates active studios from `bridge.listSessions()`.
- If no studios exist, the workbench prompts the user to create a studio or waits for daemon configuration.
