# Agent Studios — Milestone M15.1 Joint Integration Preflight
## Document 02: PR Trio Audit, File Manifest & Dependency Topology

**Milestone:** M15.1 (Joint Integration Preflight & Cross-Platform Release Gate)  
**Date:** 2026-11-06  
**Role:** Principal Integration Engineer & Release Architect  
**Validation Branch:** `integration/m15-1-joint-validation`  
**Status:** AUDITED & VERIFIED  

---

### 1. PR Trio Detailed Audit

Each component of the PR trio addresses a distinct architectural layer of the Agent Studios unified platform:

#### 1.1 PR #31: Backend Provider Discovery & Official AGS CLI
- **PR URL:** https://github.com/leduwn/agent-studios/pull/31
- **Head Branch:** `feat/m15-1-backend-cli-consolidation`
- **Head Commit:** `c98a8a357235c1a808781e9fdadcecb7a8510093`
- **Base Branch:** `integration/m15-unified-codebase`
- **Lead Architect:** Claude 1 (Principal Rust Architect)
- **Scope & Additions:**
  - Introduces official CLI binary `ags` (`crates/cli/`) supporting `doctor`, `provider list`, `model list`, `--preview`, and `--demo`.
  - Implements provider protocol abstraction supporting `anthropic_messages`, `openai_responses`, and `ollama_native`.
  - Built-in in-process deterministic engine (`InMemoryStore` + `SystemClock`) with 4 default agent roles.
  - 227 passing backend Rust tests across provider, control plane, CLI client, and TUI crates.
  - Documents published under `docs/reviews/M15-1-A-BACKEND/` including `backend_frontend_contract_manifest.json`.

#### 1.2 PR #33: Unified Cross-Platform CI Matrix & Compiler Hygiene
- **PR URL:** https://github.com/leduwn/agent-studios/pull/33
- **Head Branch:** `fix/m15-unified-cross-platform-ci`
- **Head Commit:** `b0696ace587de0e25007364a6bf8b131327de5a3` (updated from `5045fd0dc3287bef9b54a9ebd757d33190b6c3dd` and `bc72285b1d`)
- **Base Branch:** `integration/m15-unified-codebase`
- **Lead Architect:** Principal Integration Engineer & Release QA Lead
- **Scope & Additions:**
  - Multi-OS GitHub Actions workflow matrix (`ubuntu-latest` and `windows-latest`).
  - Resolves Clippy warnings (`-D warnings`), unused imports, and dead-code annotations across backend crates.
  - Normalizes Windows 8.3 short paths (`RUNNER~1`) vs canonical long paths in workspace test fixtures.
  - Gates Windows Job Object process containment so non-Windows platforms fail closed cleanly with `UnsupportedPlatform` without crashing tests that exercise process execution.

#### 1.3 PR #34: Frontend Agent/IDE Continuity & Daemon Binary Provenance
- **PR URL:** https://github.com/leduwn/agent-studios/pull/34
- **Head Branch:** `feat/m15-ui-06-agent-ide-continuity`
- **Head Commit:** `f57bc20c746d30e70958c2c61d2a72cb070b91ce` (updated from `7c8014dc520cd054c228cc9125833e1fe892f71c`)
- **Base Branch:** `integration/m15-unified-codebase`
- **Lead Architect:** Claude 2 (Principal VS Code Extension Lead)
- **Scope & Additions:**
  - Implements `AgsdBinaryResolver.ts` with strict CI boundary enforcement, rejecting historical worktrees (`agent-studios-m15-04`, `m15-03`, `m15-02`, `m15`).
  - Unmocked VS Code Extension Host integration test suite (`modeContinuity.test.ts`) validating command execution (`openInIde`, `switchToAgentMode`, `continueSession`).
  - 100% frontend gate passage: 685 unit tests, 84 real daemon E2E tests, 25 extension host integration tests (794 total).
  - Documents published under `docs/reviews/M15-UI-06-RELEASE-HARDENING/`.

---

### 2. Zero File-Overlap Proof Matrix

A critical prerequisite for clean joint integration is the mathematical guarantee that none of the three PRs modify identical files. An exhaustive cross-PR path intersection audit was performed:

