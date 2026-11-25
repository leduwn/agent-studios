# Architectural & Orchestration Decision Log

## DEC-001: Canonical Integration Baseline (`integration/m15-1-joint-validation`)
- **Date**: 2026-11-25
- **Context**: `integration/m15-unified-codebase` (`8525ec228c`) did not yet have PR #33 code (`65397d709d`), PR #31 (`c98a8a3572`), or PR #34 (`f57bc20c74`) merged. `integration/m15-1-joint-validation` (`8040d195a6`) already merged PR #33 parent (`b0696ace58`), PR #31 (`c98a8a3572`), and PR #34 (`f57bc20c74`), missing only 3 commits (`65397d709d`, `8525ec228c`, `bdc26946d1`).
- **Decision**: Merge the 3 remaining commits cleanly into the isolated worktree `D:\Projects\agent-studios-active\m15-1-joint-validation` on branch `integration/m15-1-joint-validation` (`5f7b5f525b7b5a56bccac2ea3cfd149d809e3f6c`) and preserve `D:\Projects\agent-studios` untouched.
- **Provenance**: `[EXISTING_CANONICAL_DECISION]`

## DEC-002: Drive C Build Cache Stabilization Policy
- **Date**: 2026-11-25
- **Context**: Drive `D:` has 25.52 GB free while Drive `C:` has 243.62 GB free.
- **Decision**: Enforce `CARGO_TARGET_DIR = C:\AgentStudiosBuild\worktrees\m15-1-joint-validation` for all Rust builds and tests (`bdc26946d1`).

## DEC-003: Zero Billable Inference Invariant ($0.00)
- **Date**: 2026-11-25
- **Context**: Automated execution must not consume commercial LLM API tokens without explicit operator approval.
- **Decision**: All automated P0, M15.1, M16, M17, M18 test suites run strictly offline/local using real Git worktrees, real OS subprocesses, WireMock/mock stdio endpoints, and `DeterministicLocalRuntime` (`$0.00` spend).

## DEC-004: Zero-Reimplementation Extensibility Strategy (M16)
- **Date**: 2026-11-25
- **Context**: Upstream `codex-rs` provides foundational primitives; Agent Studios needs scoped discovery/enforcement (`~/.agent-studios/`, `<repo>/.agent-studios/`), progressive disclosure, hook execution policies, MCP secret redaction, and worktree-contained `AGENTS.md`/`CLAUDE.md` resolution exposed via `ags` CLI and Control Plane.
- **Decision**: Implement `crates/extensibility` (`agent-studios-extensibility`) aligned strictly with `SKILLS_MCP_PLUGINS.md` and `AGS_CLI_TUI_SPEC.md`, without modifying `codex-rs/`.

## DEC-005: Native Internal Codex Engine & Vault Provisioning in `agsd` (MM05)
- **Date**: 2026-11-26
- **Context**: `v1.1.0` lacked real `CodexThread` execution inside `agsd.exe`, leaving VS Code chat turns routed to non-executing task records and `SecretStorage` API keys disconnected from daemon provider discovery/inference.
- **Decision**: Embed `CodexStudioEngine` and `InternalCodexAgentRuntime` (`codex_runtime.rs`) inside `agent-studios-ipc-transport` with `DaemonSecretVault` (`SetProviderSecret`/`DeleteProviderSecret`), `ExecuteChatTurn`, `InterruptChatTurn`, `GetStudioTranscript`, `ForkStudio` transcript cloning, `ExecuteMultiAgentGoal` worktree DAG execution, and `PermissionProfile::Disabled` (for workspace-write/full-access modes) paired with `ReadOnlyToolGuardContributor` for `read-only` mode.
