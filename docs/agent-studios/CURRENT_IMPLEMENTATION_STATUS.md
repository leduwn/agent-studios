# Agent Studios — Current Implementation Status & Quality Baseline

> **Status**: Authoritative Implementation Ground Truth  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Repository Branch**: `integration/m15-unified-codebase`  
> **Unified HEAD SHA**: `661daaa908810689a47ca8aaf24c788843c0e92c`  
> **Verification Date**: November 4, 2026

---

## 1. Executive Summary & Repository Health

`[IMPLEMENTED_BEHAVIOR]`
Agent Studios has completed **Milestone M15 Unified Consolidation**, integrating the disjoint backend IPC daemon, runtime adapters, and Code-OSS VS Code extension into a single cohesive monorepo.

```text
┌────────────────────────────────────────────────────────────────────────┐
│                   UNIFIED AGENT STUDIOS QUALITY GATES                  │
├──────────────────────────────────────┬─────────────────────────────────┤
│ Metric                               │ Verified Outcome                │
├──────────────────────────────────────┼─────────────────────────────────┤
│ Active Rust Workspace Crates         │ 15 crates (Cargo 2024 Edition)  │
│ Rust Compilation & Typecheck         │ 0 errors (cargo check)          │
│ Rust Clippy Linting                  │ 0 warnings (-D warnings)        │
│ Rust Automated Test Suite            │ 339+ passed, 0 failed (28s)     │
│ TypeScript Compilation & Typecheck   │ 0 errors (tsc --noEmit)         │
│ ESLint Code Quality                  │ 0 errors (npm run lint)         │
│ TypeScript Unit Test Suite           │ 657 passed, 0 failed (4s)       │
│ Real Cross-Process Daemon E2E Suite  │ 78 passed, 0 failed (12s)       │
│ Merge Conflicts on Unified Baseline  │ 0 conflicts (clean DAG merge)   │
│ Billable Cloud Token Expenditure     │ $0.00 spent (Invariant Verified)│
└──────────────────────────────────────┴─────────────────────────────────┘
```

---

## 2. Master Rust Crates Inventory (15 Crates)

All 15 crates reside under `agent-studios-rs/crates/` governed by `agent-studios-rs/Cargo.toml`:

| # | Crate Name | Path | Responsibilities | Tests | Clippy Status |
|:---:|---|---|---|:---:|:---:|
| 1 | `agent-studios-protocol` | `crates/protocol` | Domain types, task contracts, run state, artifact manifests, IDs | Unit | Clean |
| 2 | `agent-studios-control-plane` | `crates/control-plane` | Authoritative task engine, SQLite WAL store, crash recovery, state staging | **63** | Clean |
| 3 | `agent-studios-provider` | `crates/provider` | Model catalog, provider registry, credential redaction (`SecretString`) | **18** | Clean |
| 4 | `agent-studios-external-runtime` | `crates/external-runtime` | Generic agent runtime traits (`AgentRuntime`), lifecycle states, `RuntimeRegistry`| **14** | Clean |
| 5 | `agent-studios-opencode-runtime` | `crates/opencode-runtime` | OpenCode interpreter runtime adapter, process spawning, sandbox boundary | **20** | Clean |
| 6 | `agent-studios-claude-code-runtime`| `crates/claude-code-runtime` | Claude Code CLI adapter, Win32 Job Object sandbox (`#[cfg(windows)]`), 2PC approvals | **74** | Clean |
| 7 | `agent-studios-workspace` | `crates/workspace` | Git worktree orchestrator, artifact storage, lossless diff chunking | **26** | Clean |
| 8 | `agent-studios-ipc-protocol` | `crates/ipc-protocol` | Length-prefixed framing (4-byte BE), HMAC-SHA256 signing, anti-replay window | **10** | Clean |
| 9 | `agent-studios-ipc-transport` | `crates/ipc-transport` | Native `agsd` daemon, Named Pipe / Unix socket server, SQLite lock, event replay | **114** | Clean |
| 10 | `agent-studios-codex-bridge` | `crates/codex-bridge` | Compatibility adapter to upstream `codex-rs` components | Integration | Clean |
| 11 | `agent-studios-runtime-transport`| `crates/runtime-transport` | Transport layer abstractions for remote agent connections | Integration | Clean |
| 12 | `agent-studios-runtime-session` | `crates/runtime-session` | Session state machines and client subscriber tracking | Unit | Clean |
| 13 | `agent-studios-internal-agent` | `crates/internal-agent` | Built-in mock test agents for multi-agent simulation | Unit | Clean |
| 14 | `agent-studios-orchestration` | `crates/orchestration` | Multi-agent coordination abstractions, DAG scheduling | Unit | Clean |
| 15 | `agent-studios-protocol-adapters`| `crates/protocol-adapters` | Schema conversions between LLM wire formats (OpenAI, Anthropic, Gemini) | Unit | Clean |

