# M15 Canonical Migration — Unified Branch Verification Report

**Document ID**: `docs/reviews/M15-CANONICAL-MIGRATION/03_UNIFIED_BRANCH_VERIFICATION.md`  
**Date**: 2026-11-05  
**Auditor**: Principal Git Integration Engineer & Release Manager  
**Branch Target**: `integration/m15-unified-codebase`  
**Verified Integration HEAD**: `843be620336de461b77708543dee5edd55e6f34e`  
**Status**: VERIFIED & CRYPTOGRAPHICALLY VALIDATED  

---

## 1. Executive Summary

This report establishes the cryptographic provenance, commit ancestry, and structural completeness of the unified consolidation branch `integration/m15-unified-codebase` at HEAD commit `843be620336de461b77708543dee5edd55e6f34e`.

---

## 2. Cryptographic Ancestry & Lineage Verification

Three independent sprint lineages were unified into this single baseline:

### Lineage A: Authoritative Backend Tip (PR #30)
- **Commit SHA**: `cf1e6c0f98d568ed0b4c9da9ecf80c6d3eea456d`
- **Branch**: `feat/m15-04-durable-state-recovery`
- **Ancestry Verification**: `git merge-base --is-ancestor cf1e6c0f98 843be62033` returned exit code `0`.
- **Verdict**: Direct cryptographic DAG ancestor of unified HEAD.

### Lineage B: Authoritative Frontend Tip (PR #27)
- **Commit SHA**: `3de7c85a966d1b46a293235b11170219d53cc7f3`
- **Branch**: `feat/m15-real-agent-execution-ui`
- **Ancestry Verification**: `git merge-base --is-ancestor 3de7c85a96 843be62033` returned exit code `0`.
- **Verdict**: Direct cryptographic DAG ancestor of unified HEAD.

### Lineage C: Claude Code Runtime Adapter Tip (PR #15 / M12.3)
- **Commit SHA**: `3207b7462f9e3055ceee6996c4bfc914cf26fae7`
- **Branch**: `feat/m12-claude-code-runtime`
- **Consolidation Commit**: `5c8f5fd6d84376c4b1c2420f7f8b911fb58d3a3c`
- **Source Verification**: All 13 implementation files in `agent-studios-rs/crates/claude-code-runtime` match commit `3207b7462f`, with two non-breaking fixes applied:
  1. Guarded `pub use sandbox::JobObjectGuard;` with `#[cfg(windows)]` to fix Linux CI compilation.
  2. Formatted lines in `approval_concurrency_tests.rs` to satisfy `cargo fmt --check`.
- **Verdict**: Fully integrated and active in the Rust workspace.

---

## 3. Monorepo Structural Completeness

The unified branch integrates the entire Agent Studios ecosystem into a cohesive monorepo:
1. **15 Rust Crates**: `protocol`, `control-plane`, `provider`, `codex-bridge`, `protocol-adapters`, `runtime-transport`, `runtime-session`, `internal-agent`, `orchestration`, `workspace`, `external-runtime`, `opencode-runtime`, `claude-code-runtime`, `ipc-protocol`, `ipc-transport`.
2. **VS Code Extension**: `apps/agent-studios-vscode` with 9-tab Mission Control, SVG DAG visualizer, lossless diff viewer, and connection manager.
3. **Cross-Platform CI**: `.github/workflows/agent-studios-unified-ci.yml` running dual-matrix testing on `ubuntu-latest` and `windows-latest`.
4. **Canonical Documentation**: 17 master specifications under `docs/agent-studios/` establishing product vision, PRD, architecture, dual-mode UX, multi-agent control plane, and roadmap to GA v1.0.0.
