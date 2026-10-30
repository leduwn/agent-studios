## Description

<!-- Provide a concise description of the changes in this pull request -->

## Stacked PR Dependency Hierarchy

This pull request is part of the Agent Studios stacked PR architecture. Dependencies must be reviewed and merged in topological order:

- **Track 1: Control Plane & IPC Architecture**
  - `PR #17` (M11 Foundation / Baseline)
    - └── `PR #18` (M12 Claude Code Runtime & IPC Protocol)
      - └── `PR #19` (`feat/m13-ipc-02-control-plane` — IPC Server Hardening, Authorization & Runtime Bridge)

- **Track 2: Provider Management & Discovery**
  - `PR #16` (Core Provider Abstractions)
    - └── `PR #20` (`feat/provider-management-v1` — Provider Catalog, Secure Secret Storage & Model Discovery V1.2)

- **Track 3: Client & Extension (Peer Workspace)**
  - `PR #21` (`feat/vscode-extension` — VS Code Extension Host Hardening; strictly read-only peer track)

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
