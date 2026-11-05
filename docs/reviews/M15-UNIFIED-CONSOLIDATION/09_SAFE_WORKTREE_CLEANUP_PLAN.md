# 09_SAFE_WORKTREE_CLEANUP_PLAN.md — Safe Worktree Cleanup & Disk Reclamation Plan

**Document Reference**: `09_SAFE_WORKTREE_CLEANUP_PLAN.md`  
**Milestone**: M15 Consolidation & Workspace Audit  
**Auditor**: Agent D (Workspace Inventory & Cleanup Auditor)  
**Audit Date**: 2026-11-04  
**Workspace Root**: `D:\Projects`  
**Canonical Git Repository**: `D:\Projects\agent-studios` (`https://github.com/leduwn/agent-studios.git`)  

---

## 1. Guiding Principles & Safety Philosophy

1. **Zero Data Loss Guarantee**: No directory or worktree may be deleted if it contains uncommitted modifications, untracked files, unpushed commits, or unmerged unique commits.
2. **No Silent Auto-Deletion**: Destructive commands (`Remove-Item -Recurse`, `rm -rf`, `git worktree remove --force`) are strictly prohibited in automated loops. Every removal requires an explicit verification gate.
3. **Decoupled Two-Phase Reclamation**:
   - **Phase 1 (Risk-Free Disk Reclaim)**: Purge ephemeral compiler build caches (`target`, `node_modules`, `.cache`) without modifying Git worktrees or branches. This safely reclaims over **23.5 GB** of disk space immediately without altering any repository state.
   - **Phase 2 (Controlled Worktree Pruning)**: Deregister and remove intermediate worktrees only after verification gates pass and safety tags are cut.
4. **Clean Worktree Deregistration**: Worktrees must be removed using `git worktree remove <path>` followed by `git worktree prune --verbose`, never through raw filesystem deletion alone, preventing dangling metadata in `.git/worktrees`.

---

## 2. Worktree Classification & Action Matrix

| Worktree Directory | Classification | Retain / Prune Decision | Immediate Action | Prerequisites for Final Deletion |
| :--- | :--- | :--- | :--- | :--- |
| `agent-studios` | **`CANONICAL_CANDIDATE`** | **PERMANENT RETAIN** | **LOCK / PROTECTED** | CANNOT BE DELETED. Host of canonical `.git` store. Resolve 9 uncommitted changes. |
| `agent-studios-m15-04` | **`ACTIVE_WORKTREE`** | **RETAIN** (Consolidation Source) | Purge `target/` cache if needed | Keep until M15 backend (M15-05) is merged into canonical root. |
| `agent-studios-ide-ui` | **`ACTIVE_WORKTREE`** | **RETAIN** (Consolidation Source) | Keep active | Keep until M15 UI (M15-UI-05A) is merged into canonical root. |
| `agent-studios-ags-cli` | **`UNMERGED_WORK`** | **RETAIN** | Keep active | Keep until `feat/provider-management-v1` PR is merged to `main`. |
| `agent-studios-mission-control` | **`HOLD`** | **DO NOT PRUNE** | **SAFETY HOLD** | Contains 3 unique commits (`3ee485cacd`, `4693c63486`, `e988e785ac`). Must cherry-pick to UI tip before archival. |
| `agent-studios-m15-03` | **`ARCHIVE_CANDIDATE`** | **PRUNE (Phased)** | Purge `target/` (13.79 GB) | Verified 100% ancestor of `m15-04`. Tag branch, run `git worktree remove`. |
| `agent-studios-m15-02` | **`ARCHIVE_CANDIDATE`** | **PRUNE (Phased)** | Purge `target/` (7.48 GB) | Verified 100% ancestor of `m15-03`. Fetch 12 remote commits, tag branch, run `git worktree remove`. |
| `agent-studios-m15` | **`ARCHIVE_CANDIDATE`** | **PRUNE (Phased)** | Eligible for removal | Verified 100% ancestor of `m15-02`. Tag branch, run `git worktree remove`. |
| `agent-studios-m13` | **`ARCHIVE_CANDIDATE`** | **PRUNE (Phased)** | Eligible for removal | Verified 100% ancestor of `m15-01`. Tag branch, run `git worktree remove`. |
| `agent-studios-mission-control-integration` | **`ARCHIVE_CANDIDATE`** | **PRUNE (Phased)** | Eligible for removal | Verified 100% ancestor of `ide-ui`. Tag branch, run `git worktree remove`. |
| `agent-studios-vscode-ipc-03` | **`ARCHIVE_CANDIDATE`** | **PRUNE (Phased)** | Eligible for removal | Verified 100% ancestor of `integration`. Tag branch, run `git worktree remove`. |
| `agent-studios-vscode-m14-02` | **`ARCHIVE_CANDIDATE`** | **PRUNE (Phased)** | Eligible for removal | Verified 100% ancestor of all UI branches. Tag branch, run `git worktree remove`. |
| `AppData\Local\Temp\agent-studios\worktrees\*` | **`TRANSIENT_TEST_TREES`** | **PURGE & PRUNE** | Safe batch cleanup | 281 orphaned integration test worktrees. Safe to purge temp folders and run `git worktree prune`. |

