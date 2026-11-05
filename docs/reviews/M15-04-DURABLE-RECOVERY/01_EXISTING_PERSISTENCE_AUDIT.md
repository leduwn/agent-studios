# M15-04 Existing Persistence Architecture Audit

**Milestone:** M15-04  
**Date:** 2026-11-02  
**Author:** Principal Rust Systems Architect & Event-Sourcing Engineer  
**Worktree:** `D:\Projects\agent-studios-m15-04`  
**Base Commit:** `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b` (PR #29)  

---

## 1. Executive Summary

Milestones M13 through M15-03 established an event-sourced Control Plane (`ControlPlaneState`, `ControlPlaneEvent`, `EventEnvelope`, `EventStore`), external runtime execution sessions, and content-addressed artifact capture (`ArtifactStore`).

However, architectural audit of the production daemon (`crates/ipc-transport/src/daemon.rs`) reveals a fundamental persistence gap:
**The production daemon initializes a fresh in-memory Control Plane on every startup.**
`run_daemon` hardcodes:
```rust
.with_control_plane(agent_studios_control_plane::ControlPlane::new_default())
```
Where `ControlPlane::new_default()` instantiates `InMemoryStore::new()`.

As a direct consequence, restarting `agsd` or suffering an unexpected process kill causes total amnesia of all domain entities (studios, agents, tasks, runs, approvals, worktree records, and event envelopes).

---

## 2. Durable vs Ephemeral State Audit Matrix

| Component / Entity | Current Storage Mechanism | Survives Daemon Restart? | Data Loss / Failure Impact on Restart |
|---|---|---|---|
| **Studio Records** (`Studio`) | In-memory `HashMap<StudioId, Studio>` | **NO (Lost)** | Studios disappear; clients cannot query historical studios. |
| **Agent Descriptors** (`AgentDescriptor`) | In-memory `HashMap<AgentId, AgentDescriptor>` | **NO (Lost)** | Registered agent IDs and roles wiped. |
| **Task Graph & Tasks** (`TaskRecord`) | In-memory `TaskGraph` | **NO (Lost)** | All completed, running, and queued tasks wiped. |
| **Run Records** (`RunRecord`) | In-memory `HashMap<RunId, RunRecord>` | **NO (Lost)** | Run attempts, timing, outcomes, and execution specs wiped. |
| **Approval Requests** (`ApprovalRequest`) | In-memory `HashMap<ApprovalId, ...>` | **NO (Lost)** | In-flight and resolved approvals wiped. |
| **Worktree Metadata** (`WorktreeRecord`) | In-memory `HashMap<WorktreeId, ...>` | **NO (Lost)** | Worktree assignment, base commit, and status wiped. |
| **Event Stream** (`EventEnvelope`) | `InMemoryStore` (`Vec<EventEnvelope>`) | **NO (Lost)** | Complete sequence history wiped. |
| **Artifact Metadata** (`ArtifactRecord`) | In-memory `HashMap<ArtifactId, ...>` | **NO (Lost)** | Historical patch and report references unlinked. |
| **Artifact Blobs** (Patches, Reports) | `ArtifactStore` on disk (`blobs/sha256/`) | **YES (Durable)** | Blobs survive on disk, but become orphaned without Control Plane metadata. |
| **Git Worktrees on Disk** | Physical Git checkouts (`worktrees/`) | **YES (Durable)** | Physical folders remain on disk, but become orphaned without ownership tracking. |
| **Client Bootstrap Credentials** | `agsd.json`, `agsd-admin.json` | **YES (Overwritten)** | Re-generated on startup; sessions must re-authenticate. |

---

## 3. Analysis of Existing Event-Sourcing Infrastructure

### 3.1 What Works Well (Solid Foundation to Reuse)

1. **Deterministic State Transitions (`ControlPlaneState::apply_event`):**
   `apply_event` in `crates/control-plane/src/engine.rs` contains rigorous domain validation rules (preventing state regression, verifying parent entity existence, enforcing studio boundaries).
2. **Atomic Batch Persistence Contract (`EventStore::append_batch`):**
   The trait defines:
   ```rust
   fn append_batch(&mut self, events: &[EventEnvelope]) -> Result<(), StoreError>;
   ```
   With strict all-or-nothing semantics and monotonic sequence numbering per studio.
3. **Staged Transaction Pipeline (`ControlPlane::commit_transaction`):**
   Transaction execution order:
   - Construct candidate events with assigned monotonic sequence numbers.
   - Clone current state into `staged_state`.
   - Apply candidate events to `staged_state`.
   - Call `self.store.append_batch(&envelopes)?`.
   - Commit state transition only AFTER storage persistence succeeds (`self.state = staged_state`).
4. **Physical Workspace Recovery Diagnostics (`WorkspaceOrchestrator::generate_recovery_report`):**
   Compares durable `WorktreeRecord`s against physical `git worktree list` checkouts on disk, identifying matched, missing, foreign, and ownership-mismatched checkouts.

### 3.2 Critical Replay Bug in `ControlPlane::replay_events`

Lines 3876-3880 in `engine.rs` currently execute:
```rust
// 5. Populate store atomically
store.append_batch(events).map_err(...)?;
```
If `replay_events` is called with an already-populated durable store, calling `store.append_batch(events)` will fail with `DuplicateEventId` or corrupt sequence counters.
**Resolution for M15-04:** Implement `ControlPlane::restore_from_store` which queries existing committed events from the store and reconstructs state without re-appending.

---

## 4. M15-04 Architecture Goals

1. Implement `SqliteEventStore`: A transactional SQLite backend with WAL mode, single-writer locking, monotonic sequence enforcement, and corruption detection.
2. Implement `ControlPlane::restore_from_store`: Restore full state from durable store on startup.
3. Detect & Mark Interrupted Executions: Running tasks on startup must not be assumed succeeded or resumed automatically; they must be marked interrupted/failed with domain audit trail.
4. Reconcile Git Worktrees & Artifacts: Ensure physical assets on disk match restored Control Plane records.
5. Provide Read-Only Recovery Status API: Enable future VS Code extensions to view recovery health.
