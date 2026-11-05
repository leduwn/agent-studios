# M15.1-A Release Repair: Draft Pull Request Status Matrix

**Document ID**: `M15-1-A-02-DRAFT-PR-STATUS`  
**Date**: 2026-11-06  
**Status**: ACTIVE TRACKING & LINEAGE DOCUMENTATION  

---

## 1. Executive Summary

As part of the M15 unified repository governance and M15.1-A release gate, three distinct Draft Pull Requests are maintained to ensure complete traceability, safe review, and non-destructive integration without split-brain anomalies.

```
       [main] (Production baseline)
          ▲
          │ (PR #32: Unified Backend, Runtime, & Frontend)
          │
 [integration/m15-unified-codebase] (ec2058256a)
          ▲                                    ▲
          │ (PR #31: M15.1 Provider CLI)       │ (PR #33: CI Cross-Platform Fixes)
          │                                    │
 [feat/m15-1-provider-cli-unification]    [fix/m15-unified-cross-platform-ci]
            (c98a8a3572)                             (65397d709d)
```

---

## 2. Active Pull Request Matrix

| PR # | Head Branch | Base Branch | Head SHA | Status | Purpose | CI Link |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **#31** | `feat/m15-1-provider-cli-unification` | `integration/m15-unified-codebase` | `c98a8a3572` | Draft | Provider Discovery v1.2, AGS CLI, Doctor subcommand | [Run #42079215315](https://github.com/leduwn/agent-studios/actions/runs/42079215315) |
| **#32** | `integration/m15-unified-codebase` | `main` | `ec2058256a` | Draft | Full M15 Unified Codebase (Backend, Runtime, VS Code extension) | [Run #42079285158](https://github.com/leduwn/agent-studios/actions/runs/42079285158) |
| **#33** | `fix/m15-unified-cross-platform-ci` | `integration/m15-unified-codebase` | `65397d709d` | Draft | Platform conditional compilation for Clippy & Windows 8.3 path fix | [Run #42094782523](https://github.com/leduwn/agent-studios/actions/runs/42094782523) |

---

## 3. Pull Request Details

### Draft PR #31: Provider Discovery & Official AGS CLI Foundation
- **URL**: `https://github.com/leduwn/agent-studios/pull/31`
- **Title**: `feat(m15.1): integrate provider discovery and official AGS CLI foundation`
- **Head SHA**: `c98a8a357235c1a808781e9fdadcecb7a8510093`
- **Base SHA**: `ec2058256a0001f869b968f4d787b7378e413eb8`
- **Scope**: Implements Provider Discovery v1.2, secret resolution, `ags provider` commands, and 8-point doctor probe with graceful fallback.
- **Merge Order**: Blocked by PR #33 (cross-platform CI fix). Must rebase on `integration/m15-unified-codebase` once PR #33 is merged.

### Draft PR #32: Unified Backend, Runtime, and VS Code Frontend
- **URL**: `https://github.com/leduwn/agent-studios/pull/32`
- **Title**: `feat(m15): unify backend, runtime and VS Code frontend`
- **Head SHA**: `ec2058256a0001f869b968f4d787b7378e413eb8`
- **Base SHA**: `main` (repository root)
- **Scope**: Monorepo integration bringing together 11 Rust backend crates (`agent-studios-rs`), Codex runtime subsystem (`codex-rs`), and VS Code extension (`apps/agent-studios-vscode`).
- **Ancestry Note**: Documents complete historical provenance of M13 IPC transport, M14 UI continuity, and Phase B safe worktree cleanup.
- **Merge Order**: Pending full green CI run across Ubuntu and Windows runners.

### Draft PR #33: Cross-Platform CI Release Repair
- **URL**: `https://github.com/leduwn/agent-studios/pull/33`
- **Title**: `fix(ci): resolve Ubuntu Clippy warnings and Windows 8.3 path test mismatches`
- **Head SHA**: `65397d709df95b367d3eec9ee7f516a2ef772635`
- **Base SHA**: `ec2058256a0001f869b968f4d787b7378e413eb8`
- **Scope**: Surgical fixes to `crates/claude-code-runtime` and `crates/workspace/tests/workspace_tests.rs`.
- **Merge Order**: Priority 1. Merging this PR establishes green CI baseline for `integration/m15-unified-codebase`.

---

## 4. Execution Sequence to Final Release

1. **Step 1**: Complete verification of PR #33 on GitHub Actions Run #42083550287.
2. **Step 2**: Operator review and merge of PR #33 into `integration/m15-unified-codebase`.
3. **Step 3**: Fast-forward/rebase `feat/m15-1-provider-cli-unification` onto updated `integration/m15-unified-codebase`. PR #31 automatically re-triggers green CI.
4. **Step 4**: Merge PR #31 into `integration/m15-unified-codebase`.
5. **Step 5**: Fast-forward PR #32 on `main`. Once operator approves, merge PR #32 into `main`.
