# MASTER MISSION 07 — Wave 1 Research & Forensic Root-Cause Audit

**Date**: 2026-11-26  
**Baseline Audited**: `fix/extension-real-execution-and-parity` (`cbd69a9014`, Extension `1.1.1-f87e284915` / `1.1.0`)  
**Scope**: Real-world VS Code AI Coding Assistant UX benchmarking + forensic root-cause audit of operator-reported issues + alignment with `docs/agent-studios/*`.

---

## 1. Benchmark of Real Reference VS Code Extensions

We inspected installed production extensions on this workstation as well as leading OSS VS Code coding assistants:

### 1.1 Installed Extensions Inspected Locally
| Extension | Version | Activity Bar & Views | Primary Interaction Surface | Settings & Auth Pattern |
|---|---|---|---|---|
| **OpenAI Codex** (`openai.chatgpt`) | `26.1120.50957-win32-x64` | Single Activity Bar icon (`Codex`), clean Webview-first panel + editor panel option | Conversation-first thread; subtle header actions; compact bottom composer (mode/model/reasoning/send); inline grouped tool activity & clean diff cards; no debug/IPC telemetry in main pane | Dedicated settings/accounts flow; dev/mock commands omitted or hidden behind non-production conditions |
| **Anthropic Claude Code** (`anthropic.claude-code`) | `2.1.332-win32-x64` | Focused chat/session view without stacked telemetry tree panels | Conversation-first streaming Markdown + inline tool/diff blocks + prompt permissions inside chat cards; zero protocol/crypto badges on composer | Clean model/permission controls; no read-only tree dump blocking workspace |
| **Cline** (`saoudrizwan.claude-dev`, Apache-2.0) | `4.1.27` | Single Activity Bar container (`Cline`) -> 1 WebviewView (`claude-dev.SidebarProvider`), header bar with `+`, `History`, `Settings` | Clean empty state + prompt starters -> conversation stream with collapsible tool calls, inline diffs & explicit `Approve` / `Reject` buttons -> compact composer | Interactive `ApiOptions` settings view (provider dropdown, base URL, masked/write-only API key in `SecretStorage`, model picker + custom model ID, test connection) |

### 1.2 Open-Source Architectural References Analyzed
- **Cline (`cline/cline`)**: `ChatView.tsx`, `ApiOptions.tsx`, interactive API key + base URL + live model list inside extension `SecretStorage` rather than instructing users to hand-edit CLI configs or read-only trees.
- **Continue (`continuedev/continue`)**: Clean composer (`TipTapEditor`, `ModelSelect.tsx`), quick `@` file context attachment, mode pill, uncluttered monochrome VS Code theme tokens (`--vscode-*`).
- **Roo Code (`RooCodeInc/Roo-Code`) & Kilo Code (`Kilo-Org/kilocode`)**: `ModeSelector`, `ModelPicker`, collapsible `ChatRow` tool-call groups, clean inline permission prompts, progressive disclosure for advanced orchestration controls.
- **OpenCode Integrations**: Minimal header, status dot + short status label, quiet code/diff rendering, zero raw IPC paths or cryptographic details exposed to end users.

### 1.3 Synthesized Design Patterns Required for Agent Studios
1. **1 Activity Bar Icon, 1 Default Primary View**: Show only `Chat` cleanly at full height on activation; keep secondary workbench trees optional/off by default (`agentStudios.showWorkbenchTreeViews = false`) and open Mission Control's 9-tab workbench on demand.
2. **Quiet Chrome, Conversation First**: Dedicate >=75% vertical space to the conversation thread; constrain chrome to a 1-row header (`+`, History, Settings, Mission Control) and a 1-line workspace/connection status strip.
3. **9 Clean Connection States in Plain Language**: Replace `ipc://`, `HMAC-SHA256`, `agsd`, `M15.1`, and `Host Modal Gate` jargon with clear human states (`Not configured`, `Starting`, `Connecting`, `Ready`, `Running`, `Waiting for approval`, `Reconnecting`, `Error`, `Disconnected`) plus single actionable CTAs (`Configure Provider`, `Connect`, `Retry`, `Open Folder`).
4. **Unified Composer & Single Send Pipeline**: Keep `Ask` | `Plan` | `Agent`, Model selector, Reasoning selector, Advanced popover (permissions/compatible runtime/follow-up/attachments), and one always-understandable Send/Stop action.
5. **Interactive Provider & Model Settings WebviewPanel**: Replace `Providers (Read-Only)` tree frustration with an actual UI for presets, Base URL, write-only `SecretStorage` API keys synced to `DaemonSecretVault`, live `/models` discovery, custom model IDs, and `Test Connection`.
6. **Progressive Multi-Agent Disclosure**: Single-agent chat stays lightweight; structured Plan & Worker rows appear cleanly only when a run spawns subtasks/multi-agent coordination, with an optional link to Mission Control.

---

## 2. Forensic Root-Cause Audit of Reported Production UX Defects

Every issue directly observed by the operator after installing Agent Studios Extension `v1.1.0` maps to concrete lines in the codebase:

