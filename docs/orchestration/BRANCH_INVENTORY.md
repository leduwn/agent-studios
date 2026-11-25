# Branch & Worktree Inventory (`BRANCH_INVENTORY.md`)

> **Verified Post-RC1 Cleanup**: 2026-11-25T18:33:00+07:00
> **Canonical `main` HEAD**: `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c`

## 1. Retained Local Worktrees (4 Total — 279 Temp Entries & 7 Merged Worktrees Pruned)

| Worktree Directory | Checked-Out Branch | HEAD SHA | Status / Disposition |
|---|---|---|---|
| `D:\Projects\agent-studios` | `integration/m15-unified-codebase` | `8525ec228c` | **CANONICAL ROOT (PROTECTED)** — preserves 5 untracked recovery/inventory docs |
| `D:\Projects\agent-studios-active\m15-1-joint-validation` | `feat/rc2-product-completion` (from `main`) | `00e5c862b8` | **ACTIVE RC2 DEVELOPMENT WORKTREE** — isolated from `main` |
| `D:\Projects\agent-studios-ags-cli` | `feat/provider-management-v1` | `a98129ef7e` | **`HOLD` / NON-ANCESTOR** — 15 unique commits (`PR #20`) preserved for Phase A/D review |
| `D:\Projects\agent-studios-mission-control` | `feat/m14-mission-control-workbench` | `3ee485cacd` | **`HOLD`** — 3 commits (all patch-equivalent `(-)` via rebase) retained under `HOLD` policy |

## 2. Origin Remote Branches Status

| Branch | SHA | Ancestry vs `main` | Disposition |
|---|---|---|---|
| `origin/main` | `00e5c862b8` | `HEAD` | Canonical protected release baseline |
| `origin/integration/m15-unified-codebase` | `00e5c862b8` | Ancestor (equal) | Converged with `main` |
| `origin/integration/m15-1-joint-validation` | `00e5c862b8` | Ancestor (equal) | Converged with `main` |
| `origin/feat/m12-claude-code-runtime` | `3207b7462f` | Non-ancestor (`7` commits) | Open `PR #15`; kept for provenance |
| `origin/feat/ags-cli-tui-foundation` | `29865add12` | Non-ancestor (`11` commits) | Open `PR #16`; kept for provenance |
| `origin/feat/provider-management-v1` | `a98129ef7e` | Non-ancestor (`15` commits) | Open `PR #20`; kept for provenance |
| `origin/feat/m14-mission-control-workbench` | `3ee485cacd` | Patch-equivalent (`3` `(-)` commits) | Retained for `HOLD` worktree safety |
| `origin/feat/provider-discovery-v1.2-wip` | `96dfb4946e` | Non-ancestor (`8` commits) | Operator WIP discovery snapshot |

- **Deleted 18 Merged Remote Branches**: All 18 strictly ancestor feature/fix/chore branches were tagged under `archive/v1.0.0-rc1/*` and deleted cleanly from `origin` during Wave 8 cleanup.