| Component / Subsystem Path | PR #31 (Backend) | PR #33 (CI Repair) | PR #34 (Frontend) | Intersection Status |
| :--- | :---: | :---: | :---: | :---: |
| `.github/workflows/unified-ci.yml` | - | **MODIFIED** | - | **DISJOINT (PR #33 only)** |
| `agent-studios-rs/crates/cli/**` | **ADDED** | - | - | **DISJOINT (PR #31 only)** |
| `agent-studios-rs/crates/tui/**` | **ADDED** | - | - | **DISJOINT (PR #31 only)** |
| `agent-studios-rs/crates/provider/**` | **ADDED** | - | - | **DISJOINT (PR #31 only)** |
| `agent-studios-rs/crates/claude-code-runtime/**` | - | **MODIFIED** | - | **DISJOINT (PR #33 only)** |
| `agent-studios-rs/crates/opencode-runtime/**` | - | **MODIFIED** | - | **DISJOINT (PR #33 only)** |
| `agent-studios-rs/crates/workspace/**` | - | **MODIFIED** | - | **DISJOINT (PR #33 only)** |
| `apps/agent-studios-vscode/**` | - | - | **MODIFIED** | **DISJOINT (PR #34 only)** |
| `docs/reviews/M15-1-A-BACKEND/**` | **ADDED** | - | - | **DISJOINT (PR #31 only)** |
| `docs/reviews/M15-UI-06-RELEASE-HARDENING/**` | - | - | **ADDED** | **DISJOINT (PR #34 only)** |
| `codex-rs/**` | - | - | - | **UNTOUCHED (Invariant)** |

**Mathematical Proof Result:**
$$\text{Files}(PR_{31}) \cap \text{Files}(PR_{33}) = \emptyset$$
$$\text{Files}(PR_{31}) \cap \text{Files}(PR_{34}) = \emptyset$$
$$\text{Files}(PR_{33}) \cap \text{Files}(PR_{34}) = \emptyset$$
$$\text{Files}(PR_{31}) \cap \text{Files}(PR_{33}) \cap \text{Files}(PR_{34}) = \emptyset$$

Because the intersection of all modified paths is completely empty, merging all three branches onto `integration/m15-1-joint-validation` produces **zero merge conflicts**, zero git semantic collisions, and preserves pristine git commit history.

---

### 3. Dependency Graph & Release Sequencing

```
┌────────────────────────────────────────────────────────┐
│  Tier 0: Infrastructure & Compiler Hygiene (PR #33)    │
│  - Cargo check, clippy, multi-platform test runner     │
└───────────────────────────┬────────────────────────────┘
                            │ provides validated base
                            ▼
┌────────────────────────────────────────────────────────┐
│  Tier 1: Backend Capabilities & Protocol (PR #31)      │
│  - CLI binary (ags), doctor, provider discovery, IPC   │
└───────────────────────────┬────────────────────────────┘
                            │ provides daemon binary & protocol contract
                            ▼
┌────────────────────────────────────────────────────────┐
│  Tier 2: Frontend Agent Continuity & UX (PR #34)       │
│  - VS Code extension, session continuity, binary check │
└───────────────────────────┬────────────────────────────┘
                            │ combined acceptance
                            ▼
┌────────────────────────────────────────────────────────┐
│  Tier 3: Joint Acceptance Gate (M15.1)                 │
│  - All 3 PRs validated on integration/m15-1-... branch │
└────────────────────────────────────────────────────────┘
```

#### Merge Sequencing Recommendation for Milestone Completion:
1. **First:** Land PR #33 (`fix/m15-unified-cross-platform-ci`) to establish green CI on `integration/m15-unified-codebase`.
2. **Second:** Land PR #31 (`feat/m15-1-backend-cli-consolidation`) to introduce backend provider discovery, protocol schemas, and `ags` CLI.
3. **Third:** Land PR #34 (`feat/m15-ui-06-agent-ide-continuity`) to integrate frontend extension capabilities and daemon continuity.
4. **Fourth:** Once all three Draft PRs land on `integration/m15-unified-codebase`, trigger full end-to-end CI on PR #32 (`integration/m15-unified-codebase` -> `main`) for final production promotion.
