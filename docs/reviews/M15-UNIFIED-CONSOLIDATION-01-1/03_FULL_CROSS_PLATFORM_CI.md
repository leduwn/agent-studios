# 03 — Full Cross-Platform CI Specification: M15 Consolidation 01.1

**Document Reference**: `docs/reviews/M15-UNIFIED-CONSOLIDATION-01-1/03_FULL_CROSS_PLATFORM_CI.md`  
**Milestone**: M15 Consolidation & Canonical Preflight  
**Integration Branch**: `integration/m15-unified-codebase`  
**Workflow File**: `.github/workflows/agent-studios-unified-ci.yml`  
**Auditor**: Lead Integration Engineer & QA Automation Lead  

---

## 1. Executive Summary

This specification establishes the architecture of the unified cross-platform CI pipeline (`agent-studios-unified-ci.yml`), guaranteeing that the entire Agent Studios repository (Rust backend, Claude Code runtime, VS Code extension, and E2E integration suites) builds and passes automated testing on a single unified commit SHA across both primary runners (`ubuntu-latest` and `windows-latest`).

Key CI innovations and fixes:
1. **Elimination of Split-Brain Backend Pinned Checkouts**:
   - In previous iterations (`agent-studios-m14-ui-ci.yml`), frontend testing executed a custom bash step fetching and checking out a legacy pinned backend commit `dcbcb39bf6`.
   - The unified workflow eliminates this split-brain mechanism completely: both backend and frontend build directly from `${{ github.workspace }}` at the exact checked-out commit SHA (`${{ github.sha }}`).
2. **Dual-OS Compilation & Native Daemon Verification**:
   - Compiles native `agsd` daemon binary in both debug and release configurations.
   - Automatically exports `AGSD_BIN_PATH` into `$GITHUB_ENV` to drive E2E test suites with zero hardcoded paths.
3. **Zero Billable Model Invariant ($0.00 spent)**:
   - All tests operate exclusively against mock adapters or non-billable CLI metadata inspection (`claude --version`). Zero cloud API tokens are requested or spent.
4. **Complete Extension Host & VSIX Packaging**:
   - Includes full headless VS Code Extension Host integration tests via `xvfb-run -a` on Linux and direct execution on Windows.
   - Packages official VSIX distribution bundle using `npx @vscode/vsce package --no-dependencies`.

---

## 2. CI Pipeline Step-by-Step Architecture

```
[GitHub Trigger: push / pull_request]
          │
          ▼
┌────────────────────────────────────────────────────────┐
│  Matrix Runners: ubuntu-latest & windows-latest       │
└─────────────────────────┬──────────────────────────────┘
                          │
  ┌───────────────────────┴───────────────────────┐
  ▼                                               ▼
[Step 1: actions/checkout@v4]                   [Step 2: Commit SHA Verification]
  - fetch-depth: 0                                - Assert: HEAD SHA == github.sha
  - submodules: true                              - Prohibit split-brain checkout
                          │
  ┌───────────────────────┴───────────────────────┐
  ▼                                               ▼
[Step 3: Rust Toolchain & Cache]                [Step 4: Rust Code Quality Gates]
  - dtolnay/rust-toolchain (stable)               - cargo fmt --check (Ubuntu)
  - Swatinem/rust-cache@v2                        - cargo check --workspace --all-targets
                                                  - cargo clippy --workspace --all-targets
                          │
  ┌───────────────────────┴───────────────────────┐
  ▼                                               ▼
[Step 5: Rust Workspace Tests]                  [Step 6: Native agsd Build & Export]
  - cargo test --workspace (339+ tests)           - cargo build --bin agsd (Debug)
                                                  - cargo build --release --bin agsd (Release)
                                                  - Export AGSD_BIN_PATH to $GITHUB_ENV
                          │
  ┌───────────────────────┴───────────────────────┐
  ▼                                               ▼
[Step 7: Node.js 22 & NPM CI]                   [Step 8: TypeScript Quality Gates]
  - actions/setup-node@v4 (node 22)               - npm run lint (ESLint 0 errors)
  - npm ci (in apps/agent-studios-vscode)         - npm run typecheck (tsc --noEmit)
                                                  - npm run build (tsc compilation)
                          │
  ┌───────────────────────┴───────────────────────┐
  ▼                                               ▼
[Step 9: Extension Unit Tests]                  [Step 10: Real Daemon E2E Integration]
  - npm run test:unit (657 tests)                 - npm run test:e2e (78 tests)
                                                  - Dynamic AGSD_BIN_PATH IPC tests
                          │
  ┌───────────────────────┴───────────────────────┐
  ▼                                               ▼
[Step 11: VS Code Host Integration]             [Step 12: Packaging & Zero Billable Gate]
  - Ubuntu: xvfb-run -a npm run test:integration  - npx @vscode/vsce package --no-dependencies
  - Windows: npm run test:integration             - Validate $0.00 token expenditure
```

---

## 3. Runner-Specific Execution Matrix

| Stage | Ubuntu Runner (`ubuntu-latest`) | Windows Runner (`windows-latest`) | Expected Result |
|---|---|---|:---:|
| **Commit Check** | `git rev-parse HEAD == github.sha` | `git rev-parse HEAD == github.sha` | Exact Match |
| **Rust Formatting** | `cargo fmt --check` | (Skipped for cross-platform newline parity)| PASS |
| **Rust Cargo Check**| `cargo check --workspace --all-targets` | `cargo check --workspace --all-targets` | 0 errors |
| **Rust Clippy** | `cargo clippy --workspace -- -D warnings` | `cargo clippy --workspace -- -D warnings` | 0 warnings |
| **Rust Workspace Tests**| `cargo test --workspace` | `cargo test --workspace` | 339+ PASS |
| **Native Daemon Build** | `cargo build --release --bin agsd` | `cargo build --release --bin agsd` | Binary compiled |
| **TS Lint & Typecheck**| `npm run lint && npm run typecheck` | `npm run lint && npm run typecheck` | 0 errors |
| **TS Unit Tests** | `npm run test:unit` | `npm run test:unit` | 657 PASS |
| **Real Daemon E2E** | `npm run test:e2e` (with `AGSD_BIN_PATH`) | `npm run test:e2e` (with `AGSD_BIN_PATH`) | 78 PASS |
| **Extension Host Tests**| `xvfb-run -a npm run test:integration` | `npm run test:integration` | PASS |
| **VSIX Packaging** | `vsce package --no-dependencies` | `vsce package --no-dependencies` | `.vsix` built |
| **Zero Billable Invariant**| Assert $0.00 spent | Assert $0.00 spent | PASS |