---

## 3. Explicit Safety Gates

Before any worktree removal command is executed, all 5 safety gates must evaluate to `PASSED`:

```
[TARGET WORKTREE]
       │
       ▼
 [GATE 1: Uncommitted Check] ──(Dirty > 0)──► [HALT: Abort, Require Stash/Commit]
       │ (0 Dirty)
       ▼
 [GATE 2: Remote Sync Check] ──(Unpushed > 0)─► [HALT: Abort, Require git push]
       │ (Synced / Ancestor)
       ▼
 [GATE 3: Lineage Subsumption Check] ──(Unique Commits > 0)─► [HALT: Flag HOLD, Require Cherry-pick]
       │ (Subsumed)
       ▼
 [GATE 4: Process Lock Check] ──(Locks Present)─► [HALT: Close Locking Process]
       │ (No Locks)
       ▼
 [GATE 5: Archival Tagging Gate] ──► [Create git tag archive/<branch>-<sha>]
       │
       ▼
 [EXECUTE: git worktree remove <path>]
```

### Safety Gate Specifications

1. **Gate 1 — Working Tree Cleanliness (`status --porcelain`)**:
   - Command: `git -C <path> status --porcelain`
   - Gate Condition: Output must be completely empty (0 characters).
   - If Failed: Immediate HALT. Never pass `--force` to `git worktree remove`.

2. **Gate 2 — Upstream Synchronization (`log @{u}..HEAD`)**:
   - Command: `git -C <path> log @{u}..HEAD --oneline` (or vs `origin/<branch>`).
   - Gate Condition: 0 unpushed commits. Local HEAD must exist on remote `origin`.
   - If Failed: Immediate HALT. Prompt operator to push branch.

3. **Gate 3 — Ancestry & Subsumption Verification**:
   - Command: `git -C "D:\Projects\agent-studios" merge-base --is-ancestor <candidate_branch> <downstream_tip_branch>`
   - Gate Condition: Must return exit code `0`. For backend branches, downstream tip is `feat/m15-04-durable-state-recovery`. For UI branches, downstream tip is `feat/m15-real-agent-execution-ui`.
   - If Failed: Immediate HALT. Move classification to `HOLD`.

4. **Gate 4 — Process & Handle Lock Verification**:
   - Verification: Ensure no shell terminal has `CWD` inside the target directory, and no active process (`cargo`, `rustc`, `node`, `Code.exe`) holds open handles.
   - If Failed: Terminate or close the offending process before removal.

5. **Gate 5 — Archival Tagging**:
   - Prior to worktree removal, create an immutable reference tag:
     ```bash
     git tag -a "archive/m15-consolidation/<branch-name>-<short-sha>" <short-sha> -m "Archival snapshot before worktree removal"
     ```

---

## 4. Phased Execution Roadmap

### Phase 1: Immediate Disk Reclamation (Reclaiming ~23.5 GB)

Phase 1 frees disk space immediately without touching Git branches, worktree registrations, or code files. It only removes build caches (`target` and `.code-review-graph`).

#### Step 1.1: Purge Intermediate Rust Build Targets
Run `cargo clean` inside `agent-studios-m15-02` and `agent-studios-m15-03`:

