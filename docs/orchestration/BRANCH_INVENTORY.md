# Branch & Worktree Inventory (`BRANCH_INVENTORY.md`)

> **Verified At**: 2026-11-25T00:45:00+07:00

## 1. Active & Preserved Local Worktrees

| Worktree Directory | Checked-Out Branch | HEAD SHA | Status / Disposition |
|---|---|---|---|
| `D:\Projects\agent-studios` | `integration/m15-unified-codebase` | `8525ec228c` | **CANONICAL ROOT (PROTECTED)** — has 5 untracked recovery/inventory artifacts |
| `D:\Projects\agent-studios-active\m15-1-joint-validation` | `integration/m15-1-joint-validation` | `5f7b5f525b` | **ACTIVE INTEGRATION WORKTREE** — supersedes PR #31/#33/#34 + cache policy |
| `D:\Projects\agent-studios-active\ci-repair` | `fix/m15-unified-cross-platform-ci` | `65397d709d` | **PR #33** — fully merged into `integration/m15-1-joint-validation` |
| `D:\Projects\agent-studios-active\m15-1-backend` | `feat/m15-1-provider-cli-unification` | `c98a8a3572` | **PR #31** — fully merged into `integration/m15-1-joint-validation` |
| `D:\Projects\agent-studios-active\m15-ui-06` | `feat/m15-ui-06-agent-ide-continuity` | `f57bc20c74` | **PR #34** — fully merged into `integration/m15-1-joint-validation` |
| `D:\Projects\agent-studios-ags-cli` | `feat/provider-management-v1` | `a98129ef7e` | **PRESERVED** — historical PR #20 CLI/provider origin |
| `D:\Projects\agent-studios-ide-ui` | `feat/m15-real-agent-execution-ui` | `3de7c85a96` | **HOLD** — PR #27 tip |
| `D:\Projects\agent-studios-m15-04` | `feat/m15-04-durable-state-recovery` | `cf1e6c0f98` | **HOLD** — PR #30 tip |
| `D:\Projects\agent-studios-mission-control` | `feat/m14-mission-control-workbench` | `3ee485cacd` | **HOLD (DIVERGENT COMMITS)** |
| `D:\Projects\agent-studios-unified-staging` | detached HEAD | `843be62033` | **PRESERVED** |

## 2. Recommended PR Merge Sequence (Operator Sign-off)

1. **PR #33** (`fix/m15-unified-cross-platform-ci` `65397d709d` → `integration/m15-unified-codebase`)
2. **PR #31** (`feat/m15-1-provider-cli-unification` `c98a8a3572` → `integration/m15-unified-codebase`)
3. **PR #34** (`feat/m15-ui-06-agent-ide-continuity` `f57bc20c74` → `integration/m15-unified-codebase`)
4. **PR #32** (`integration/m15-unified-codebase` / `integration/m15-1-joint-validation` → `main`)

*(Note: All commits from #33, #31, #34, and `chore/dev-build-cache-policy` are already consolidated in `integration/m15-1-joint-validation`.)*
