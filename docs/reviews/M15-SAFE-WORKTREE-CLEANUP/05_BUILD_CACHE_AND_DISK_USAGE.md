# P0 Safe Worktree Cleanup — Build Cache & Disk Usage Report

**Document ID**: `docs/reviews/M15-SAFE-WORKTREE-CLEANUP/05_BUILD_CACHE_AND_DISK_USAGE.md`  
**Date**: 2026-11-05  
**Auditor**: Principal Git Repository Engineer, Windows Filesystem Safety Auditor  
**Audit Phase**: PHASE A — NON-DESTRUCTIVE SAFETY AUDIT  

---

## 1. Executive Summary

Drive `D:\` capacity is 110.00 GB, with only 4.18 GB free at the start of this audit. Accurate per-worktree disk measurement was performed across all 7 candidates, avoiding junction recursion and separating source files from build artifacts.

---

## 2. Granular Disk Footprint Breakdown

| # | Worktree Candidate | Cargo `target/` | `node_modules/` | `.vscode-test/` | Source & Other | Total Footprint (MB) | Total Footprint (GB) |
|:---:|---|:---:|:---:|:---:|:---:|:---:|:---:|
| 1 | `agent-studios-m13` | 0.00 MB | 0.00 MB | 0.00 MB | 89.73 MB | **89.73 MB** | **0.088 GB** |
| 2 | `agent-studios-m15` | 0.00 MB | 0.00 MB | 0.00 MB | 89.85 MB | **89.85 MB** | **0.088 GB** |
| 3 | `agent-studios-m15-02` | 7,478.52 MB | 0.00 MB | 0.00 MB | 89.94 MB | **7,568.46 MB** | **7.391 GB** |
| 4 | `agent-studios-m15-03` | 13,790.39 MB | 0.00 MB | 0.00 MB | 90.15 MB | **13,880.54 MB** | **13.555 GB** |
| 5 | `agent-studios-vscode-m14-02` | 0.00 MB | 102.16 MB | 1,024.10 MB | 91.31 MB | **1,217.57 MB** | **1.189 GB** |
| 6 | `agent-studios-vscode-ipc-03` | 0.00 MB | 102.16 MB | 0.00 MB | 91.67 MB | **193.83 MB** | **0.189 GB** |
| 7 | `agent-studios-mission-control-integration` | 0.00 MB | 102.16 MB | 1,024.10 MB | 92.37 MB | **1,218.63 MB** | **1.190 GB** |
| **TOTAL** | **7 Candidates Combined** | **21,268.91 MB** | **306.48 MB** | **2,048.20 MB** | **635.02 MB** | **24,258.61 MB** | **23.690 GB** |

---

## 3. Shared Build Cache Safety Analysis

1. **Cargo `CARGO_TARGET_DIR` Verification**:
   - Inspected `agent-studios-rs/.cargo/config.toml` across all worktrees.
   - Result: `None`. Zero worktrees override `target-dir`.
   - Each Cargo target directory resides strictly inside its own worktree (`agent-studios-m15-02\agent-studios-rs\target`, `agent-studios-m15-03\agent-studios-rs\target`).
   - Removing these directories has zero effect on the canonical target directory (`D:\Projects\agent-studios\agent-studios-rs\target`).

2. **Node.js & VS Code Test Cache Verification**:
   - `apps/agent-studios-vscode/node_modules` and `.vscode-test` are strictly local to each candidate worktree.
   - Removing them will not affect the canonical VS Code extension or dependencies in `D:\Projects\agent-studios\apps\agent-studios-vscode`.

---

## 4. Recoverable Disk Space Projection

- **Current Free Space on D:**: ~4.18 GB.
- **Recoverable from 7 Candidates**: **~23.69 GB** (24,258.61 MB).
- **Projected Free Space after Phase B**: **~27.87 GB**.
- **Net Gain**: Substantial restoration of disk capacity, resolving the MSVC concurrent linker space bottlenecks permanently.