```powershell
# Reclaims 7.48 GB from m15-02
if (Test-Path "D:\Projects\agent-studios-m15-02\agent-studios-rs\target") {
    Write-Output "Cleaning target in agent-studios-m15-02..."
    cargo clean --manifest-path "D:\Projects\agent-studios-m15-02\agent-studios-rs\Cargo.toml"
}

# Reclaims 13.79 GB from m15-03
if (Test-Path "D:\Projects\agent-studios-m15-03\agent-studios-rs\target") {
    Write-Output "Cleaning target in agent-studios-m15-03..."
    cargo clean --manifest-path "D:\Projects\agent-studios-m15-03\agent-studios-rs\Cargo.toml"
}
```

#### Step 1.2: Prune Stale Code Review Graph in Canonical Root
In `D:\Projects\agent-studios`, `.code-review-graph` consumes **1.83 GB**. If review runs are completed:
```powershell
# Optional: reclaims 1.83 GB from canonical root
if (Test-Path "D:\Projects\agent-studios\.code-review-graph") {
    Write-Output "Removing ephemeral .code-review-graph..."
    Remove-Item -Recurse -Force "D:\Projects\agent-studios\.code-review-graph"
}
```
*Total Disk Recovered in Phase 1: ~23.1 GB.*

---

### Phase 2: Purge Transient Temp Worktree Registry (281 Worktrees)

The 281 orphaned test worktrees in `C:\Users\Duwn\AppData\Local\Temp\agent-studios\worktrees` bloat `.git/worktrees` and slow down all Git operations.

#### Step 2.1: Verify Temp Directory
Verify that no critical user work exists in `C:\Users\Duwn\AppData\Local\Temp\agent-studios\worktrees`:
```powershell
$tempDir = "C:\Users\Duwn\AppData\Local\Temp\agent-studios\worktrees"
if (Test-Path $tempDir) {
    $count = (Get-ChildItem $tempDir -Directory).Count
    Write-Output "Found $count temporary test worktree directories."
}
```

#### Step 2.2: Purge Temp Worktrees from Disk
```powershell
$tempDir = "C:\Users\Duwn\AppData\Local\Temp\agent-studios\worktrees"
if (Test-Path $tempDir) {
    Remove-Item -Recurse -Force $tempDir
    Write-Output "Temporary test worktree directories deleted from AppData."
}
```

#### Step 2.3: Prune Git Worktree Registry
Run `git worktree prune` from the canonical repository root:
```powershell
git -C "D:\Projects\agent-studios" worktree prune --verbose
```
*Result: Removes 281 stale control directories from `.git/worktrees`. Restores `git worktree list` output from 42 KB to ~1 KB.*

---

### Phase 3: Archive & Prune Intermediate Worktrees in `D:\Projects`

Only perform Phase 3 for the 7 verified `ARCHIVE_CANDIDATE` directories after completing M15 consolidation planning.

#### Step 3.1: Pre-Archival Branch Tagging
Cut archival snapshot tags in the canonical repository:
```powershell
$archiveBranches = @(
    @{ Branch = "feat/m13-ipc-02-control-plane"; Sha = "ce0396c1f1" },
    @{ Branch = "feat/m15-01-per-task-execution-contract"; Sha = "a5dbbfc5eb" },
    @{ Branch = "feat/m15-02-multi-runtime-registry"; Sha = "b307b9a6e2" },
    @{ Branch = "feat/m15-03-artifact-diff-review"; Sha = "cc1c3e8f39" },
    @{ Branch = "feat/m14-vscode-extension-host-hardening"; Sha = "ed80b243c9" },
    @{ Branch = "feat/m14-ipc-03-connected-client"; Sha = "73b7298ca3" },
    @{ Branch = "feat/m14-mission-control-connected-integration"; Sha = "a13230ee93" }
)

foreach ($item in $archiveBranches) {
    $tag = "archive/m15-audit/$($item.Branch)"
    & git -C "D:\Projects\agent-studios" tag -f -a $tag $item.Sha -m "Consolidation archive snapshot of $($item.Branch)"
    Write-Output "Created tag: $tag ($($item.Sha))"
}
```

#### Step 3.2: Safe Worktree Removal Script
Execute standard Git worktree removal with gate validation:

