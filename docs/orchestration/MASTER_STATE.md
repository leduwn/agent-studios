# Agent Studios — Master Orchestration State (`MASTER_STATE.md`)

> **Last Updated**: 2026-11-25T00:45:00+07:00
> **Active Milestone**: M15.1 → M16 → M17/M18/M19 Execution Loop
> **Active Isolated Worktree**: `D:\Projects\agent-studios-active\m15-1-joint-validation`
> **Active Integration Branch**: `integration/m15-1-joint-validation`
> **Baseline Commit SHA**: `5f7b5f525b7b5a56bccac2ea3cfd149d809e3f6c`
> **Canonical Root (Untouched / Clean Safety Gate)**: `D:\Projects\agent-studios` (`integration/m15-unified-codebase` at `8525ec228c37e38c35ee514dc886af295fbc4e00`)
> **Build Cache Target Dir**: `C:\AgentStudiosBuild\worktrees\m15-1-joint-validation`

---

## 1. Current Integration Status

All active development branches (`PR #33` tip `65397d709d`, `PR #31` tip `c98a8a3572`, `PR #34` tip `f57bc20c74`, `integration/m15-unified-codebase` tip `8525ec228c`, and `chore/dev-build-cache-policy` tip `bdc26946d1`) have been verified via Git ancestry and merged cleanly with 0 conflicts into `integration/m15-1-joint-validation` (`5f7b5f525b7b5a56bccac2ea3cfd149d809e3f6c`).

## 2. Phase 0 Feature Classification Baseline

| Feature / Subsystem | Status | Evidence / Gap Analysis |
|---|---|---|
| Control Plane DAG & Kahn Scheduler (`crates/control-plane`) | `VERIFIED_IMPLEMENTED` | SQLite WAL store, single-writer `DatabaseLock`, replay backfill |
| Provider Discovery v1.2, Importer, SecretStore (`crates/provider`) | `VERIFIED_IMPLEMENTED` | WireMock 6-stage probe, OS/in-memory secret store |
| External Runtime Trait & Registry (`crates/external-runtime`) | `VERIFIED_IMPLEMENTED` | Tristate capabilities, `RuntimeRegistry` |
| OpenCode Runtime Adapter (`crates/opencode-runtime`) | `VERIFIED_IMPLEMENTED` | Process containment, cross-platform sandbox tests |
| Claude Code Runtime Adapter (`crates/claude-code-runtime`) | `VERIFIED_IMPLEMENTED` | Win32 Job Object (`#[cfg(windows)]`), 2PC stdin approval |
| Internal Multi-Agent Supervisor (`crates/internal-agent`) | `VERIFIED_IMPLEMENTED` | Links upstream `codex-rs`, `WorkspacePolicyArbitrator`, budgets |
| Git Worktree Isolation & Lossless Diff (`crates/workspace`) | `VERIFIED_IMPLEMENTED` | Ephemeral `GIT_INDEX_FILE`, SHA-256 chunked diffs |
| Native IPC Framing, HMAC & `agsd` Daemon (`crates/ipc-protocol`, `crates/ipc-transport`) | `VERIFIED_IMPLEMENTED` | Named Pipes & UDS, privilege separation (`agsd.json` / `agsd-admin.json`) |
| VS Code Extension, 9-Tab Mission Control & Mode Continuity (`apps/agent-studios-vscode`) | `VERIFIED_IMPLEMENTED` | Unit + real daemon E2E + extension host tests |
| `ags` CLI (`--help`, `--version`, `doctor [--json]`, `provider`, `model`) | `VERIFIED_IMPLEMENTED` | `crates/cli` |
| `ags provider discover <instance_id>` CLI command | `PARTIALLY_IMPLEMENTED` | `ModelDiscoveryClient::discover_models` in `crates/provider`, adding CLI subcommand in Wave 2 |
| `ags run` (`--headless`, `--stdin`, NDJSON streaming, exit codes 0/1/2/3) | `MISSING` | Implementing in Wave 2 (`crates/cli/src/commands/run.rs`) |
| `ags daemon` (`status`, `health`) CLI command | `MISSING` | Implementing in Wave 2 (`crates/cli/src/commands/daemon.rs`) |
| Daemon-Connected CLI Client (`ConnectedClient` in `crates/cli-client`) | `PARTIALLY_IMPLEMENTED` | `ExecutionMode::Connected` exists; implementing `ConnectedClient` + `connect_or_fallback` in Wave 2 |
| Multi-Runtime Selection in `cli-client` (`ClaudeCodeRuntime` + `OpenCodeRuntime` + `DeterministicLocalRuntime`) | `PARTIALLY_IMPLEMENTED` | Adding `ClaudeCodeRuntime` probing & preference routing in Wave 2 |
| P0 Real Agent Execution Acceptance Harness | `PARTIALLY_IMPLEMENTED` | Implementing end-to-end worktree/command/diff/approval/cancel/reconnect harness in Wave 2 |
| M16 Skills Engine, MCP Config/Policy, Lifecycle Hooks, Hierarchical Instructions | `PLANNED` | Implementing `crates/extensibility` + CLI subcommands in Wave 3 |
| M17/M19 Localization (`en-US` / `vi-VN` parity + decoupling) & Canonical Paths | `PARTIALLY_IMPLEMENTED` | Implementing i18n bundles + `CanonicalPaths.ts` in Wave 4 |
| M17 Windows Standalone Bundle & Installer Scripts | `PLANNED` | Implementing packaging & installer scripts in Wave 4 |
| M18 Concurrency Stress & Security Hardening Tests | `PARTIALLY_IMPLEMENTED` | Implementing 10+ workers / 1k turns / 100k events & race tests in Wave 4 |
| Live Billable Cloud LLM Acceptance ($1–$5 spend) | `BLOCKED` | Requires operator approval gate and live API credentials (`$0.00` gate enforced) |

