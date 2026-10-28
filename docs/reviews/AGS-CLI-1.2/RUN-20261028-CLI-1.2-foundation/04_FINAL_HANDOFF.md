# Milestone AGS CLI Foundation 1.2 — Final Handoff & Release Gate Sign-Off

- **Milestone**: `AGS CLI Foundation 1.2 — Truthful Multi-Agent Runtime & Resilient TUI`
- **Execution Run**: `RUN-20261028-CLI-1.2-foundation`
- **Date**: `2026-10-28T02:45:00+07:00`
- **Lead Owner**: Principal Engineering Lead & Release Gate Owner
- **PR**: `#16` (`https://github.com/leduwn/agent-studios/pull/16`)
- **Status**: **APPROVED FOR MERGE REVIEW (NO SELF-MERGE)**

---

## 1. Release Verification Matrix

- [x] **CLI-1.2-001**: Truthful AI runtime discovery, no simulated fake tool events, explicit `--demo`/`--preview` flags.
- [x] **CLI-1.2-002**: Legitimate Control Plane worktree registration (`create_worktree`, `assign_worktree`) with tracked `ExecutionWorkspace::managed`.
- [x] **CLI-1.2-003**: Linearizable event subscription handoff with broadcast-before-snapshot registration and monotonic sequence deduplication.
- [x] **CLI-1.2-004**: Accurate terminal lifecycle ingestion and exit status reaping.
- [x] **CLI-1.2-005**: Typed fail-closed approval bridge requiring active runtime session.
- [x] **TUI Polish**: Streaming chunk coalescing, scroll lock bottom pinning, two-stage Ctrl+C, Vietnamese NFC graphemes, `"tokens: unknown"` status.
- [x] **Quality Gate**: 39/39 tests passing, 0 clippy warnings.
- [x] **Governance**: No self-merge on PR #16; staged for peer review.
