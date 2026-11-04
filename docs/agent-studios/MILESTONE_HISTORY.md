# Agent Studios — Milestone History & Evolution (M01 – M15)

> **Status**: Authoritative Milestone Chronicle & Evolution History  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Reference**: Synthesizes git DAG audit, PR review bundles (M10-M15), and `02_UNIFIED_HISTORY_VERIFICATION.md`

---

## 1. Master Milestone Overview

```text
2026-Q1 ─────────────────────────────────────────────────────────────► 2026-Q4
M01-M02: Core Protocol & Rust Control Plane Actor
    │
    ▼
M03-M07.6: Multi-Provider Adapters (OpenAI, Anthropic, Gemini, Ollama)
    │
    ▼
M08-M09: Internal Multi-Agent Runtime, Git Worktrees & Artifact Store
    │
    ▼
M10-M11: External Runtime Interface & OpenCode Adapter (PR #13, #14)
    │
    ▼
M12: Claude Code Runtime & Win32 Job Object Sandbox (PR #15)
    │
    ▼
M13: Native IPC Protocol, Length-Prefixed Transport & agsd Daemon (PR #17-#19)
    │
    ▼
M14: Code-OSS Extension, Activity Bar & 9-Tab Mission Control (PR #21-#25)
    │
    ▼
M15: Real Execution, Lossless Diff, Durable Replay & Consolidation (PR #26-#30)
```

---

## 2. Milestone Chronicles

### M01 & M02: Protocol Foundation & Deterministic Control Plane
- **Scope**: Created `crates/protocol` and `crates/control-plane`. Established strong ID newtypes (`SessionId`, `TaskId`, `RunId`, `AgentId`), domain event schemas, and the single-writer `ControlPlaneActor`.
- **Key Innovations**: Implemented Kahn's algorithm for topological sorting and cycle detection in task graphs. Enforced the fundamental rule: LLM intelligence is probabilistic; state transitions are deterministic.
- **Verification**: Unit tests covering state transitions, acyclicity invariants, and boundary conditions.

### M03 – M07.6: Multi-Provider Wire Protocols & In-Memory Secret Zeroization
- **Scope**: Built `crates/provider`, `crates/protocol-adapters`, `crates/runtime-transport`, and `crates/runtime-session`.
- **Supported Wire Protocols**:
  - Anthropic Messages API (Claude 3.5 / 3.7 Sonnet).
  - OpenAI Chat Completions & Responses (GPT-4o, o1, o3-mini).
  - Google Gemini GenerateContent (Gemini 1.5/2.0).
  - Local / Custom HTTP endpoints (Ollama, vLLM).
- **Key Innovations**: In-memory secret zeroization using `SecretString` to guarantee zero plaintext leakage. WireMock mock test harness validating SSE chunk reassembly and error mapping.

### M08: Internal Multi-Agent Supervisor & Pre-Execution Budgets
- **Scope**: Established `crates/internal-agent`. Created thread management integration with upstream `codex-rs`.
- **Key Innovations**:
  - `WorkspacePolicyArbitrator`: Single-writer mutating lock with RAII `WorkspaceLease`.
  - `AgentStudiosToolLifecycleContributor`: Pre-execution tool budget enforcer.
  - Supervisor failure policies (`FailFast`, `ContinueIndependent`, `RetryTask`).

### M09: Git Worktree Isolation & Ephemeral Index Change Capture
- **Scope**: Created `crates/workspace`.
- **Key Innovations**:
  - Dedicated Git worktrees under `.git/agent-studios/worktrees/<session>/<task>/`.
  - Ephemeral Git index isolation via `GIT_INDEX_FILE` preventing corruption of the main working tree.
  - Content-addressed SHA-256 artifact storage (`blobs/sha256/<hash>`).
  - Deterministic patch reconciliation via `git apply --check` and `git apply --3way`.
  - `Blocked != Cancelled` invariant: Tasks with merge conflicts transition to `Blocked`, never cancelled.