## 3. Task Execution Summary

- **Completed Tasks**:
  - `W1-T1`: Phase 0 Git topology & documentation audit; merged `65397d709d` and `bdc26946d1` into `integration/m15-1-joint-validation` (`5f7b5f525b7b5a56bccac2ea3cfd149d809e3f6c`).
- **Running / Active Tasks**:
  - `W1-T2`: Author persistent orchestration documents (`docs/orchestration/*`).
- **Remaining Tasks**:
  - `W2-T1`: `ConnectedClient` & multi-runtime selection (`crates/cli-client`)
  - `W2-T2`: `ags run`, `ags daemon`, `ags provider discover` (`crates/cli`)
  - `W2-T3`: P0 Real Execution & CLI acceptance test suites
  - `W3-T1`: `agent-studios-extensibility` crate (Skills, MCP, Hooks, Hierarchical Instructions) + CLI integration
  - `W4-T1`: Frontend i18n (`en-US`/`vi-VN`), language decoupling & `CanonicalPaths`
  - `W4-T2`: M18 Concurrency stress (12 workers, 100k events, 1k+ turns) & security hardening tests
  - `W4-T3`: M17/M19 Windows release bundle, installer scripts & release docs
  - `W5-T1`: Full mandatory quality gates (Rust + TypeScript + Packaging) & final state closure

## 4. Human Approvals Required (Non-Blocking for Offline Technical Work)

1. Merging `integration/m15-unified-codebase` / `integration/m15-1-joint-validation` into protected `main` (`PR #32` + PR `#31/#33/#34`).
2. Destructive cleanup of legacy worktrees on `HOLD` or `APPROVED_CANDIDATE` list.
3. Live billable LLM API spend cap ($1.00–$5.00) and real credentials.
4. Production Authenticode code-signing certificate for Windows installer binaries.

## 5. Exact Next Action

Commit Wave 1 orchestration documents (`W1-T2`), then execute Wave 2 (`W2-T1`, `W2-T2`, `W2-T3`) in parallel / scoped work packages.
