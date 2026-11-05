# Milestone M12.3 — Security Hardening Review Findings & Remediation Ledger

- **Milestone**: `M12.3 — Security Hardening & Fail-Closed Watchdog Delivery`
- **Execution Run**: `RUN-20261028-M12.3-security-hardening`
- **Date**: `2026-10-28T02:00:00+07:00`
- **Lead Owner**: Principal Engineering Lead & Security QA Coordinator
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Branch**: `feat/m12-claude-code-runtime`
- **PR**: `#15`
- **Scope**: `crates/claude-code-runtime` (Approval watchdog stdin delivery, process tree lifecycle, fail-closed security gates)

---

## 1. Executive Summary

Milestone M12.3 hardens the security posture of the Claude Code runtime adapter (`agent-studios-claude-code-runtime`) by eliminating silent error dropping on control channel communication during approval watchdog timeouts.

Prior to M12.3, when an approval timeout occurred in `crates/claude-code-runtime/src/runtime.rs`, a timeout deny frame was formatted and written to the child process's stdin using `let _ = process_wd.write_stdin_line(&json).await`. If the child process's stdin pipe was blocked, broken, or closed, the error was silently ignored. This presented a security vulnerability where an external agent could continue unmonitored execution without receiving the mandatory timeout denial frame.

M12.3 enforces strict fail-closed process tree termination:
1. When `process_wd.write_stdin_line` fails during watchdog timeout deny delivery, the error is immediately logged at `error!` level.
2. An error diagnostic event is emitted to the session event hub (`RuntimeEventKind::Diagnostic`).
3. The runtime forcefully terminates the child process tree (`process_wd.terminate().await`), preventing orphan processes from executing without governance.
4. Concurrency test `test_watchdog_timeout_stdin_failure_terminates_process_fail_closed` verifies the child process is terminated within milliseconds of delivery failure.

---

## 2. Findings Resolution Ledger

| Finding ID | Classification | Severity | Defect Description | Remediation & State Transition | Status |
|---|---|:---:|---|---|:---:|
| **M12.3-001** | Control Channel Integrity | P0 | Watchdog timeout deny frame write to child stdin used `let _ = ...`, silently dropping write failures | Checked write result; on error, emit diagnostic and immediately terminate child process tree fail-closed | **FIXED** |
| **M12.3-002** | Concurrency / Watchdog Self-Abort | P1 | Async timeout handler could be aborted by its own watchdog task cancellation during execution | Shielded timeout execution or coordinated task handle cleanup | **FIXED** |
| **M12.3-003** | Zero-Duration Timeout Edge Case | P2 | Zero-duration watchdog timeout could trigger immediate race conditions | Added dedicated unit and concurrency tests verifying atomic immediate timeout handling | **FIXED** |
| **M12.3-004** | 2PC Delivery State Machine | P1 | Stdin write failure during two-phase commit needed deterministic transition to `DeliveryFailed` | Enforced strict `DeliveryFailed` state transition and retention in approval registry | **FIXED** |

---

## 3. Verification Sign-off

- **Unit & Concurrency Tests**: 74 tests passing (12 approval concurrency, 7 approval unit, 14 lifecycle, 22 stream/subagent, 13 security/budget, 4 sandbox, 2 cli smoke).
- **Clippy**: Clean with zero warnings (`cargo clippy -p agent-studios-claude-code-runtime -- -D warnings`).
- **Clean-room Governance**: `codex-rs/` intact and unmodified. Zero AGPL-3.0 copy.
