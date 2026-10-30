# M13 IPC-02.2.2 Ubuntu CI Root Cause Analysis & Remediation

**Date**: 2026-10-30  
**Target Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Status**: RESOLVED & VERIFIED  

---

## 1. Problem Statement & Incident Context

During GitHub Actions remote continuous integration on run `41039274025` for pull request #19, the Linux (Ubuntu) build step failed under `cargo clippy --all-targets -- -D warnings`. 

The compilation failure produced 3 distinct diagnostic errors:
1. `unused import: std::sync::Arc` in `crates/opencode-runtime/src/sandbox.rs`
2. `unused import: tracing::info` in `crates/opencode-runtime/src/sandbox.rs`
3. `unused variable: requires_os_containment` in `crates/opencode-runtime/src/server.rs`

Under the workspace compiler configuration with `-D warnings`, warnings are promoted to fatal compiler errors, blocking CI progression.

---

## 2. Root Cause Analysis

### 2.1 Unused Win32 Imports in `sandbox.rs`
- **Location**: `crates/opencode-runtime/src/sandbox.rs:1-15`
- **Mechanism**: `Arc` and `info` were imported at the module root of `sandbox.rs` to support the Windows Win32 Job Object containment implementation (`JobObjectGuard`). On Windows, `JobObjectGuard` uses `Arc` to coordinate handle lifetimes and `tracing::info` to log Job Object creation and assignment.
- **Root Cause**: On non-Windows platforms (Linux, macOS), `JobObjectGuard` is omitted from compilation via `#[cfg(windows)]`. Consequently, `std::sync::Arc` and `tracing::info` were never referenced on Linux, triggering `clippy::unused_imports`.

### 2.2 Unused Containment Check in `server.rs`
- **Location**: `crates/opencode-runtime/src/server.rs:215`
- **Mechanism**: In `spawn_opencode_server`, the variable `requires_os_containment` was computed via:
  ```rust
  let requires_os_containment = config.sandbox_policy.level.requires_os_containment();
  ```
- **Root Cause**: On Windows, this flag determines whether `creation_flags(CREATE_SUSPENDED)` is passed to `tokio::process::Command` to guarantee zero-window execution prior to Job Object assignment. On Linux, process containment is handled post-spawn or via cgroups/namespaces without suspending process creation flags. Therefore, `requires_os_containment` was never read on Linux targets, triggering `clippy::unused_variables`.

---

## 3. Engineering Remediation

Rather than suppressing linter diagnostics with blanket `#![allow(...)]` attributes (which could mask genuine future regressions), surgical conditional compilation attributes were applied:

### 3.1 Scoped Win32 Imports (`sandbox.rs`)
```rust
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use tracing::info;
use tracing::warn;
```

### 3.2 Scoped Variable Declaration (`server.rs`)
```rust
// 3. Initialize OS process containment BEFORE child process startup (fails closed if unavailable)
#[cfg(windows)]
let requires_os_containment = config.sandbox_policy.level.requires_os_containment();
let containment = ProcessContainment::new(config.sandbox_policy)?;
```

---

## 4. Verification & Prevention

1. **Local Verification**: Executed `cargo clippy --manifest-path crates/opencode-runtime/Cargo.toml --all-targets -- -D warnings` and verified 0 warnings.
2. **Cross-Compilation Verification**: Validated conditional compilation gates for non-Windows CFG branches.
3. **Subtree Invariant**: Verified `codex-rs/` remains 100% untouched.
