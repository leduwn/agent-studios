# M15 Canonical Migration — Draft PR Release Gate Specification

**Document ID**: `docs/reviews/M15-CANONICAL-MIGRATION/05_DRAFT_PR_STATUS.md`  
**Date**: 2026-11-05  
**Auditor**: Principal Git Integration Engineer & Release Manager  
**Base Branch**: `main`  
**Head Branch**: `integration/m15-unified-codebase`  
**Status**: DRAFT READY FOR OPERATOR MERGE AUTHORIZATION  

---

## 1. Pull Request Parameters

- **Repository**: `https://github.com/leduwn/agent-studios`
- **Base Branch**: `main`
- **Head Branch**: `integration/m15-unified-codebase`
- **Draft Mode**: Enabled (`draft: true`)
- **Direct Web URL**: [Create Draft PR on GitHub](https://github.com/leduwn/agent-studios/pull/new/integration/m15-unified-codebase)
- **PR Title**: `feat(m15): unify backend, runtime and VS Code frontend`

---

## 2. Pull Request Description Body

```markdown
# Agent Studios — Unified Consolidation (M01–M15)

## Summary
This Pull Request establishes the authoritative unified baseline for Agent Studios by consolidating backend control plane crates, Claude Code runtime adapter, Code-OSS extension frontend, and unified cross-platform CI into a single monorepo.

### Provenance & Lineages Merged
- **Authoritative Backend Tip (PR #30)**: `cf1e6c0f98d568ed0b4c9da9ecf80c6d3eea456d` (Durable SQLite WAL recovery, Kahn DAG scheduling, 2PC stdin approval protocol).
- **Authoritative Frontend Tip (PR #27)**: `3de7c85a966d1b46a293235b11170219d53cc7f3` (9-tab Mission Control workbench, lossless diff streaming, single Activity Bar icon).
- **Claude Code Runtime Adapter (PR #15 / M12.3)**: `3207b7462f9e3055ceee6996c4bfc914cf26fae7` (Win32 Job Object sandboxing, 2PC challenge token stdin verification).
- **Master Documentation Reconstruction**: 17 authoritative specifications under `docs/agent-studios/`.

### Verified Quality Gates
- **Rust Workspace**: 15 active crates; `cargo check` clean (0 errors), `cargo clippy` clean (0 warnings with `-D warnings`), 339+ tests passing.
- **VS Code Extension**: `tsc --noEmit` clean, ESLint clean, 657 unit tests passing, 78 cross-process E2E daemon tests passing.
- **Unified Cross-Platform CI**: `.github/workflows/agent-studios-unified-ci.yml` builds from checked-out commit SHA without split-brain checkouts.
- **Zero Billable Invariant**: All test suites enforce non-billable mocks/metadata ($0.00 spent).

### Safety & Invariants
- Provider Discovery v1.2 WIP preserved on dedicated branch `feat/provider-discovery-v1.2-wip` (`96dfb4946e`).
- Zero worktrees deleted under Phase 0 Safety Gate.
- Canonical repository `D:\Projects\agent-studios` verified clean at unified HEAD (`843be62033`).

### Reviewer Checklist
- [ ] Verify `.github/workflows/agent-studios-unified-ci.yml` passes on Ubuntu and Windows.
- [ ] Confirm `task.submit` remains gated by operator challenge.
- [ ] Confirm no secrets present in commit history or logs.
- [ ] Authorize squash-merge or merge commit into `main`.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```
