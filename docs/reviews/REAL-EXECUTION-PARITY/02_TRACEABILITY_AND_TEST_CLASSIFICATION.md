# MASTER MISSION 05 — Traceability & Test Truthfulness Classification

## 1. End-to-End Traceability (`VS Code UI` -> `agsd IPC` -> `Codex Engine` -> `Inference Endpoint`)

| Journey / Defect Closed | VS Code Entry Point | IPC Command / Event | Rust Implementation | Verification Evidence |
|---|---|---|---|---|
| **F01: Real Turn & Streaming** | `ChatWebviewViewProvider` -> `DaemonConnectedBridge.sendMessage` | `IpcCommand::ExecuteChatTurn`, `runtime_output_delta`, `IpcResponse::ChatTurnCompleted` | `CodexStudioEngine::execute_chat_turn` (`codex_runtime.rs`) + `codex_core::ThreadManager` | `real_codex_ipc_execution_tests::test_02`, `RealCodexExecutionAcceptanceE2E::J3`, Clean VSIX |
| **F02: Vault Auth & 401/200 Discovery** | `ProviderManagementCommands` (`SecretStorage`) -> `setProviderSecret` / `discoverProviderModels` | `IpcCommand::SetProviderSecret`, `UpsertProvider`, `DiscoverModels` | `DaemonSecretVault` + `ModelDiscoveryClient` (`connection.rs`, `server.rs`) | `real_codex_ipc_execution_tests::test_01`, `RealCodexExecutionAcceptanceE2E::J2`, Clean VSIX |
| **F03: Composer Controls & Read-Only Guard** | `ChatComposer` (`mode`, `reasoningEffort`, `permissionMode`, `followUpQueueMode`) | `ExecuteChatTurn` composer fields + `tool_started`/`tool_completed`/`tool_failed` | `apply_composer_config`, `ReadOnlyToolGuardContributor`, `PermissionProfile::{ReadOnly, Disabled}` | `real_codex_ipc_execution_tests::test_03` (`exec_command` blocked under `read-only`) |
| **F04: Transcript Persistence & Fork Cloning** | `switchSession`, `forkSession`, `getSessionTranscript` | `IpcCommand::GetStudioTranscript`, `ForkStudio` | `CodexStudioEngine` `sessions.json` snapshot + SQLite recovery | `real_codex_ipc_execution_tests::test_02`, `RealCodexExecutionAcceptanceE2E::J3` |
| **F05: Native `"codex"` Runtime & Worktree DAG** | `ChatWebviewViewProvider` runtime selector & `executeMultiAgentGoal` | `GetExecutionReadiness` -> `"codex"`, `ExecuteMultiAgentGoal` | `InternalCodexAgentRuntime` + `CoordinatorDecision` + `WorkspaceOrchestrator` + `ControlPlane::create_reconciliation` | `real_codex_ipc_execution_tests::test_04`, `RealCodexExecutionAcceptanceE2E::J1`/`J4` |
| **F06: Context, AGENTS.md & Extensibility** | `ContextEngine`, `$skill` prompt, `.agents/hooks.json` | `ExecuteChatTurn` (`attachments`, `workspace_path`) | `InstructionResolver`, `SkillRegistry`, `McpRegistry`, `execute_phase_hooks` inside `codex_runtime.rs` | `RealCodexExecutionAcceptanceE2E::J3`, Clean VSIX (`AGENTS.md` + `$audit` verified in wire body) |
| **F07: Editor & Supervisor Hardening** | `AgentStudiosInlineCompletionProvider`, `EmbeddedDaemonSupervisor.ensureDaemonRunning` | `--repo` arg, startup mutex, `resolveAgsdBinary(extensionPath)` | `DaemonConfig::default_repo_path` (`agsd.rs`, `daemon.rs`) | `RealCodexExecutionAcceptanceE2E::J4`, Clean VSIX (`AGSD_BIN_PATH` unset) |

## 2. Test Truthfulness Classification

| Layer | Suite | Execution Environment | Billable Spend | Observed Result |
|---|---|---|---|---|
| **Rust Unit + Integration** | `cargo test --workspace -j 1` | Real compiled Rust crates, `wiremock` HTTP/SSE, real Git repos & worktrees | `$0.00` (`GATE-03`) | **121 test suites: 1026 passed, 0 failed, 0 ignored** (`0` warnings in `clippy -- -D warnings`, `cargo fmt --check` passed) |
| **Rust Real Codex IPC** | `real_codex_ipc_execution_tests` | Real `IpcServer` + `IpcClient` + `CodexThread` + `DaemonSecretVault` + `wiremock` | `$0.00` | **4 passed, 0 failed** (`test_01`..`test_04`) |
| **VS Code Unit** | `npm run test:unit` | Node + Mocha + `vscodeMock` | `$0.00` | **706 passed, 0 failed** (3s) |
| **VS Code E2E** | `npm run test:e2e` | Real compiled `agsd.exe` release binary over Windows Named Pipe + `DeterministicInferenceServer` | `$0.00` | **93 passed, 0 failed** (23s, includes Journeys `J1`–`J4`) |
| **VS Code Extension Host** | `npm run test:integration` | Real Electron VS Code `Code.exe` Extension Development Host | `$0.00` | **25 passed, 0 failed** (14s, exit 0) |
| **Clean VSIX Profile GUI** | Isolated `--user-data-dir` & `--extensions-dir` | Installed `agent-studios-vscode-1.1.1-f87e284915-win32-x64.vsix`, **`AGSD_BIN_PATH` unset** | `$0.00` | **PASSED (exit 0)**; bundled `bin/win32-x64/agsd.exe` auto-resolved and executed real Codex turn |
