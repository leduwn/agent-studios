# Agent Studios — Remaining Gaps & Backlog Analysis

> **Status**: Authoritative Gap Audit & Backlog Specification  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Reference**: Synthesizes Subagent D Roadmap Analysis, M15 Reviews, and PR #20 Audit

---

## 1. Executive Summary: What Is Done vs. What Remains

`[IMPLEMENTED_BEHAVIOR]`
Milestone M15 successfully established the unified codebase baseline:
- 15 Rust crates compiled cleanly with 0 clippy warnings (`-D warnings`).
- 339+ Rust backend tests and 735 TypeScript extension tests passing with 0 failures.
- Native length-prefixed IPC transport (`agsd`) and 9-tab Mission Control workbench unified.
- Two-Phase Commit (2PC) approval routing and Win32 Job Object sandbox implemented.
- Unified cross-platform CI workflow configured for Ubuntu and Windows.

However, moving from the current consolidation baseline to a production-grade developer release requires addressing specific operator decision gates and technical backlog milestones.

---

## 2. Operator Decision Gates (Action Required by Human Lead)

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        OPERATOR DECISION GATES                         │
├──────────────────────────────────┬─────────────────────────────────────┤
│ Gate Item                        │ Required Operator Decision          │
├──────────────────────────────────┼─────────────────────────────────────┤
│ 1. Unified PR Merge              │ Authorize Draft PR merge to main    │
│ 2. Canonical Root Preservation   │ Select branch or stash for PR #20   │
│ 3. Obsolete Worktree Removal     │ Approve pruning 7 candidate trees   │
│ 4. Mission Control Retention     │ Confirm permanent HOLD disposition  │
│ 5. Live AI Execution Budget      │ Authorize non-zero token spend cap  │
└──────────────────────────────────┴─────────────────────────────────────┘
```

1. **Unified PR Merge Authorization**:
   - `[USER_DIRECT_REQUIREMENT]` Operator must review the Draft PR specification (`06_INTEGRATION_PR_RELEASE_GATE.md`) and authorize the merge of `integration/m15-unified-codebase` into `main`.
2. **Canonical Root Preservation Strategy**:
   - `[EXISTING_CANONICAL_DECISION]` In `D:\Projects\agent-studios`, preserve uncommitted dynamic model discovery files (`discovery.rs`) before switching branches:
     - *Recommended*: Commit to backup branch `feat/provider-discovery-v1.2-wip`.
     - *Alternative*: Stash with descriptive tag `STASH-PRE-M15-CONSOLIDATION-PROVIDER-DISCOVERY-20261104`.
3. **Obsolete Worktree Pruning Approval**:
   - `[EXISTING_CANONICAL_DECISION]` In strict accordance with Phase 0 Safety Gate, 7 `APPROVED_CANDIDATE` worktrees (`m13`, `m15`, `m15-02`, `m15-03`, `vscode-m14-02`, `vscode-ipc-03`, `mission-control-integration`) are preflighted and safe to remove, but require explicit operator sign-off to reclaim 23.69 GB.
4. **Mission Control Disposition Confirmation**:
   - `[EXISTING_CANONICAL_DECISION]` Confirm that `D:\Projects\agent-studios-mission-control` remains on **`HOLD`** because its raw commits (`3ee485ca`) diverged from `ed80b243`, even though its source code was cleanly rebased into PR #25.
5. **Live AI Execution Budget Allocation**:
   - `[USER_DIRECT_REQUIREMENT]` Authorize an explicit non-zero budget ($1.00 – $5.00) to execute live end-to-end coding agent acceptance tests. All automated tests currently run under the $0.00 zero-billable gate.

---

## 3. Technical Implementation Backlog

### 3.1 Immediate P0: Provider Discovery & CLI Consolidation (M15.1)
- `[IMPLEMENTED_BEHAVIOR]` Dynamic model discovery client implemented in `crates/provider/src/discovery.rs` (PR #20).
- `[OPEN_DECISION]` Merge `crates/cli`, `crates/cli-client`, and `crates/tui` from `agent-studios-ags-cli` into the canonical `agent-studios-rs` workspace.
- Wire `ClaudeCodeRuntime` and internal `CodexRuntime` into `cli-client/src/runtime.rs`.
- Implement `ags run --headless "<goal>"` supporting non-interactive CI automation with JSON event streaming.

### 3.2 Milestone M16: Full Codex Feature Surface Parity
- **Skills System (`SKILL.md`)**:
  - Implement full parsing, progressive loading, and GitHub repository installation (`/skill install <url>`).
  - Support Global (`~/.agent-studios/skills`), Project (`.agent-studios/skills`), and Agent scopes.
- **Model Context Protocol (MCP)**:
  - Add workspace (`.agent-studios/mcp.json`) and global (`~/.agent-studios/mcp.json`) MCP server configurations.
  - Surface MCP tool approval gates in Mission Control and TUI.
- **Lifecycle Hooks**:
  - Implement deterministic hooks: `pre_turn`, `post_task`, `on_reconciliation`, `on_approval`.
- **Hierarchical Instructions**:
  - Support recursive `AGENTS.md` and `CLAUDE.md` discovery scoped to allocated worktree directories.

### 3.3 Milestone M17: Desktop Shell & Native Distribution Packaging
- **Code-OSS Desktop Shell Integration**:
  - Package standalone Code-OSS desktop binary bundling Monaco, LSP, DAP, and the native `agsd` daemon.
  - Deliver seamless Agent Mode startup view with instant zero-loss toggling to IDE Mode.
- **Native Installers & Auto-Updater**:
  - Windows: Native Windows 11 x64 MSI / NSIS installer.
  - Code signing pipeline and cryptographic auto-update channel.
  - Canonical OS paths:
    - Config: `%APPDATA%\AgentStudios\config.toml`
    - State: `%LOCALAPPDATA%\AgentStudios\state.db`
    - Artifacts: `%LOCALAPPDATA%\AgentStudios\artifacts\`

### 3.4 Milestone M18: Concurrency Stress & Security Hardening
- **Multi-Agent Stress**: Benchmark 10+ parallel mutating workers under high worktree contention.
- **Long-Running Session Profiling**: Validate 1000+ turns and 100k+ event envelopes without memory leaks.
- **Abnormal Crash Recovery**: Simulate hard daemon termination (`SIGKILL`) during active task execution; verify SQLite lock recovery and fail-closed task reconciliation.

### 3.5 Milestone M19: Production Stable Release (v1.0.0 GA)
- Out-of-the-box first-run setup wizard.
- Full bilingual English and Vietnamese UI verification.
- Production documentation, developer guide, and public distribution release.