| # | Operator-Observed Defect | Exact Root Cause (File & Symbol/Line) | Fix Plan (Wave) |
|---|---|---|---|
| 1 | `PREVIEW FIXTURE` tag & `Developer Preview Mode Active` exposed to end users | `apps/agent-studios-vscode/package.json` (`agentStudios.previewMode`, `agent-studios.toggleFixturePreview` in menus/palette); `src/chat/OnboardingWorkflow.ts` (`Enable Developer Preview` CTA); `src/extension.ts` + `src/bridge/BridgeManager.ts` importing `FixtureBridge`; `.vscodeignore` only excluding `out/test/**`, shipping `out/bridge/FixtureBridge.js` & `out/mission-control/fixtures/missionControlFixtures.js` inside VSIX | **Wave 2**: Move fixtures under `src/test/fixtures/`, remove all preview/fixture code from production runtime & `package.json`, add `tsconfig.prod.json` + automated VSIX hygiene gate |
| 2 | Engineering/debug card `Execution Target Summary` & `Host Modal Gate (HMAC-SHA256)` permanently rendered above composer | `apps/agent-studios-vscode/src/chat/ChatHtmlBuilder.ts:826-873` unconditionally rendered `.workbench-toolbar` (`Runtime`, `Studio`, `Workspace` dropdowns) and `.execution-target-summary` with `Host Modal Gate (HMAC-SHA256)` & hardcoded `<span id="summaryRuntime">Claude Code (Native CLI)</span>` | **Wave 4**: Delete `Execution Target Summary`, `Host Modal Gate`, and top workbench dropdown bar; replace with 1-line quiet workspace/connection strip & Advanced popover |
| 3 | Nonsensical `Claude Code` runtime paired with DeepSeek / Ollama model | `ChatHtmlBuilder.ts:851` hardcoded `Claude Code (Native CLI)` while `FixtureBridge.ts:361` listed `deepseek-coder:6.7b` under Ollama without compatibility filtering | **Wave 3 & 4**: Add `ChatPresenter.filterCompatibleRuntimes()` & `isRuntimeCompatibleWithProvider()` so `claude-code` only pairs with Anthropic/Claude |
| 4 | `E2E Genuine Verification Studio`, stale `d:/Projects/agent-studios-ide-ui/...`, and 511+ hour timer | `src/test/e2e/RealRustDaemonE2E.test.ts:146-159,312` (and `RealAgentExecutionE2E.test.ts`, `RealAgentIdeContinuityE2E.test.ts`, `RealArtifactIpcE2E.test.ts`, `LiveMissionControlE2E.test.ts`) spawned `agsd` without `--state-dir`, writing `E2E Genuine Verification Studio` directly into user's real `%APPDATA%\AgentStudios\agsd.db`; `FixtureBridge.ts:53,62` hardcoded `agent-studios-ide-ui` and `startedAt: 1716000000000` | **Wave 2 & 3**: Pass `--state-dir tempDir` in ALL E2E daemon spawns + add idempotent `UserStateMigration` (`v2`) & read filter `isKnownTestOrFixtureRecord()` |
| 5 | Broken `▶ Start Task` button silently disabled & split `agent -> submitTask` vs `ask/plan -> sendMessage` | `ChatHtmlBuilder.ts:975,1363` dispatched `submitTask` in `agent` mode and disabled `▶ Start Task` silently when workspace/studio wasn't manually matched | **Wave 6**: Unify client & host routing on `sendPrompt`, auto-bind workspace studio, return clear inline actionable CTAs (`Open Folder`, `Configure Provider`, `Connect`) |
| 6 | Stale M15 dev copy inside empty welcome card | `ChatHtmlBuilder.ts:1936`: `"Dynamic Provider Discovery pending M15.1. Static adapters: Claude Code, Internal Codex, Codex."` | **Wave 4**: Replace with clean conversational empty state & starter prompts |
| 7 | `Providers (Read-Only)` tree & no in-UI API key / Base URL / Discover Models panel | `package.json:68-71` & `OnboardingWorkflow.ts:89-93` told users to use CLI / never enter API keys in webviews | **Wave 5**: Build interactive `ProviderSettingsPanel` WebviewPanel storing secrets write-only in `vscode.SecretStorage` -> `DaemonSecretVault` |
| 8 | 7 stacked views in `Agents` Activity Bar cluttering sidebar | `package.json:41-71` showed `Chat & Assistant`, `Current Session`, `Tasks`, `Approvals`, `Artifacts`, `Timeline & Activity`, `Providers (Read-Only)` simultaneously | **Wave 8**: Keep 1 Activity Bar icon (`Agents`) showing full-height `Chat`; gate auxiliary tree views behind `agentStudios.showWorkbenchTreeViews` (default `false`); Mission Control opens on demand |

---

## 3. Alignment with Original Specifications (`docs/agent-studios/`)

| Original Principle / Spec | MM07 Reconstruction Compliance |
|---|---|
| `PRODUCT_PRINCIPLES.md`: *Useful, Native, Quiet*, *Progressive Disclosure*, *One Activity Bar Icon* | Monochrome VS Code theme tokens, 1 clean Chat view in 1 `Agents` Activity Bar container, Mission Control & secondary trees on demand |
| `AGENT_AND_IDE_EXPERIENCE.md`: *Agent-First, Conversation-First, Session-First* | Clean thread-first UI, New Chat + History switcher in 1-row header, unified Ask/Plan/Agent send path, inline diffs + Open in IDE continuity |
| `MISSION_CONTROL_PRODUCT_SPEC.md`: *9-Tab Authoritative Workbench* | All 9 tabs preserved cleanly for multi-agent supervision on demand without polluting default single-agent sidebar |
| `LOCALIZATION.md`: *Bilingual English & Vietnamese* | Full `en.ts` & `vi.ts` parity for all reconstructed UI controls, states, onboarding steps, and settings |