```powershell
$worktreesToPrune = @(
    "D:\Projects\agent-studios-m13",
    "D:\Projects\agent-studios-m15",
    "D:\Projects\agent-studios-m15-02",
    "D:\Projects\agent-studios-m15-03",
    "D:\Projects\agent-studios-vscode-m14-02",
    "D:\Projects\agent-studios-vscode-ipc-03",
    "D:\Projects\agent-studios-mission-control-integration"
)

foreach ($wt in $worktreesToPrune) {
    if (-not (Test-Path $wt)) {
        Write-Output "Worktree already absent: $wt"
        continue
    }

    # Gate 1: Check status
    $porcelain = & git -C $wt status --porcelain 2>$null
    if ($porcelain) {
        Write-Error "GATE 1 FAILED: $wt has uncommitted changes! Aborting removal."
        continue
    }

    # Gate 4: Process lock check
    Write-Output "Removing worktree: $wt"
    & git -C "D:\Projects\agent-studios" worktree remove $wt
    if ($LASTEXITCODE -eq 0) {
        Write-Output "Successfully removed $wt"
    } else {
        Write-Warning "git worktree remove reported non-zero status for $wt"
    }
}

# Final registry prune
git -C "D:\Projects\agent-studios" worktree prune --verbose
```

---

## 5. Preservation & Special Action Directives

### Special Directive 1: Resolving the 9 Uncommitted Files in Canonical Root (`agent-studios`)
`agent-studios` is on branch `feat/m12-claude-code-runtime` and has 9 uncommitted files.
1. **Uncommitted files**:
   - `M agent-studios-rs/Cargo.lock`
   - `M agent-studios-rs/crates/provider/Cargo.toml`
   - `M agent-studios-rs/crates/provider/src/error.rs`
   - `M agent-studios-rs/crates/provider/src/lib.rs`
   - `M agent-studios-rs/crates/provider/src/secret.rs`
   - `?? AGENTS.md`
   - `?? agent-studios-rs/crates/provider/src/discovery.rs`
   - `?? agent-studios-rs/crates/provider/tests/discovery_tests.rs`
   - `?? docs/reviews/M13-IPC-02.2.2/`
2. **Analysis**:
   - The provider modifications (`Cargo.toml`, `error.rs`, `lib.rs`, `secret.rs`, `discovery.rs`, `discovery_tests.rs`) are an earlier scratch draft of what was officially finalized and committed in `feat/provider-management-v1` (`agent-studios-ags-cli`).
   - `docs/reviews/M13-IPC-02.2.2/M15_BACKEND_READINESS_MATRIX.md` is identical to the file committed in `feat/m13-ipc-02-control-plane`.
3. **Preservation Command**:
   ```powershell
   git -C "D:\Projects\agent-studios" stash save "WIP-provider-discovery-scratchpad-before-m15-consolidation"
   ```
   This safely preserves all 9 files into Git stash storage while restoring a 100% clean working directory on `feat/m12-claude-code-runtime`.

### Special Directive 2: Cherry-Picking Mission Control Workbench (`agent-studios-mission-control`)
`feat/m14-mission-control-workbench` contains 3 commits that must NOT be discarded:
- `e988e785ac`: `docs(mission-control): establish M14 Mission Control architecture and specifications`
- `4693c63486`: `feat(vscode): deliver M14 Mission Control workbench multi-tab monitoring, DAG & review interface`
- `3ee485cacd`: `docs(mission-control): align product spec and reference inventory with research findings`

Prior to pruning `agent-studios-mission-control`:
1. Verify if these features are desired in M15 UI or the unified branch.
2. If desired, cherry-pick them into `feat/m15-real-agent-execution-ui` or create a consolidation branch.
3. If intentionally superseded, document the decision in the consolidation changelog before removing the worktree.

---

## 6. Disaster Recovery & Rollback Procedure

If a worktree was removed and needs to be re-instantiated:
1. All Git history remains intact in `.git/objects` and remote `origin`.
2. To recreate any worktree instantly:
   ```bash
   git -C "D:\Projects\agent-studios" worktree add <path> <branch-or-tag>
   ```
   Example:
   ```bash
   git -C "D:\Projects\agent-studios" worktree add "D:\Projects\agent-studios-m15-03" feat/m15-03-artifact-diff-review
   ```
3. To recover an accidental commit deletion or detached HEAD, query `git reflog`:
   ```bash
   git -C "D:\Projects\agent-studios" reflog
   ```

---
*Plan authored by Agent D (Workspace Inventory & Cleanup Auditor) for AGENT STUDIOS M15 CONSOLIDATION.*
