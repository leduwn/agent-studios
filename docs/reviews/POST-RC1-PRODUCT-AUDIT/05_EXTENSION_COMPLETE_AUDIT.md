# 05 — Complete VS Code Extension Forensic Audit (`apps/agent-studios-vscode`)

> **Inventory**: `150` `.ts` files, `48,890` lines of code, `version: 0.1.0` in `package.json`, `0` TypeScript errors, `0` ESLint errors (`352` pre-existing warnings).

---

## 1. Audited Surface Coverage

### Present & Verified (`700` Unit, `84` E2E, `25` Real Extension Host GUI Tests)
- **Activity Bar**: Strictly **1 icon** (`agent-studios-activitybar`, title `"Agents"`), complying with the One Activity Bar Icon Rule.
- **Views (`6` TreeViews + `1` WebviewView)**:
  - `agent-studios.chat` (`ChatWebviewViewProvider`: bidirectional postMessage, XSS-sanitized `ChatHtmlBuilder`, draft persistence, runtime readiness builder, onboarding card, capability gating).
  - `agent-studios.session`, `agent-studios.tasks`, `agent-studios.approvals`, `agent-studios.artifacts`, `agent-studios.timeline`, `agent-studios.providers`.
- **Commands (`18` registered in `package.json` & `CommandsRegistry.ts` / `ApprovalCommands.ts` / `ArtifactCommands.ts` / `MissionControlCommands.ts`)**:
  - Workbench & mode transitions: `openMissionControl`, `openInIde`, `switchToAgentMode`, `continueSession`, `openAgents`, `openSession`, `showTaskGraph`, `showApprovals`.
  - IPC lifecycle & diagnostics: `connectDaemon`, `disconnectDaemon`, `toggleFixturePreview`, `refreshViews`.
  - Approvals & Diffs: `approveAction`, `denyAction`, `inspectArtifact`, `openArtifactReview` (uses virtual `agent-studios-diff:` read-only document provider without mutating disk).
  - Basic selection context actions: `explainSelection`, `refactorSelection`, `generateTests` (each calls `bridge.submitEditorAction`, gated on trusted workspace + `connected` bridge).
- **Mission Control**: 9-tab webview panel (`Overview`, `Fleet`, `Tasks`, `Runs`, `Timeline`, `Review & Diff`, `Approvals`, `Providers`, `Diagnostics`) + `TaskGraphWebview` + `SessionDetailWebview`.
- **Localization & Security**: `LocalizationManager.ts` (`en-US` / `vi-VN` key parity, NFC normalization, strict UI vs agent response language decoupling, wired into `extension.ts`), `WorkspaceVerification`, `OperatorConfirmationModal` (single-use HMAC challenge tokens), `ChatSecurityGuards` (prototype pollution & traversal rejection).

---

## 2. Functional Product Gaps Blocking Daily Coding Adoption (Target: Wave 4)

| Area | Current State at RC1 | Missing Daily-Use Capabilities |
|---|---|---|
| **Conversation Lifecycle** | Single active conversation buffer inside `ChatWebviewViewProvider` | Multi-conversation CRUD (`new`, `rename`, `search`, `archive`, token/context usage tracker) |
| **Editor Intelligence** | Context menu commands require manual selection + active daemon | No `vscode.InlineCompletionItemProvider` (single/multi-line completions), no `vscode.CodeActionProvider` quick-fix (`fixDiagnostics`), no selection-aware inline edit diff flow |
| **Context Engine** | Basic manual `ContextAttachment[]` array on `ChatSendRequest` | No repository map builder, git diff / diagnostics / terminal / symbol extractors, content deduplication, or token budgeting |
| **Change Review & Checkpoints** | Whole-artifact virtual `vscode.diff` read-only preview (`DiffReviewManager`) | No hunk-level accept/reject (`acceptHunk` / `rejectHunk`), no workspace checkpoints, no safe one-click patch revert |
| **Provider Management** | Strictly read-only `ProvidersTreeProvider` | No interactive add/edit/remove provider instance (via `SecretReference` only), no model discovery trigger, no connection health probe command |
| **Extensibility & Agents UI** | Only available via CLI (`ags skill/mcp/hooks/instructions`) | No VS Code commands/UI surfaces for Skills, MCP servers, Lifecycle Hooks, or hierarchical `AGENTS.md`/`CLAUDE.md` |
| **Webview Runtime Localization** | `LocalizationManager` wired into `extension.ts` settings | Webview HTML templates (`ChatHtmlBuilder`, Mission Control tabs) still embed hardcoded English labels rather than reading resolved locale bundles |
