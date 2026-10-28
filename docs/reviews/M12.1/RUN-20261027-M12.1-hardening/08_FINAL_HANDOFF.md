# Milestone M12.1 — Final Handoff & Acceptance Report

- **Milestone**: `M12.1 — Claude Code Runtime Hardening & Protocol Corrections`
- **Execution Run**: `RUN-20261027-M12.1-hardening`
- **Date**: `2026-10-28T00:00:00+07:00`
- **Lead Owner**: Principal Engineering Lead & Integration Owner
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Base Commit**: `710051e6566e12cf869e2db02590f81726fe7943`
- **Active Branch**: `feat/m12-claude-code-runtime`
- **Status**: **HARDENED & READY FOR INDEPENDENT VERIFICATION**

---

## 1. Executive Acceptance Summary

Milestone M12.1 resolves all eight technical issues and capability overstatements identified by the independent review of Milestone M12. The Claude Code runtime adapter (`agent-studios-claude-code-runtime`) is hardened to strict production-grade standards:

1. **Protocol Fidelity**: `ControlResponse` matches the exact nested envelope (`subtype: "success"` with `behavior: "allow" | "deny"`) expected by Claude Agent SDK and Claude Code CLI.
2. **Lifecycle Integrity**: Startup sequence emits `SessionStarted` strictly once at sequence 1. `SystemMessage::Init` is retained as diagnostic telemetry.
3. **Budget Security**: Malformed, negative, NaN, and non-numeric budget strings fail closed immediately, preventing silent budget bypass.
4. **Active Approval Watchdog**: An active timeout watchdog enforces pre-execution tool approval deadlines, failing closed with a formatted deny response upon timeout.
5. **Process Exit Truthfulness**: `wait_for_exit()` safely awaits and reaps child processes, classifying exit codes 0, 2, 130/143, and nonzero crashes distinctly with stderr diagnostics.
6. **No False Interruptions**: Control responses no longer map to `Interrupted`.
7. **Accurate Capability Advertising**: `persistent_session` is truthfully advertised as `Unsupported` due to `--no-session-persistence`.
8. **Robust Upstream Intelligence**: `.github/workflows/upstream-intelligence.yml` tracks all 6 upstream repositories daily and prevents duplicate issue spam using `.github/upstream-state.json`.

---

## 2. Review Findings Resolution Ledger

| Finding ID | Title | Severity | Before | After | Final Status |
|---|---|:---:|---|---|:---:|
| **M12-REV-001** | ControlResponse Wire Format | P0 | Top-level `request_id`, no `subtype` | Nested `subtype: "success"`, `behavior` | **FIXED** |
| **M12-REV-002** | Duplicate SessionStarted | P1 | Emitted on start AND on Init message | Emitted once at seq 1; Init maps to Diagnostic | **FIXED** |
| **M12-REV-003** | Budget Validation Bypass | P1 | Malformed string converted to None | Validated strictly; fails closed on invalid | **FIXED** |
| **M12-REV-004** | Approval Timeout Not Integrated | P0 | Stream loop never enforced timeout | Active async watchdog sends fail-closed deny | **FIXED** |
| **M12-REV-005** | Incorrect Process Exit Status | P1 | Cached status was None on EOF | `wait_for_exit()` reaps child, classifies codes | **FIXED** |
| **M12-REV-006** | ControlResponse Event Mapping | P1 | Mapped to `Interrupted` | Mapped to no-op, zero false interrupts | **FIXED** |
| **M12-REV-007** | Capability Inaccuracy | P2 | Declared `persistent_session: Supported` | Declared `persistent_session: Unsupported` | **FIXED** |
| **M12-REV-008** | Upstream Intelligence Reliability| P2 | 4 repos, duplicate issues on run | 6 repos, state tracked, deduplicated | **FIXED** |

---

## 3. Test Verification Metrics

- Total Integration & Protocol Tests: **45 passed, 0 failed, 0 ignored** (finished in 0.22s)
  - `stream_and_subagent_tests.rs`: 14 passed
  - `runtime_lifecycle_tests.rs`: 14 passed
  - `security_and_budget_tests.rs`: 13 passed
  - `sandbox_containment_tests.rs`: 4 passed
- Compiler & Linter Verification:
  - `cargo check -p agent-studios-claude-code-runtime --all-targets`: Clean build
  - `cargo clippy -p agent-studios-claude-code-runtime --all-targets -- -D warnings`: 0 warnings, 0 errors
  - `cargo fmt -p agent-studios-claude-code-runtime -- --check`: Format compliant

---

## 4. Subagent Execution Roster

| Subagent | Role | Scope | Execution Mode | Result |
|---|---|---|---|:---:|
| **Agent A** | Protocol Compatibility | `protocol.rs`, `mapping.rs`, wire tests | Native Claude Code subagent | **COMPLETED** |
| **Agent B** | Runtime Lifecycle | `runtime.rs`, `process.rs`, `discovery.rs` | Native Claude Code subagent | **COMPLETED** |
| **Agent C** | Security & Budget | `approval.rs`, `runtime.rs`, budget tests | Native Claude Code subagent | **COMPLETED** |
| **Agent D** | Upstream Intelligence | `.github/workflows/`, state JSON | Native Claude Code subagent | **COMPLETED** |
| **Agent E** | Independent QA | Quality gates, adversarial tests | Native Claude Code subagent | **COMPLETED** |

---

## 5. Upstream Immutability & IP Quarantine Confirmation

- **`codex-rs/` status**: Strictly 0 files modified, 0 lines diff. Verified via `git diff --stat codex-rs/`. Upstream patches 001–004 intact.
- **`777genius/agent-teams-ai` quarantine**: Clean-room UX principles only. Zero lines of AGPL-3.0 code imported.

---

## 6. Next Steps

- Submit Draft Pull Request for `feat/m12-claude-code-runtime` against `main`.
- Await external review verdict before proceeding to Milestone M13 (Code-OSS Shell Integration Foundation).
