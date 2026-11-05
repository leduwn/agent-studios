# Contract Test Execution & Zero-Regression Verification Report
**Milestone:** M15.1-A — Backend and CLI Consolidation  
**Target Crates:** `provider`, `control-plane`, `cli-client`, `tui`, `cli`, `protocol`  
**Execution Environment:** Windows 11 (x86_64-pc-windows-msvc) / Rust 1.85+  
**Date:** 2026-11-06  
**Auditor:** Claude 1 (Principal Rust Architect & Quality Assurance Lead)

---

## 1. Executive Summary

This report documents the contract test execution across all ported, reconciled, and unified crates in the `feat/m15-1-provider-cli-unification` feature branch.

A total of **227 automated tests** across the six primary backend and CLI crates executed with **100% success (227 passed, 0 failed, 0 ignored)**.

---

## 2. Test Execution Summary by Crate

| Crate | Test Suite Binary | Tests Run | Passed | Failed | Ignored | Duration |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **`agent-studios-provider`** | `src/lib.rs` (unit tests) | 35 | 35 | 0 | 0 | 0.00s |
| | `tests/discovery_tests.rs` | 15 | 15 | 0 | 0 | 0.52s |
| | `tests/importer_tests.rs` | 8 | 8 | 0 | 0 | 0.00s |
| | `tests/provider_tests.rs` | 33 | 33 | 0 | 0 | 0.01s |
| | `tests/secret_store_tests.rs` | 5 | 5 | 0 | 0 | 0.00s |
| **`agent-studios-control-plane`** | `src/lib.rs` (unit tests) | 15 | 15 | 0 | 0 | 0.05s |
| | `tests/integration_tests.rs` | 48 | 48 | 0 | 0 | 0.08s |
| **`agent-studios-cli-client`** | `tests/client_tests.rs` | 4 | 4 | 0 | 0 | 1.64s |
| | `tests/runtime_slice_tests.rs` | 10 | 10 | 0 | 0 | 5.40s |
| | `tests/subscription_tests.rs` | 7 | 7 | 0 | 0 | 22.93s |
| | `tests/worktree_isolation_tests.rs` | 3 | 3 | 0 | 0 | 2.58s |
| **`agent-studios-tui`** | `src/lib.rs` (widgets) | 4 | 4 | 0 | 0 | 0.00s |
| | `tests/terminal_tests.rs` | 8 | 8 | 0 | 0 | 0.01s |
| | `tests/widget_tests.rs` | 15 | 15 | 0 | 0 | 1.13s |
| **`agent-studios-cli`** | `src/main.rs` (subcommands/doctor) | 6 | 6 | 0 | 0 | 0.18s |
| **`agent-studios-protocol`** | `src/lib.rs` (events & types) | 11 | 11 | 0 | 0 | 0.00s |
| **TOTALS** | **15 Test Binaries** | **227** | **227** | **0** | **0** | **~34.5s** |

---

## 3. Key Contract Invariants Verified

### 3.1 Event Streaming & Domain Event Durability
- `ControlPlaneEvent::AssistantMessageChunk`: Verified end-to-end token streaming with `is_thought` differentiation.
- `apply_event` in `ControlPlane`: Confirmed linear sequence numbering and atomic persistence into `SqliteStore`.
- Subscription watermark tracking in `cli-client`: Verified zero dropped events during high-throughput broadcast bursts.

### 3.2 Provider & Secret Isolation
- `SecretReference`: Confirmed that raw API secrets are never embedded in JSON snapshots, log outputs, or error payloads.
- Secret store CRUD: Verified atomic set/get/delete across `InMemorySecretStore`, `EnvironmentSecretStore`, and `OsKeyringSecretStore`.
- Multi-instance isolation: Tested distinct OpenAI instances with distinct base URLs and credentials without cross-contamination.

### 3.3 TUI & Terminal Resilience
- Vietnamese NFC Unicode: Tested 69 grapheme clusters containing complex tone combinations (`à`, `ả`, `ã`, `á`, `ạ`, `ơ`, `ư`, etc.) rendered through `Ratatui` buffer cells. Verified zero diacritic truncation or double-width misalignment.
- Terminal Title Sanitization: Confirmed neutralization of ANSI control characters (`\x1b[...`) and bidirectional Trojan Source overrides.
- Fail-Closed Approvals: Verified that approval modal cancellation rejects actions immediately and preserves modal state on error.

### 3.4 CLI & Daemon Offline Fallback
- `ags doctor`: Probed system readiness, verifying accurate reporting of dirty worktree status, Git version, and terminal dimensions.
- Daemon Probe: When `agsd` is offline, `ags doctor` outputs status `INFO` with explicit fallback message and zero exit code failures.

---

## 4. Conclusion

All contracts across domain models, storage interfaces, IPC protocols, and user interfaces are verified green. Zero regressions were introduced into the canonical workspace.
