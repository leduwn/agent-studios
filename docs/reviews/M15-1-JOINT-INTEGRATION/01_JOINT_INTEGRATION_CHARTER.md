# Agent Studios — Milestone M15.1 Joint Integration Preflight
## Document 01: Joint Integration Charter & Governance Framework

**Milestone:** M15.1 (Joint Integration Preflight & Cross-Platform Release Gate)  
**Date:** 2026-11-06  
**Role:** Principal Integration Engineer, Release Architect & Rust/TypeScript QA Lead  
**Validation Branch:** `integration/m15-1-joint-validation`  
**Target Baseline:** `integration/m15-unified-codebase` (`ec2058256a0001f869b968f4d787b7378e413eb8`)  
**Status:** ACTIVE CHARTER  

---

### 1. Executive Mandate & Purpose

Milestone M15.1 constitutes the formal integration preflight gate for the Agent Studios platform. Following the parallel execution of backend infrastructure consolidation (Milestone M15.1-A) and frontend real-agent continuity engineering (Milestone M15 UI-06), three major pull requests currently stand unmerged against the integration baseline:

1. **PR #31:** Backend Provider Discovery, Anthropic/OpenAI/Ollama Catalog Engine, and Official `ags` CLI.
2. **PR #33:** Unified Cross-Platform CI Matrix, Linux Cargo Clippy Remediation, and Multi-OS Test Hardening.
3. **PR #34:** Frontend Agent-First IDE Experience, Shared Session Continuity, and Real Daemon Binary Provenance.

The purpose of this charter is to define the non-destructive validation framework, safety rules, multi-tier testing gates, and release architecture necessary to confirm combined backend–frontend acceptance without prematurely modifying or merging into `integration/m15-unified-codebase` or `main`.

---

### 2. PR Trio Architecture & Alignment

```
                                  [ main ]
                                     ▲
                                     │ (PR #32 - Preserved Baseline)
                                     │
                     [ integration/m15-unified-codebase ]
                                     ▲
             ┌───────────────────────┼───────────────────────┐
             │                       │                       │
      (Draft PR #31)          (Draft PR #33)          (Draft PR #34)
   Backend & CLI Engine        Cross-Platform CI     Frontend Extension
    feat/m15-1-backend...   fix/m15-unified-ci...   feat/m15-ui-06...
             │                       │                       │
             └───────────────────────┼───────────────────────┘
                                     │
                      (Non-Destructive Joint Merge)
                                     ▼
                [ integration/m15-1-joint-validation ]
                     (Dedicated Validation Worktree)
```

#### PR Trio Roles and Scope:
- **PR #31 (Backend Core):** Introduces official `ags` CLI binary, in-process deterministic engine, real HTTP/SSE provider adapters, model metadata catalog, and diagnostics doctor.
- **PR #33 (CI Infrastructure & Platform Gates):** Resolves compiler warnings, dead-code lints, non-Windows platform sandbox fail-closed gates, and cross-platform path handling in Rust test runners.
- **PR #34 (Frontend IDE & Extension):** Implements VS Code extension agent-first layout, continuous session restoration, strict daemon binary resolution, and extension host mode continuity.

---

### 3. Absolute Safety Invariants & Guardrails

To preserve workspace integrity and project safety, the following rules are strictly enforced:

1. **NO Premature Merges:** Under no circumstances will PR #31, #33, or #34 be merged into `integration/m15-unified-codebase` or `main` until joint release sign-off is achieved.
2. **NO Force Operations:** Absolutely no `git push --force`, `git reset --hard`, or destructive history rewriting on any branch.
3. **Dedicated Isolation Worktree:** All integration verification is isolated within `D:\Projects\agent-studios-active\m15-1-joint-validation` on branch `integration/m15-1-joint-validation`.
4. **Preservation of Active Worktrees:** Active worktrees `D:\Projects\agent-studios-ide-ui`, `D:\Projects\agent-studios-active\m15-1-backend`, `D:\Projects\agent-studios-active\m15-ui-06`, and historical worktrees (`agent-studios-m15-04`, `m15-03`, `m15-02`, `m15`) are strictly preserved.
5. **Codex Crate Invariance:** No modifications to `codex-rs/` crates.
6. **Zero-Dollar Inference Guarantee:** All test suites, CLI invocations, and extension tests run in non-billable, offline, or mock modes. Total spend: **$0.00**.

---

### 4. Verification Hierarchy & Acceptance Criteria

Joint acceptance requires 100% passage across all four tiers of the Agent Studios verification pyramid:

- **Tier 1: Static Analysis & Unit Mock Suites**
  - Rust: `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`.
  - TypeScript: `npm run compile`, `npm run typecheck`, `npm run lint`, `npm run test:unit` (685 tests).
- **Tier 2: Real Daemon Native IPC & SQLite WAL**
  - Native daemon `agsd` process lifecycle, Named Pipe / Unix Domain Socket framing, two-phase HMAC challenge-response, role-based command authorization, crash recovery, and monotonic event replay.
- **Tier 3: Authentic Extension Host Integration**
  - Real unmocked VS Code Extension Development Host via `@vscode/test-electron` (v1.145.0), validating activation, command registration, and session mode continuity without mocking `vscode.commands`.
- **Tier 4: Standalone Production Packaging**
  - VSIX package compilation (`npm run package`), manifest validation, asset containment, and zero external dependency leakage.

---

### 5. Multi-Role Engineering Responsibilities

- **Principal Integration Engineer (Release Architect):** Manages the unified branch, audits cross-PR dependency graphs, resolves CI platform failures, and signs off on the final verdict.
- **Claude 1 (Principal Rust Architect):** Owns `agent-studios-rs/`, control plane, IPC daemon (`agsd`), CLI (`ags`), and provider catalog.
- **Claude 2 (Frontend Implementation Lead):** Owns `apps/agent-studios-vscode/`, webview panels, bridge client, and extension host test suites.
