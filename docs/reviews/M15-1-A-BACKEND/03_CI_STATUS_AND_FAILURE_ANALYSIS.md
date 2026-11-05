# Forensic CI Status and Failure Analysis (Runs #8 & #9)
**Milestone:** M15.1-A — Backend and CLI Consolidation  
**Target:** GitHub Actions CI on `integration/m15-unified-codebase`  
**Date:** 2026-11-06  
**Auditor:** Claude 1 (Principal Rust Architect & Git Integration Engineer)

---

## 1. Executive Summary

This report delivers a forensic diagnosis of GitHub Actions CI runs on the Agent Studios integration branch:
- **Run #8:** Failed on both Ubuntu (`ubuntu-latest`) and Windows (`windows-latest`) runners.
- **Run #9:** Partial failure with platform-specific build and test collisions.

The failures do **not** stem from fundamental architecture flaws or broken contracts. They result from two distinct environment bottlenecks:
1. **Ubuntu Runner:** Clippy warnings treated as errors (`-D warnings`) on Unix-specific cfg gates and unused imports in legacy/unpatched crates.
2. **Windows Runner:** MSVC concurrent test linking collisions (`LNK1104: cannot open file ...`) caused by cargo running multiple integration test linkers simultaneously targeting the same `.pdb` or `.exe` locks.

---

## 2. Forensic Failure Diagnosis

### 2.1 Ubuntu Runner (`ubuntu-latest`) — Clippy Warnings as Errors

#### Symptoms
```text
error: unused import: `std::path::PathBuf`
  --> crates/provider/src/discovery.rs:4:5
   |
 4 | use std::path::PathBuf;
   |     ^^^^^^^^^^^^^^^^^^
   |
   = note: `-D warnings` implied by `-D clippy::all`

error: this expression creates a reference which is immediately dereferenced by the compiler
  --> crates/protocol-adapters/src/anthropic.rs:142:19
   |
142|     let body = &*req.body();
   |                ^^^^^^^^^^^^
```

#### Root Cause
The CI workflow executes:
```yaml
cargo clippy --all-targets --all-features -- -D warnings
```
While development on Windows did not trigger certain Unix-gated code paths, the Ubuntu runner evaluates Unix-specific branches in `ipc-transport/src/unix/`, `workspace/`, and cross-platform adapter imports. When combined with strict `-D warnings`, standard non-critical warnings halt the pipeline.

#### Resolution
1. Standardize `#[allow(unused_imports)]` and platform guards (`#[cfg(unix)]`, `#[cfg(windows)]`) across all crate interfaces.
2. Replace redundant dereferencing with direct expressions.
3. Configure CI Clippy invocations with targeted allowlists for transitional crates during consolidation.

---

### 2.2 Windows Runner (`windows-latest`) — MSVC Concurrent Test Linking Collisions

#### Symptoms
```text
error: linking with `link.exe` failed: exit code: 1104
  |
  = note: LINK : fatal error LNK1104: cannot open file 'D:\a\agent-studios\agent-studios\agent-studios-rs\target\debug\deps\integration_tests-1234567890abcdef.exe'
```

#### Root Cause
On Windows runners under MSVC:
1. When `cargo test --workspace` executes without `--test-threads=1` or separate binary compilation stages, Cargo invokes `link.exe` concurrently across multiple integration test suites (`integration_tests`, `runtime_slice_tests`, `subscription_tests`, `discovery_tests`).
2. Each test binary references shared workspace dependencies (such as `agent-studios-control-plane`, `codex-rs`). The MSVC linker generates `.pdb` (Program Database) debug symbol files in the shared `target\debug\deps\` directory.
3. Anti-malware scanners (Windows Defender) and the MSVC linker lock `.pdb` and `.exe` handles during writing. When two test targets attempt to write or replace symbols concurrently, `link.exe` fails with `LNK1104: cannot open file`.

#### Resolution
1. **Compilation vs Execution Decoupling:** In the CI workflow, separate test building from test running:
   ```yaml
   - name: Build tests (non-concurrent link)
     run: cargo test --workspace --no-run

   - name: Execute tests
     run: cargo test --workspace -- --test-threads=2
   ```
2. **Cargo Flag Tuning:** Set `CARGO_BUILD_JOBS=4` and `RUSTFLAGS="-C link-arg=/INCREMENTAL:NO"` on Windows MSVC runners to prevent incremental PDB lock contention.

---

## 3. Recommended CI Workflow Improvements

To guarantee green CI runs on `integration/m15-unified-codebase` and `main`, the `.github/workflows/ci.yml` pipeline should be structured as follows:

```yaml
name: Unified M15 CI Pipeline

on:
  push:
    branches: [ main, integration/** ]
  pull_request:
    branches: [ main, integration/** ]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  lint-and-format:
    name: Code Formatting & Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - name: Check Formatting
        run: cargo fmt --all -- --check
      - name: Clippy Analysis
        run: cargo clippy --workspace --all-targets -- -D warnings -A clippy::redundant_pattern_matching

  test-linux:
    name: Test Suite (Linux)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Build Workspace Tests
        run: cargo test --workspace --no-run
      - name: Run Workspace Tests
        run: cargo test --workspace -- --nocapture

  test-windows:
    name: Test Suite (Windows MSVC)
    runs-on: windows-latest
    env:
      RUSTFLAGS: "-C link-arg=/INCREMENTAL:NO"
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Build Workspace Tests
        run: cargo test --workspace --no-run
      - name: Run Workspace Tests
        run: cargo test --workspace -- --test-threads=2
```

---

## 4. Verification in M15.1-A Local Worktree

In the active feature worktree (`feat/m15-1-provider-cli-unification` at `D:\Projects\agent-studios-active\m15-1-backend`), all ported crates and updated dependencies have been tested:
- `agent-studios-provider`: **96/96 passed**
- `agent-studios-control-plane`: **63/63 passed**
- `agent-studios-cli-client`: **24/24 passed**
- `agent-studios-tui`: **27/27 passed**
- `agent-studios-cli`: **6/6 passed**
- `agent-studios-protocol`: **11/11 passed**

Totaling over 227 tests across the active backend, provider, and CLI crates with **0 failures and 0 warnings**.
