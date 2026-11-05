# 06 — Integration PR & Release Gate: M15 Consolidation 01.1

**Document Reference**: `docs/reviews/M15-UNIFIED-CONSOLIDATION-01-1/06_INTEGRATION_PR_RELEASE_GATE.md`  
**Milestone**: M15 Consolidation & Canonical Preflight  
**Integration Branch**: `integration/m15-unified-codebase`  
**Target Branch**: `main`  
**Draft PR Title**: `feat(m15): unify backend, runtime and VS Code frontend`  
**Auditor & Release Lead**: Principal Integration Engineer & Release Manager  

---

## 1. Draft Pull Request Specification

### Metadata
- **Base**: `main`
- **Compare**: `integration/m15-unified-codebase`
- **Unified HEAD SHA**: `5c8f5fd6d84376c4b1c2420f7f8b911fb58d3a3c`
- **Creation Link**: [Open Draft PR on GitHub](https://github.com/leduwn/agent-studios/pull/new/integration/m15-unified-codebase)

---

### PR Description Body (Ready to Paste)

```markdown
## Summary & Milestone Objectives

This Pull Request establishes the consolidated baseline for **Agent Studios (Milestones M10 through M15)**, unifying the disjoint backend IPC/control plane, runtime adapters, and Code-OSS VS Code extension into a single cohesive codebase.

### Integrated Milestones & Tracks
- **M10 (Runtime Foundation & External Interface)**: PR #13 merged (`a9cba2dca9`).
- **M11 (OpenCode Runtime Adapter & Sandbox)**: PR #14 merged (`dc363d1e89`, baseline common ancestor).
- **M12 (Claude Code Runtime & Subprocess Security)**: PR #15 (`3207b7462f`), adding `crates/claude-code-runtime`, Win32 Job Object sandbox (`#[cfg(windows)] JobObjectGuard`), and 2PC stdin approval delivery.
- **M13 (Native IPC Protocol, Transport & Control Plane)**: PR #17, #18, #19 (`ce0396c1f1`), implementing length-prefixed Named Pipe/Unix Domain Socket IPC, HMAC-SHA256 authentication, and `agsd` native daemon.
- **M14 (Code-OSS Agent Workbench & Mission Control)**: PR #21, #22, #23, #25 (`a13230ee93`), delivering the 9-tab Mission Control observability workbench, extension host security, and live event read-model projection.
- **M15 (Real Agent Execution, Durable Replay & Unified Consolidation)**:
  - Backend: PR #26, #28, #29, #30 (`cf1e6c0f98`), delivering dynamic multi-runtime routing, lossless chunked Git diff streaming, SQLite WAL durable event store, and single-writer advisory locks.
  - Frontend: PR #27 (`3de7c85a96`), delivering live IPC streaming, task execution, and automated crash recovery.

---

## Source SHAs & Unified Topology

- **Common Ancestor Base**: `dc363d1e89fe8751c001f9829bd2cd0b9f7d37f9` (PR #14 / M11)
- **Authoritative Backend Tip**: `cf1e6c0f98d568ed0b4c9da9ecf80c6d3eea456d` (PR #30, M15-04)
- **Authoritative Frontend Tip**: `3de7c85a966d1b46a293235b11170219d53cc7f3` (PR #27, M15-UI-05A)
- **Runtime Adapter Tip**: `3207b7462f9e3055ceee6996c4bfc914cf26fae7` (PR #15, M12.3)
- **Unified Merge Commit**: `9e1a2a2a3d`
- **Consolidation HEAD SHA**: `5c8f5fd6d84376c4b1c2420f7f8b911fb58d3a3c`

---

## Quality Gates & Verification Evidence

| Quality Gate | Target / Test Suite | Result | Details |
|---|---|:---:|---|
| **Rust Compilation** | `cargo check --workspace --all-targets` | **PASS** | 0 errors across 15 crates |
| **Rust Clippy** | `cargo clippy --workspace --all-targets` | **PASS** | 0 warnings (`-D warnings` enforced) |
| **Rust Backend Tests** | `cargo test --workspace` | **PASS** | **339+ tests passed**, 0 failed |
| **TS Compilation** | `npm run compile && npm run typecheck` | **PASS** | 0 errors |
| **TS Linting** | `npm run lint` | **PASS** | 0 errors |
| **TS Unit Tests** | `npm run test:unit` | **PASS** | **657 tests passed**, 0 failed (4s) |
| **Real Daemon E2E** | `npm run test:e2e` | **PASS** | **78 tests passed**, 0 failed (12s) |
| **Extension Host Tests**| `npm run test:integration` | **PASS** | Verified on Linux (Xvfb) and Windows |
| **VSIX Packaging** | `npx @vscode/vsce package --no-dependencies` | **PASS** | Distribution bundle packaged |
| **Zero Billable Invariant**| $0.00 spent across all test suites | **PASS** | Verified mock/offline harnesses |

---

## Security Invariants & Limitations

1. **Production AI Execution Remains Gated**:
   - `task.submit` is disabled by default in `CapabilityMatrix`.
   - Live AI execution requires explicit operator challenge validation and multi-factor confirmation modals.
   - Zero billable token cost incurred during CI or test execution ($0.00).
2. **Role-Based Redaction**:
   - Standard client connections are read-only; mutations fail closed with permission errors.
   - Internal SQLite database paths and system metrics are redacted from standard clients.
3. **OS-Level Sandboxing**:
   - Windows processes constrained via Win32 Job Objects (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`).
   - Linux processes isolated in process groups (`setpgid`).

---

## Known Issues & Follow-Ups

1. **Provider Discovery & CLI (PR #20)**:
   - Standalone CLI/TUI and dynamic model discovery (`discovery.rs`) remain tracked on `feat/provider-management-v1` (`a98129ef`) and uncommitted in canonical root. Candidate for immediate follow-up PR following M15 merge.
2. **`agent-studios-mission-control` Worktree on HOLD**:
   - Retained on `HOLD` because raw commits (`3ee485ca`) diverged from `ed80b243`, even though its code was rebased into PR #25 (`a13230ee`) and integrated into unified HEAD.

---

## Operator Review Checklist

- [x] All 15 Rust crates build without warnings or errors.
- [x] All 735 TypeScript tests pass without failures.
- [x] Linux CI compilation defect (`JobObjectGuard`) fixed with `#[cfg(windows)]`.
- [x] Unified GitHub Actions workflow (`agent-studios-unified-ci.yml`) configured for Ubuntu & Windows.
- [x] Zero billable API token invariant verified ($0.00).
- [x] Canonical root `D:\Projects\agent-studios` uncommitted changes preserved.
- [x] 8 candidate worktrees preflighted with zero deletions executed.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```
