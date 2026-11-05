# P0 Safe Worktree Cleanup — Phase B Execution Plan

**Document ID**: `docs/reviews/M15-SAFE-WORKTREE-CLEANUP/09_PHASE_B_EXECUTION_PLAN.md`  
**Date**: 2026-11-05  
**Auditor**: Principal Git Repository Engineer, Windows Filesystem Safety Auditor  
**Status**: STANDBY — REQUIRES EXPLICIT OPERATOR APPROVAL  
**Approved Manifest SHA-256**: `9e89e60e0a7b96640bb6a9e8f754559f2974f891e117af9096f568453832eba5`  

---

## 1. Safety Guard Notice

**THIS PLAN MUST NOT BE EXECUTED DURING PHASE A.**  
Execution is strictly restricted to Phase B following receipt of an explicit operator directive matching manifest SHA-256 `9e89e60e0a7b96640bb6a9e8f754559f2974f891e117af9096f568453832eba5`.

---

## 2. Pre-Execution Verification Steps

Before running any removal command in Phase B, verify:
1. Canonical repository HEAD is unchanged (`224f0bf486021afcf3d0773d4fc5fd6c86578411`).
2. Candidate worktrees remain clean (`git -C <path> status --porcelain=v1` returns 0).
3. Candidate worktree HEAD SHAs match expected values.
4. No new system processes have attached to candidate directories.

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