### M10: External Runtime Interface (PR #13, `a9cba2dca9`)
- **Scope**: Created `crates/external-runtime`.
- **Key Innovations**:
  - Core trait `AgentRuntime` defining `spawn`, `send`, `interrupt`, `stop`, `events`.
  - Tristate capability profiles across 16 dimensions (`Supported`, `Unsupported`, `Unknown`).
  - Monotonic, gapless event sequencing with `EventSequencer`.
  - Remediated 18 contract findings across three review rounds (P1-01 to P1-06, SR3-01 to SR3-08).

### M11: OpenCode Runtime Adapter & OS Containment (PR #14, `dc363d1e89`)
- **Scope**: Created `crates/opencode-runtime`.
- **Key Innovations**:
  - Subprocess adapter executing OpenCode interpreter tasks.
  - Linux process group sandboxing (`setpgid`).
  - Mapping OpenCode session steps into canonical Control Plane `Task` and `Run` entities.

### M12: Claude Code Runtime Adapter & Approval Security (PR #15, `3207b7462f`)
- **Scope**: Created `crates/claude-code-runtime`.
- **Key Innovations**:
  - Subprocess supervisor for Anthropic Claude Code CLI.
  - Win32 Job Object lifecycle sandbox (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`).
  - Two-Phase Commit (2PC) stdin delivery for interactive tool approvals.
  - Remediated M12.3 findings: process termination on stdin write failure, zero-duration timeout protection, and cross-platform compilation guards (`#[cfg(windows)] JobObjectGuard`).

### M13: Native IPC Protocol, Transport & `agsd` Daemon (PR #17, #18, #19, `ce0396c1f1`)
- **Scope**: Created `crates/ipc-protocol` and `crates/ipc-transport`. Compiled native `agsd` daemon binary.
- **Key Innovations**:
  - Physical transport via Windows Named Pipes (`\\.\pipe\agent-studios-*`) and Unix Domain Sockets (`/tmp/agent-studios-*/agsd.sock`).
  - 4-byte big-endian length-prefixed framing with 4 MiB clamp.
  - HMAC-SHA256 handshake with 32-byte nonces, 30s timestamp window, and sliding anti-replay window.
  - Role-based privilege segregation (`agsd.json` read-only vs. `agsd-admin.json` full admin).

### M14: Native Code-OSS Extension & Mission Control (PR #21 – #25, `a13230ee93`)
- **Scope**: Created `apps/agent-studios-vscode`.
- **Key Innovations**:
  - Strict Activity Bar Rule: Exactly ONE icon (`Agents`) added to Code-OSS Activity Bar.
  - 9-Tab Mission Control observability workbench (Overview, Fleet, Tasks Kanban/DAG, Runs, Timeline, Review, Approvals, Providers, Diagnostics).
  - Zero-dependency SVG topological DAG graph renderer.
  - Native `vscode.diff` launching via `DiffReviewManager`.
  - 2PC confirmation modal for operator action approval.

### M15: Real Execution, Multi-Runtime, Lossless Diff & Consolidation
- **Scope**: Consolidated entire codebase across PRs #26, #27, #28, #29, #30, and #15.
- **Authoritative Source Lineage**:
  - Authoritative Backend Tip: `cf1e6c0f98` (PR #30). Persistent SQLite WAL event store, single-writer advisory lock (`DatabaseLock`), crash recovery, and monotonic replay backfill.
  - Authoritative Frontend Tip: `3de7c85a96` (PR #27). `ArtifactClient` multi-chunk diff reassembly with SHA-256 digest validation, live Mission Control event projection, 78 E2E cross-process tests.
  - Merged Cleanly without conflicts: `9e1a2a2a3d`.
  - Ported M12 Claude Code runtime with Linux compilation guards: `5c8f5fd6d8`.
  - Fixed cargo fmt wrap check: `661daaa908` (Unified HEAD).
- **Consolidated Baseline**: 15 Rust crates, 339+ Rust tests, 735 TypeScript tests, unified cross-platform CI matrix on Ubuntu and Windows.