---

## 3. VS Code Extension Implementation (`apps/agent-studios-vscode`)

The native extension host implementation resides under `apps/agent-studios-vscode/src/`:

### 3.1 Subsystem Architecture
1. **IPC Bridge & Transport (`src/bridge/`)**:
   - `NativeIpcTransport.ts`: Native Windows Named Pipe (`\\.\pipe\agent-studios-*`) and Unix Domain Socket client.
   - `RustIpcCodec.ts`: 4-byte big-endian framing encoder/decoder with 4 MiB clamp.
   - `RustHmacSigner.ts`: HMAC-SHA256 handshake signer with 30s timestamp window.
   - `ArtifactClient.ts`: Lossless chunked diff streaming with per-chunk SHA-256 verification.
   - `DaemonConnectedBridge.ts`: Auto-reconnecting live client.
2. **Mission Control Workbench (`src/mission-control/`)**:
   - `MissionControlPanel.ts`: Webview panel lifecycle manager with strict CSP.
   - `DagGraphRenderer.ts`: Zero-dependency SVG topological DAG graph engine.
   - `KanbanBoard.ts`: Task state column board.
   - `AttentionQueue.ts`: Real-time alerts for approvals and blockers.
   - 9 View Tabs: Overview, Tasks, Runs, Timeline, Review, Approvals, Agents, Providers, Diagnostics.
3. **Activity Bar Chat & Security (`src/chat/`, `src/security/`)**:
   - `ChatWebviewViewProvider.ts`: Natural language prompt composer.
   - `OperatorConfirmationModal.ts`: 2PC confirmation modal dialog with cryptographic challenge token.
   - `WorkspaceVerification.ts`: Strict path traversal and symlink escape protection.

---

## 4. Evidence vs. Claims Matrix

| Subsystem / Feature | Implemented | Locally Tested | CI Verified | Real Daemon Verified | Real Model Verified | Production Ready |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| **Control Plane DAG Engine** | YES | YES | YES | YES | NO | NO |
| **Native IPC Transport (`agsd`)** | YES | YES | YES | YES | NO | NO |
| **Durable Replay & SQLite WAL** | YES | YES | YES | YES | NO | NO |
| **Claude Code Runtime Adapter** | YES | YES | YES | YES | NO | NO |
| **Win32 Job Object Sandboxing** | YES | YES | YES | YES | NO | NO |
| **Lossless Diff Streaming** | YES | YES | YES | YES | NO | NO |
| **Mission Control 9-Tab UI** | YES | YES | YES | YES | NO | NO |
| **Two-Phase Commit Approvals** | YES | YES | YES | YES | NO | NO |
| **Live AI Cloud Token Spend** | NO | NO | NO | NO | **NO (GATED)** | NO |
| **VSIX Distribution Package** | YES | YES | YES | YES | NO | **NO** |

*Note*: General end-user distribution and live cloud execution remain deliberately gated until the operator authorizes Draft PR merge and sets production budget limits.
