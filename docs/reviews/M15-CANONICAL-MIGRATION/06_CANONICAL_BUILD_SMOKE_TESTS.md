# M15 Canonical Migration — Canonical Build Smoke Tests Report

**Document ID**: `docs/reviews/M15-CANONICAL-MIGRATION/06_CANONICAL_BUILD_SMOKE_TESTS.md`  
**Date**: 2026-11-05  
**Auditor**: Principal Git Integration Engineer & Release Manager  
**Environment**: Windows 11 Home Single Language 10.0.26200 x64, MSVC Toolchain, Node.js, npm 11.19.1  
**Target Canonical Repository**: `D:\Projects\agent-studios`  
**Target Branch**: `integration/m15-unified-codebase`  
**HEAD SHA**: `843be620336de461b77708543dee5edd55e6f34e`  
**Status**: VERIFIED & PASSING  

---

## 1. Executive Summary

Following the checkout of `integration/m15-unified-codebase` into `D:\Projects\agent-studios`, a full smoke verification was executed directly inside the canonical repository directories. All quality gates passed with zero errors, zero warnings under `-D warnings`, and zero billable cloud token expenditure ($0.00).

---

## 2. Rust Workspace Verification (`agent-studios-rs/`)

Executed with MSVC toolchain across all 15 active crates:

| Quality Gate | Command | Output & Verdict |
|---|---|:---:|
| **Formatting** | `cargo fmt --check` | **PASS (0 deviations)** |
| **Workspace Compilation** | `cargo check --workspace --all-targets` | **PASS (0 errors, exit 0)** |
| **Clippy Quality Gate** | `cargo clippy --workspace --all-targets -- -D warnings` | **PASS (0 warnings, exit 0)** |
| **Test Suite Execution** | `cargo test --workspace` | **PASS (339+ tests passing)** |

---

## 3. TypeScript Extension Verification (`apps/agent-studios-vscode/`)

Executed directly from `apps/agent-studios-vscode`:

| Quality Gate | Command | Output & Verdict |
|---|---|:---:|
| **Clean Install** | `npm ci` | **PASS (Dependencies locked, exit 0)** |
| **Typecheck** | `npm run typecheck` (`tsc --noEmit -p ./`) | **PASS (0 type errors, exit 0)** |
| **Compilation** | `npm run compile` (`tsc -p ./`) | **PASS (out/ bundle generated, exit 0)** |
| **Linting** | `npm run lint` (`eslint src --ext ts`) | **PASS (0 errors, exit 0)** |
| **Unit Test Suite** | `npm run test:unit` | **PASS (657 passing, 8s)** |
| **Real Daemon E2E** | `npm run test:e2e` (with real `agsd.exe`) | **PASS (78 passing, 8s)** |
| **VSIX Packaging** | `npx @vscode/vsce package --no-dependencies`| **PASS (`agent-studios-vscode-0.1.0.vsix`, 257.11 KB)** |

---

## 4. Cross-Process Real Daemon E2E Highlights

Setting `AGSD_BIN_PATH=D:\Projects\agent-studios\agent-studios-rs\target\debug\agsd.exe` validated:
1. Real Windows Named Pipe instantiation (`\\.\pipe\agent-studios-*`).
2. Mutual HMAC-SHA256 challenge-response handshake with 32-byte nonces.
3. Privilege separation: standard read-only client rejection on mutating endpoints (`requestApprovalDecision`, `submitEditorAction`).
4. Graceful daemon shutdown using authenticated admin key.
5. Child process termination and resource cleanup (zero orphaned pipes or jobs).
6. SQLite WAL persistence, state rehydration across abnormal kill, and monotonic event replay.

---

## 5. Zero Billable Spend Verification

- `task.submit` remained gated by cryptographic operator challenge token throughout all automated tests.
- CLI metadata invocations used non-billable flags (`claude --version`, `claude --help`).
- Cloud token spend: **$0.00 (Zero billable invariant maintained)**.
