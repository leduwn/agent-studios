# Milestone M15 UI-01.2 Review: Backend CI Provenance Audit

**Milestone**: M15 UI-01.2 — Real Rust Execution Integration & Final Safety Gates  
**Area**: Pinned Backend CI Provenance & Dual-Runner Native Compilation  
**Verdict**: APPROVED  

---

## 1. Executive Summary

Milestone M15 UI-01.2 establishes strict, immutable Git SHA provenance for the compiled Rust daemon (`agsd`) used in CI. The legacy M13 commit (`ce0396c1f165760ea271110eaf9bfd6dc84a67a7`) has been completely eradicated from the workflow configuration and replaced with the authoritative approved PR #26 backend commit `a5dbbfc5ebada254727db9a788c0ee40e3035ea8`.

The GitHub Actions workflow `.github/workflows/agent-studios-m14-ui-ci.yml` enforces fail-closed SHA verification without moving branch fallback on both Ubuntu (`ubuntu-latest`) and Windows (`windows-latest`) runners.

---

## 2. Provenance Architecture & Wire Integrity

### 2.1 Pinned Commit Specification

| Metric | Target Specification | Status |
|--------|----------------------|--------|
| Target SHA | `a5dbbfc5ebada254727db9a788c0ee40e3035ea8` (Approved PR #26) | Verified |
| Legacy SHA | `ce0396c1f165760ea271110eaf9bfd6dc84a67a7` (Deprecated M13) | Eradicated |
| Moving Branch Fallback | Strictly Prohibited (`exit 1` on fetch failure) | Verified |
| Rust Cache Key | Pinned to `a5dbbfc5ebada254727db9a788c0ee40e3035ea8` | Verified |
| Native Daemon Binary | Compiled via `cargo build --bin agsd -p agent-studios-ipc-transport` | Verified |

### 2.2 GitHub Actions Provenance Enforcement

```yaml
- name: Checkout pinned Rust backend (M15 IPC agsd)
  working-directory: ${{ github.workspace }}
  shell: bash
  run: |
    PINNED_SHA="a5dbbfc5ebada254727db9a788c0ee40e3035ea8"
    echo "Fetching pinned backend commit: $PINNED_SHA"
    git fetch --depth=1 --force origin "$PINNED_SHA":backend-pin || {
      echo "::error::Failed to fetch pinned backend commit $PINNED_SHA. Branch fallback disabled."
      exit 1
    }
    ACTUAL_SHA=$(git rev-parse backend-pin)
    echo "Asserting exact SHA provenance: $ACTUAL_SHA == $PINNED_SHA"
    [ "$ACTUAL_SHA" = "$PINNED_SHA" ] || {
      echo "::error::SHA provenance assertion failed: expected $PINNED_SHA, got $ACTUAL_SHA"
      exit 1
    }
    git checkout backend-pin -- agent-studios-rs/
```

### 2.3 Binary Discovery Path Hygiene

Both `RealRustDaemonE2E.test.ts` and `LiveMissionControlE2E.test.ts` have been purged of legacy references to `agent-studios-m13`. Candidate resolution prioritizes:
1. `process.env.AGSD_BIN_PATH` (injected by CI workflow runner).
2. Workspace-relative `agent-studios-rs/target/debug/agsd` (or `.exe` on Windows).
3. Approved M15 workspace sibling directory (`agent-studios-m15`).

---

## 3. Verification & CI Dual-Platform Readiness

- **Ubuntu Runner (`ubuntu-latest`)**: Compiles native ELF binary at `agent-studios-rs/target/debug/agsd`, sets executable permissions (`chmod +x`), and executes Tier 3 E2E test suite against Unix domain socket endpoint.
- **Windows Runner (`windows-latest`)**: Compiles native PE binary at `agent-studios-rs/target/debug/agsd.exe` and executes Tier 3 E2E test suite over Windows Named Pipe endpoint (`\\.\pipe\...`).
- **No Mock Fallback in Tier 3**: Real compiled `agsd` binary execution is required and non-skippable in CI.
