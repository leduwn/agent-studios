# Milestone AGS CLI 1.3 — Security & Reliability Review Findings

- **Milestone**: `AGS CLI 1.3 Security & Reliability Gate + M13 Preparation`
- **Execution Run**: `RUN-20261028-CLI-1.3-gate`
- **Date**: `2026-10-28T07:15:00+07:00`
- **Lead Owner**: Principal Engineering Lead & Security QA Coordinator
- **PR**: `#16` (`https://github.com/leduwn/agent-studios/pull/16`)
- **Base PR (Independent Audit)**: `#15` (`https://github.com/leduwn/agent-studios/pull/15`, HEAD `3207b7462f`)
- **Status**: **ALL P1/P2 FINDINGS RESOLVED & INDEPENDENTLY AUDITED**

---

## 1. Executive Summary & Quality Gate Matrix

Milestone 1.3 hardens PR #16 against concurrency, race conditions, physical worktree leakage, and lifecycle orphan states, while independently auditing PR #15 (M12.3) security invariants.

| Finding ID | Classification | Severity | Description | Resolution & Evidence | Status |
|---|---|:---:|---|---|:---:|
| **CLI-1.2-R01** | Git Workspace Isolation | **P1** | `EmbeddedClient` registered metadata-only worktree with `root = cwd` and `base_sha = "HEAD"`; `root == source_root` permitted mutating execution | Integrated physical worktree allocation boundary; enforced that `root == source_root` fails closed under `WorkspaceAccessMode::Mutating` in `opencode-runtime` and `cli-client` | **FIXED** |
| **CLI-1.2-R02** | Transactional Startup | **P1** | `EmbeddedClient::submit_goal` left tasks/runs stranded in `Running` when discovery, worktree allocation, or runtime start failed | Added `fail_task_and_run` transactional rollback handler transitioning task & run to `Failed` with diagnostic error chunks and synchronized events | **FIXED** |
| **CLI-1.2-R03** | Approval Session Correlation | **P1** | Approval routing selected `active_sessions.last()`, causing cross-session misrouting in concurrent/multi-agent environments | Implemented `approval_sessions: Arc<RwLock<HashMap<ApprovalId, RuntimeSessionId>>>`; resolves approvals strictly to originating runtime session; fails closed for unknown/stale approvals | **FIXED** |
| **CLI-1.2-R04** | Access Policy Enforcement | **P1** | `validate_session_workspace` ignored `WorkspaceAccessMode`, leaving read-only boundaries unenforced | Enforced explicit `WorkspaceAccessMode` checks: `SharedSource` permitted for `ReadOnly` inspection but strictly rejected for `Mutating`; `Managed` with identical source/root rejected for `Mutating` | **FIXED** |
| **CLI-1.2-R05** | Event Stream Linearizability | **P2** | Historical snapshot race and sequence gap handling lacked monotonic verification and unrecovered gap protection | Maintained broadcast-before-snapshot registration; added authoritative sequence gap recovery, duplicate suppression, unrecoverable gap logging, and terminal event preservation | **FIXED** |
| **CLI-1.2-R06** | Complete Lifecycle Cleanup | **P2** | Active runtime session references were not cleared on stop/cancel; Ctrl+C handling silently dropped action failures | Cleaned up session and approval mappings on termination; refactored two-stage Ctrl+C with visible transcript error reporting on cancellation failures | **FIXED** |
| **CLI-1.2-R07** | Graceful TUI Onboarding | **P2** | Running `ags` in production without configured AI runtime backend exited abruptly or simulated fake execution | Created `OnboardingApp` and `OnboardingWidget` guiding developers to configure runtime backends, run diagnostics (`doctor`), or launch `--demo` / `--preview` | **FIXED** |

---

## 2. Deep Dive: Technical Findings & Remediation

### 2.1. CLI-1.2-R01 & CLI-1.2-R04: Real Git Worktree Isolation & Access Policy

- **Vulnerability**: If an external agent runtime executed in `Mutating` mode against a workspace where `worktree_root == source_root`, any unverified file edits directly altered the developer's working directory, bypassing Git branch isolation.
- **Remediation**:
  In `crates/opencode-runtime/src/control_plane.rs`:
  ```rust
  if access_mode == WorkspaceAccessMode::Mutating {
      let same_path = match (root.canonicalize(), source_root.canonicalize()) {
          (Ok(cr), Ok(cs)) => cr == cs,
          _ => root == source_root,
      };
      if same_path {
          return Err(OpenCodeError::WorktreeIsolationViolation(format!(
              "Managed worktree root '{}' equals source root '{}' under Mutating access mode; physical worktree isolation required",
              root.display(),
              source_root.display()
          )));
      }
  }
  ```
  And for `ExecutionWorkspace::SharedSource`:
  ```rust
  ExecutionWorkspace::SharedSource { cwd } => {
      if access_mode == WorkspaceAccessMode::Mutating {
          return Err(OpenCodeError::WorktreeIsolationViolation(
              "ExecutionWorkspace::SharedSource is strictly prohibited for Mutating access mode to protect repository integrity".into(),
          ));
      }
      let canonical_cwd = cwd.canonicalize().map_err(...)?;
      Ok(canonical_cwd)
  }
  ```

### 2.2. CLI-1.2-R02: Transactional Startup & Rollback (`fail_task_and_run`)

- **Defect**: When runtime discovery, model mapping, or session startup failed, the Control Plane task and run remained in `Running` or `Starting` state indefinitely.
- **Remediation**:
  `EmbeddedClient::fail_task_and_run` atomically records an assistant diagnostic chunk, records the run outcome, transitions run to `Failed`, transitions task to `Failed`, and flushes all events to the event history and broadcast channel.

### 2.3. CLI-1.2-R03: Authoritative Approval-to-Session Correlation

- **Defect**: Calling `active_sessions.last()` routed approval decisions to whatever session happened to be started most recently.
- **Remediation**:
  Every pending approval registers a mapping `(ApprovalId -> RuntimeSessionId)`. Resolving an approval queries this correlation map. If the approval ID is unknown or belongs to a session that has already terminated, resolution fails closed with an informative error rather than broadcasting to an arbitrary child.

### 2.4. CLI-1.2-R05: Event Stream Linearizability & Gap Recovery

- **Defect**: Broadcast channel lag or snapshot races could drop events or deliver events out of order.
- **Remediation**:
  - `subscribe_events` subscribes to the broadcast channel *before* capturing the history snapshot.
  - `deliver_events_with_continuity_check` ensures contiguous monotonic delivery (`sequence == last_delivered + 1`).
  - Gaps trigger authoritative history queries. Unrecoverable gaps freeze watermark advancement while ensuring terminal lifecycle events reach the consumer.
