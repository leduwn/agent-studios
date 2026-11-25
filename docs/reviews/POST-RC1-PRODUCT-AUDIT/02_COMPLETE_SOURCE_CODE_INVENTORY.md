# 02 — Complete Source Code Inventory

> **Verified At**: `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c` (`feat/rc2-product-completion`)
> **TODO / FIXME / `unimplemented!` / `todo!` Count**: **`0`** hits across `agent-studios-rs/crates/` and `apps/agent-studios-vscode/src/`

---

## 1. Codebase Ownership Classification

| Layer | Provenance | Package / Crate Count | Measured Size |
|---|---|---|---|
| `agent-studios-rs/crates/*` | **Custom Agent Studios (Apache-2.0)** | `19` crates | `275` `.rs` files / **`109,030` lines** |
| `apps/agent-studios-vscode/src/*` | **Custom Agent Studios (Apache-2.0)** | `1` VS Code extension (`14` src dirs) | `150` `.ts` files / **`48,890` lines** |
| `codex-rs/*` | **Inherited Upstream OpenAI Codex (`d25c114d`) + Patches 001–004** | `162` `Cargo.toml` manifests | Upstream tree (kept strictly patched via 4 neutral seams) |
| `installer/*`, `scripts/packaging/*` | **Custom Release Scripts** | `6` files | Windows NSIS/PS1/JSON/bundlers |

---

## 2. Custom Rust Workspace Crates (`agent-studios-rs/crates/`)

| Crate (`agent-studios-*`) | `.rs` Files | Total Lines | Primary Responsibilities & Entry Points | Completeness Notes |
|---|---:|---:|---|---|
| `protocol` | 13 | 2,006 | Core domain IDs, tasks, events, approvals, artifacts, capabilities | Complete |
| `control-plane` | 9 | 8,446 | SQLite WAL store, `DatabaseLock`, Kahn DAG scheduler, crash recovery | Complete |
| `provider` | 22 | 6,332 | Provider catalog, keyring/env `SecretStore`, WireMock 6-stage `ModelDiscoveryClient` | Complete |
| `codex-bridge` | 7 | 1,185 | Bridge adapters between `agent-studios-provider` and `codex_model_provider_info` | Complete |
| `protocol-adapters` | 29 | 11,497 | OpenAI Chat/Responses, Anthropic Messages, Gemini `generateContent` SSE adapters | Complete (mock SSE tested) |
| `runtime-transport` | 14 | 4,634 | `ModelInferenceBackend` impl bridging Codex core to `protocol-adapters` | Complete |
| `runtime-session` | 7 | 1,948 | `AgentStudiosRuntimeSessionFactory` + `ThreadManager` override wiring | Complete (`7/7` default-stack after Wave 6) |
| `internal-agent` | 25 | 14,085 | In-process multi-agent supervisor, `WorkspacePolicyArbitrator`, budget guards | Complete |
| `orchestration` | 5 | 2,253 | Multi-agent roles, scheduler policies, handoff coordination | Complete |
| `workspace` | 7 | 2,363 | Git worktree provisioning, ephemeral index diff capture, SHA-256 artifact store | Complete |
| `external-runtime` | 14 | 6,396 | `AgentRuntime` trait, tristate capabilities, `RuntimeRegistry`, `SessionEventHub` | Complete |
| `opencode-runtime` | 21 | 5,014 | OpenCode CLI/HTTP adapter + OS containment sandboxing | Complete (local mock/CLI tested) |
| `claude-code-runtime` | 17 | 7,572 | Claude Code CLI adapter, Win32 Job Object, 2PC stdin approvals | Complete (`AGENT_STUDIOS_LIVE_CLI_SMOKE` guarded) |
| `ipc-protocol` | 11 | 3,082 | Versioned length-prefixed frames, HMAC-SHA256 handshake, `SecretRedactor` | Complete |
| `ipc-transport` (`agsd`) | 27 | 18,019 | `agsd` native daemon binary, Named Pipe / UDS server, privilege separation | Complete |
| `cli-client` | 11 | 4,779 | `ConnectedClient` (`IpcClient`), `EmbeddedClient`, `connect_or_fallback`, P0 runtime | Missing live acceptance harness wrapper |
| `extensibility` | 6 | 1,585 | `SkillRegistry`, `McpRegistry`, `HookSpec`/`execute_hook`, `InstructionResolver` | **Gap**: `McpTransportConfig::Http` unsupported (`mcp.rs:260,342`); no `resources`/`prompts` |
| `tui` | 20 | 5,149 | `ratatui` `AgsApp`, `OnboardingApp`, `OfflinePreviewApp`, diff/approval widgets | Complete (connected in Wave 6 `main.rs`) |
| `cli` (`ags`) | 10 | 2,685 | `ags` binary: `doctor`, `provider`, `model`, `run`, `daemon`, `skill|mcp|hooks|instructions` | Complete |
| **`desktop`** *(Missing at RC1)* | 0 | 0 | Standalone `AgentStudios.exe` host shell & daemon supervisor | **MISSING — Adding in Wave 2–3 (`RC2`)** |

---

## 3. VS Code Extension TypeScript Structure (`apps/agent-studios-vscode/src/`)

- `bridge/` (IPC client, `DaemonDiscovery`, `NativeIpcTransport`, `DaemonConnectedBridge`, `FixtureBridge`, `DisconnectedBridge`, `ArtifactClient`, `ExecutionClient`)
- `cards/` (`DiffReviewManager`, approval/diff webview cards) — **Missing hunk accept/reject & checkpoints**
- `chat/` (`ChatWebviewViewProvider`, `ChatHtmlBuilder`, `ChatSecurityGuards`) — **Missing multi-conversation CRUD / rename / search / archive**
- `commands/` (`CommandsRegistry`, `ApprovalCommands`, `ArtifactCommands`) — **Missing provider CRUD & extensibility commands**
- `composer/` (prompt composer helpers)
- `config/` (`CanonicalPaths.ts`)
- `controls/`, `security/` (`OperatorConfirmationModal`, `WorkspaceVerification`), `state/` (`SessionStateProjection`, `TaskExecutionStore`), `views/` (`6` TreeProviders), `webviews/` (`SessionDetailWebview`, `TaskGraphWebview`), `mission-control/` (9 tabs + projection), `i18n/` (`en.ts`, `vi.ts`, `LocalizationManager.ts`), `test/` (`unit/`, `e2e/`, `integration/`).
- **Missing Modules Needed for Daily Coding (`Wave 4`)**: `editor/` (`InlineCompletionItemProvider`, `CodeActionProvider`, inline editing) and `context/` (`ContextEngine` repo map, diff/diagnostics/terminal attachment + token budgeting).
