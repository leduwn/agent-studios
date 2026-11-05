# 03 — Rust Crates Integration Specification: M15 Consolidation

**Document Reference**: `docs/reviews/M15-UNIFIED-CONSOLIDATION/03_RUST_CRATES_INTEGRATION_SPEC.md`  
**Milestone**: M15 Consolidation & Workspace Audit  
**Integration Branch**: `integration/m15-unified-codebase`  
**Workspace Path**: `D:\Projects\agent-studios-unified-staging\agent-studios-rs`  
**Auditor**: Lead Integration Engineer & Agent B (Backend Auditor)  

---

## 1. Executive Summary

The unified Agent Studios Rust workspace consolidates 15 core crates under `agent-studios-rs`, integrating the authoritative backend implementation from PR #30 (`cf1e6c0f98`) with the previously unmerged M12 Claude Code runtime adapter from PR #15 (`3207b7462f`).

Key architectural enhancements achieved during consolidation:
1. **Claude Code Runtime Integration**: `agent-studios-claude-code-runtime` added to workspace `Cargo.toml` members.
2. **Critical Linux CI Compilation Fix**: Discovered and resolved an unguarded Windows-only export `pub use sandbox::JobObjectGuard` in `claude-code-runtime/src/lib.rs`. Guarded with `#[cfg(windows)]` to prevent compilation failure on `ubuntu-latest` CI runners.
3. **Dependency Graph Optimization**: Pruned unused dependencies (`agent-studios-provider`, `agent-studios-workspace`, `agent-studios-control-plane`) from `claude-code-runtime/Cargo.toml`, eliminating 3 cycles/redundant edges from the build graph.
4. **Dual-Runtime Execution Support**: The daemon architecture (`agsd` / `IpcServer`) fully supports dynamic task routing between `OpenCodeRuntime` and `ClaudeCodeRuntime` via `TaskExecutionSpec.runtime_preference`.
5. **Durable Replay & Crash Recovery**: Incorporates the complete SQLite-backed durable event store, 2PC approval delivery, and race-free 4-phase subscription handshake verified in M15-05.

---

## 2. Master Rust Crates Inventory (15 Crates)

| # | Crate Name | Directory Path | Version | Primary Responsibility | Test Suites & Count |
|:---:|---|---|:---:|---|---|
| 1 | `agent-studios-protocol` | `crates/protocol` | 0.1.0 | Shared domain models, task contracts, artifacts, and identifiers | Unit tests (all models) |
| 2 | `agent-studios-control-plane` | `crates/control-plane` | 0.1.0 | Authoritative execution engine, SQLite event store, crash recovery | 63 tests (15 unit, 48 integration) |
| 3 | `agent-studios-provider` | `crates/provider` | 0.1.0 | Provider catalog, model descriptors, and credential redaction | Unit tests |
| 4 | `agent-studios-external-runtime` | `crates/external-runtime` | 0.1.0 | Generic agent runtime traits, lifecycle states, and runtime registry | Unit tests |
| 5 | `agent-studios-opencode-runtime` | `crates/opencode-runtime` | 0.1.0 | OpenCode interpreter runtime adapter and OS process sandbox | Integration tests |
| 6 | `agent-studios-claude-code-runtime` | `crates/claude-code-runtime` | 0.1.0 | Claude Code CLI process supervisor, Win32 Job Object, NDJSON streaming | 74 tests (7 test suites) |
| 7 | `agent-studios-workspace` | `crates/workspace` | 0.1.0 | Git worktree orchestrator, artifact blob store, and lossless diff chunking | Integration tests |
| 8 | `agent-studios-ipc-protocol` | `crates/ipc-protocol` | 0.1.0 | Wire framing, HMAC-SHA256 signer, anti-replay nonces, and ReplayBuffer | 10 unit tests |
| 9 | `agent-studios-ipc-transport` | `crates/ipc-transport` | 0.1.0 | Native `agsd` daemon, named pipe / domain socket, 2PC approvals, hybrid replay | 114 tests (12 test suites) |
| 10 | `agent-studios-codex-bridge` | `crates/codex-bridge` | 0.1.0 | Bridge adapter between Agent Studios and codex-rs upstream components | Integration tests |
| 11 | `agent-studios-runtime-transport` | `crates/runtime-transport` | 0.1.0 | Legacy transport abstractions | Integration tests |
| 12 | `agent-studios-runtime-session` | `crates/runtime-session` | 0.1.0 | Session state machines and subscriber tracking | Unit tests |
| 13 | `agent-studios-internal-agent` | `crates/internal-agent` | 0.1.0 | Internal synthetic test agent implementations | Unit tests |
| 14 | `agent-studios-orchestration` | `crates/orchestration` | 0.1.0 | Multi-agent coordination abstractions | Unit tests |
| 15 | `agent-studios-protocol-adapters` | `crates/protocol-adapters` | 0.1.0 | Wire format adapters and schema conversions | Unit tests |

