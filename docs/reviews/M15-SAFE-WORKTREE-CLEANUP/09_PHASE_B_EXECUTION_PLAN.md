# P0 Safe Worktree Cleanup — Phase B Execution Plan

**Document ID**: `docs/reviews/M15-SAFE-WORKTREE-CLEANUP/09_PHASE_B_EXECUTION_PLAN.md`  
**Date**: 2026-11-05  
**Auditor**: Principal Git Repository Engineer, Windows Filesystem Safety Auditor  
**Status**: STANDBY — REQUIRES EXPLICIT OPERATOR APPROVAL  
**Approved Manifest SHA-256**: `0ad2ef03f893b448a16e8a618bf916b93445967a0f3a50ef19c5b4827c9b58cc`  

---

## 1. Safety Guard Notice

**THIS PLAN MUST NOT BE EXECUTED DURING PHASE A.**  
Execution is strictly restricted to Phase B following receipt of an explicit operator directive matching manifest SHA-256 `0ad2ef03f893b448a16e8a618bf916b93445967a0f3a50ef19c5b4827c9b58cc`.

---

## 2. Pre-Execution Verification Steps

Before running any removal command in Phase B, verify:
1. Operator approval string exactly matches: `APPROVE P0 CLEANUP PHASE B: 7 allowlisted worktrees, manifest SHA-256: 0ad2ef03f893b448a16e8a618bf916b93445967a0f3a50ef19c5b4827c9b58cc`.
2. Manifest integrity: `Get-FileHash docs/reviews/M15-SAFE-WORKTREE-CLEANUP/cleanup_candidate_manifest.json` matches SHA-256 `0ad2ef03f893b448a16e8a618bf916b93445967a0f3a50ef19c5b4827c9b58cc`.
3. Canonical repository HEAD descends from audited baseline `224f0bf486021afcf3d0773d4fc5fd6c86578411` (`git merge-base --is-ancestor 224f0bf486021afcf3d0773d4fc5fd6c86578411 HEAD`), and `git diff --name-only 224f0bf486021afcf3d0773d4fc5fd6c86578411 HEAD` contains only paths under `docs/` (zero source code, tests, or configurations modified).
4. Candidate worktrees remain 100% clean (`git -C <path> status --porcelain=v1` returns empty).
5. Candidate worktree HEAD SHAs match expected values in the manifest.
6. Target paths strictly match the 7 allowlisted paths (no traversal, no symlinks, no drive C temp worktrees).
7. Zero process locks: no active processes have open handles to candidate directories.
8. If ANY check fails: ABORT immediately (fail-closed, no partial execution).

---

## 3. Sequential Worktree Removal Sequence

Execute removals sequentially using Git-native commands from canonical root `D:\Projects\agent-studios`:

### Step 1: Clean build caches in candidate worktrees (Optional Pre-pass)
```powershell
# Relieve file locks and reduce disk footprint before unregistering
cargo clean --manifest-path "D:\Projects\agent-studios-m15-02\agent-studios-rs\Cargo.toml"
cargo clean --manifest-path "D:\Projects\agent-studios-m15-03\agent-studios-rs\Cargo.toml"
```

### Step 2: Sequential Git-native worktree removal
```powershell
# 1. agent-studios-m13
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-m13"

# 2. agent-studios-m15
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-m15"

# 3. agent-studios-m15-02
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-m15-02"

# 4. agent-studios-m15-03
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-m15-03"

# 5. agent-studios-vscode-m14-02
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-vscode-m14-02"

# 6. agent-studios-vscode-ipc-03
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-vscode-ipc-03"

# 7. agent-studios-mission-control-integration
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-mission-control-integration"
```

### Step 3: Worktree metadata pruning
```powershell
# Prune orphaned worktree administrative records in .git/worktrees
git -C "D:\Projects\agent-studios" worktree prune
```

---

## 4. Post-Execution Verification Checklist

Following completion of Step 3:
1. `git -C "D:\Projects\agent-studios" worktree list` confirms only 6 protected worktrees remain on drive `D:\`.
2. `git -C "D:\Projects\agent-studios" status` confirms working copy clean.
3. `Get-PSDrive D` confirms free space increased by **~23.69 GB** (projected total: **~27.87 GB free**).
4. Verify Provider Discovery WIP backup in `D:\Projects\agent-studios-preservation` remains 100% intact.
5. Verify branch references in `.git/refs/heads/` remain completely intact.

---

## 5. Worktree Registry Scope Distinction & Safety Boundaries

### Drive D: vs Drive C: Scope Separation
- **Drive D: Local Worktrees (13 Total)**:
  - 1 Canonical Root: `D:\Projects\agent-studios` (PERMANENT)
  - 5 Protected Worktrees: `agent-studios-unified-staging`, `agent-studios-m15-04`, `agent-studios-m15-frontend`, `agent-studios-m14-archive`, `agent-studios-ags-cli` (NEVER DELETE)
  - 7 Approved Candidates: strictly matching allowlisted paths in `cleanup_candidate_manifest.json`
- **Drive C: Temporary Test Worktrees (281 Total)**:
  - Located in `C:\Users\Duwn\AppData\Local\Temp\agent-studios\worktrees\...`
  - Created dynamically by automated integration test harnesses
  - **OUT OF SCOPE**: Phase B executes zero operations against Drive C temporary worktrees.

### Strict Prohibition of Force Mode
- Every removal command MUST be standard `git worktree remove <path>` WITHOUT `--force`.
- Git's internal safety gate prevents deletion if unexpected uncommitted or untracked files are discovered.
- If any command fails or raises an error, Phase B execution MUST stop immediately (fail-closed).

