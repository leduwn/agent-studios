## Description

<!-- Provide a concise description of the changes in this pull request -->

## Stacked PR Dependency Hierarchy

<!-- Specify upstream/downstream PR dependencies in topological merge order -->
- **Base Branch**: `main` (or parent feature branch)
- **Dependent PRs**: None / `PR #...` (upstream dependencies that must be merged prior to this PR)
- **Downstream PRs**: None / `PR #...` (child branches stacked on top of this PR)

---

## Security Invariants Checklist

All changes in this pull request MUST adhere to the following strict security invariants:

- [ ] **No Secrets Leakage**: Zero bearer tokens, API keys, or raw HMAC credentials committed to git, fixtures, test logs, or debug representations.
- [ ] **Bootstrap & Socket Security**:
  - Unix: Atomic restrictive permissions (`0o600` on bootstrap files, `0o700` on socket runtime directories) via single-syscall `OpenOptionsExt`.
  - Windows: Explicit single-ACE DACL granting access solely to current user SID via `CreateFileW` and Win32 security attributes.
  - Symlink / Reparse Point Protection: Target paths verified via `symlink_metadata()` before open/write to eliminate symlink hijacking.
- [ ] **Fail-Closed Authorization**:
  - Out-of-band and untracked approval resolutions strictly require `ClientRole::Admin`.
  - Missing origin entries evaluate to `is_foreign = true`.
- [ ] **Authoritative Runtime Lifecycle Execution**:
  - No synthetic task success. Tasks only transition to `Succeeded` upon verified receipt of `RuntimeEventKind::Completed` from an active `AgentRuntime`.
  - Unconfigured runtime, failed discovery, or runtime start failure fail closed truthfully to `TaskState::Failed` / `RunState::Failed`.
- [ ] **Truthful Provider Diagnostics**:
  - HTTP 429 Too Many Requests responses report `CheckState::RateLimited` (never false `Passed`).
- [ ] **Subtree Integrity**:
  - Subtree `codex-rs/` is 100% untouched (0 byte changes).
- [ ] **Draft PR & No Self-Merge Policy**:
  - This PR must remain in Draft status until approved. Self-merging is strictly prohibited.

---

## Verification & QA Evidence

- [ ] Linux CI / Unit Tests: `cargo test` passing.
- [ ] Windows CI / Unit Tests: `cargo test` passing.
- [ ] Clippy Linting: Zero warnings under `cargo clippy -- -D warnings`.
- [ ] Subagent Independent Review: Independent Claude Code QA subagent review completed and logged in `docs/reviews/`.