---

## 3. Unified Workspace `Cargo.toml`

```toml
[workspace]
resolver = "2"
members = [
    "crates/protocol",
    "crates/control-plane",
    "crates/provider",
    "crates/codex-bridge",
    "crates/protocol-adapters",
    "crates/runtime-transport",
    "crates/runtime-session",
    "crates/internal-agent",
    "crates/orchestration",
    "crates/workspace",
    "crates/external-runtime",
    "crates/opencode-runtime",
    "crates/claude-code-runtime",
    "crates/ipc-protocol",
    "crates/ipc-transport",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "Apache-2.0"
authors = ["Agent Studios Contributors"]
repository = "https://github.com/leduwn/agent-studios"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1.10", features = ["v4", "serde"] }
thiserror = "2.0"
chrono = { version = "0.4", features = ["serde"] }
url = { version = "2.5", features = ["serde"] }

[patch.crates-io]
tokio-tungstenite = { git = "https://github.com/openai-oss-forks/tokio-tungstenite", rev = "0e5b2d73aa18dd9f0a50ee9ff199d5aef7594186" }
tungstenite = { git = "https://github.com/openai-oss-forks/tungstenite-rs", rev = "4fffad30fe373adbdcffab9545e9e9bf4f2fc19f" }

[profile.dev]
debug = 1

[profile.test]
debug = 1
```

---

## 4. Multi-Runtime Routing Architecture

```
                    ┌────────────────────────┐
                    │      Client Request    │
                    │       SubmitTask       │
                    └───────────┬────────────┘
                                │
                                ▼
                    ┌────────────────────────┐
                    │    IpcServer Router    │
                    │   connection.rs L1445  │
                    └───────────┬────────────┘
                                │
         ┌──────────────────────┴──────────────────────┐
         ▼                                             ▼
┌─────────────────────────────┐               ┌─────────────────────────────┐
│    OpenCode Runtime         │               │     Claude Code Runtime     │
│  "opencode" Adapter         │               │   "claude-code" Adapter     │
│  - Python Interpreter       │               │   - Claude CLI Subprocess   │
│  - Sandboxed Environment    │               │   - Win32 Job Object Kill   │
│  - Direct Message Delivery  │               │   - 2PC Approval Delivery   │
└─────────────────────────────┘               └─────────────────────────────┘
```

Routing rules enforced by `crates/ipc-transport/src/connection.rs`:
- Explicit `runtime_preference = "claude-code"` -> routes to `ClaudeCodeRuntime`.
- Explicit `runtime_preference = "opencode"` -> routes to `OpenCodeRuntime`.
- If single runtime registered -> auto-routes to available adapter.
- If multiple runtimes registered without preference -> fails closed with `AmbiguousRuntimePreference`.

---

## 5. Linux Compatibility & Security Remediations

### Remediation 1: Conditional Export for `JobObjectGuard`
In `crates/claude-code-runtime/src/lib.rs`:
```rust
// Before (Defective on non-Windows targets):
pub use sandbox::{
    JobObjectGuard, ProcessContainment, SandboxLevel, SandboxPolicy, validate_session_workspace,
    validate_workspace_path,
};

// After (Remediated):
#[cfg(windows)]
pub use sandbox::JobObjectGuard;
pub use sandbox::{
    ProcessContainment, SandboxLevel, SandboxPolicy, validate_session_workspace,
    validate_workspace_path,
};
```

### Remediation 2: Unused Dependency Removal
Pruned `agent-studios-provider`, `agent-studios-workspace`, and `agent-studios-control-plane` from `crates/claude-code-runtime/Cargo.toml`. Verified that 0 source files in `src/` or `tests/` reference these crates.
