# Truthful UI Interactions & Onboarding Workflow Report

**Milestone:** M14-UI-02  
**Run ID:** RUN-20261030-M14-UI-02  
**Date:** 2026-10-30  
**Scope:** UI state truthfulness, onboarding workflow, and credential isolation architecture.

---

## 1. Truthful Interaction Principles

An enterprise developer tool must never deceive the user regarding its operational state, network connectivity, or file system modifications. Milestone M14-UI-02 establishes four foundational truthfulness invariants:

```
┌────────────────────────────────────────────────────────────────────────┐
│                      Truthful Interaction States                       │
├───────────────────┬────────────────────────────────────────────────────┤
│ State             │ UI Manifestation & Guarantee                       │
├───────────────────┼────────────────────────────────────────────────────┤
│ 1. Disconnected   │ Explicit error card; clear daemon endpoint info;   │
│                   │ fail-closed execution; zero phantom completions.   │
├───────────────────┼────────────────────────────────────────────────────┤
│ 2. Preview Mode   │ Unambiguous "Developer Preview" beaker banner;     │
│                   │ explicit note: zero workspace file mutations;      │
│                   │ in-memory safe fixtures only.                      │
├───────────────────┼────────────────────────────────────────────────────┤
│ 3. Run Cancel     │ Immediate visual abort feedback; state transition  │
│                   │ to stopped; no ghost streaming chunks accepted.    │
├───────────────────┼────────────────────────────────────────────────────┤
│ 4. Approvals      │ Side-by-side diff review before execution;         │
│                   │ explicit decision badges (Approved/Denied/Pending).│
└───────────────────┴────────────────────────────────────────────────────┘
```

---

## 2. Onboarding Workflow (`OnboardingWorkflow.ts`)

The onboarding architecture adapts dynamically to the runtime environment without nagging or obstructing workflow:

### A. State-Aware Card Generation
```typescript
export interface OnboardingState {
  isConnected: boolean;
  isPreview: boolean;
  hasSeenOnboarding: boolean;
  daemonEndpoint?: string;
  activeSessionId?: string;
}
```

1. **Disconnected First Run:**
   - Alerts the developer that the Agent Studios native daemon is not reachable at the configured socket/pipe (`ipc://agent-studios.sock`).
   - Offers two immediate, actionable choices:
     - `Enable Developer Preview`: Explore the full UI using safe, local fixture data.
     - `Retry Connection`: Re-attempt IPC connection to the daemon.
   - Clarifies system prerequisites without generic troubleshooting jargon.

2. **Developer Preview Active:**
   - Informs the user that session responses, tool cards, and diff reviews operate on in-memory simulated data.
   - Reassures developers that **no workspace files are modified or overwritten**.
   - Provides a one-click `Connect to Daemon` button to transition to live mode.

3. **Daemon Connected (First Run):**
   - Displays a welcoming banner confirming active session ID and control-plane readiness.
   - Dismissible with a single click, setting `hasSeenOnboarding = true`.

---

## 3. Strict Zero Raw Credential Solicitation Architecture

A critical vulnerability in developer extensions is soliciting model provider API keys (OpenAI, Anthropic, Google) directly in untrusted webviews.

### Threat Model:
- Webview input fields can be subjected to cross-context scripting or extension injection.
- Storing keys in webview `localStorage` or passing them across `postMessage` exposes credentials to memory inspection.

### Architectural Invariant:
**Agent Studios NEVER prompts for, collects, or displays raw API keys in webviews.**

```
┌────────────────────────────────────────────────────────────────────────┐
│                        Credential Architecture                         │
├────────────────────────────┬───────────────────────────────────────────┤
│ Webview Boundary           │ Native Daemon Host Boundary               │
│ (VS Code UI)               │ (Rust IPC Service)                        │
│                            │                                           │
│ ❌ No password inputs      │ ✔ OS Keyring (SecretService / WinCrypt)   │
│ ❌ No apiKey textboxes     │ ✔ Authenticated model provider dispatch   │
│ ❌ No token storage        │ ✔ Environment variable ingestion          │
│ ✔ Informational notice     │ ✔ Zero key exposure to renderer process   │
└────────────────────────────┴───────────────────────────────────────────┘
```

### Automated Verification:
Unit test `onboarding.test.ts` validates that rendered HTML outputs strictly reject:
- `<input type="password"` (Asserted absent)
- `name="apiKey"` (Asserted absent)
- `Security Notice` text (Asserted present)
