# 01 — Repository, Branch, PR & Worktree Forensic State

> **Baseline SHA**: `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c`
> **Audited**: 2026-11-25

---

## 1. Branch & Ref Convergence

| Ref | SHA | Relationship to `main` |
|---|---|---|
| `main` / `origin/main` | `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c` | Canonical protected baseline |
| `integration/m15-unified-codebase` / `origin/...` | `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c` | Equal to `main` (local root checkout stays safely at `8525ec228c`) |
| `integration/m15-1-joint-validation` / `origin/...` | `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c` | Equal to `main` |
| `feat/rc2-product-completion` | `00e5c862b8...` | Isolated Wave 0–10 RC2 branch checked out in active worktree |

---

## 2. Pull Request State on GitHub

- **Merged / Closed PRs**: `#35`, `#34`, `#33`, `#32`, `#31`, `#21`, `#17` (and stacked historical PRs `#30`..`#18` excluding `#20`, `#16`, `#15` auto-closed when merged branches were pruned after cutting `archive/v1.0.0-rc1/*` tags).
- **Still Open Historical Non-Ancestor PRs (Audit Verdict)**:

| PR | Branch | SHA | `git cherry -v main <branch>` | Forensic Assessment |
|---|---|---|---|---|
| **#15** | `origin/feat/m12-claude-code-runtime` | `3207b7462f` | `7` `(+)` commits | Early M12 commits authored before clean squash/re-import; actual `crates/claude-code-runtime` code (`7,572` SLOC, M12–M15) is already in `main` at higher maturity. Do NOT blindly merge (would conflict); preserve via `archive/v1.0.0-rc1/feat/m12-claude-code-runtime` and close cleanly once verified by operator. |
| **#16** | `origin/feat/ags-cli-tui-foundation` | `29865add12` | `11` `(+)` commits | Early M13 CLI/TUI branch; superceded cleanly by `PR #31` (`c98a8a3572`) and Wave 2/6 `ags` work already in `main`. Preserve tag; recommend close as superseded. |
| **#20** | `origin/feat/provider-management-v1` | `a98129ef7e` | `15` `(+)` commits | Early provider v1 branch; `crates/provider` (`6,332` SLOC) and `provider.rs` CLI already consolidated through `PR #31` into `main` (`git diff main..origin/feat/provider-management-v1 --stat` should be checked before closing). Preserve `HOLD` worktree until operator closes PR `#20`. |
| *(no PR)* | `origin/feat/m14-mission-control-workbench` | `3ee485cacd` | `3` `(-)` **100% patch-equivalent** | Every commit (`3ee485cacd`, `4693c63486`, `e988e785ac`) is already present in `main` via PR #25 rebase (`0` unique deltas). Preserved in `HOLD` worktree `D:\Projects\agent-studios-mission-control`. |
| *(no PR)* | `origin/feat/provider-discovery-v1.2-wip` | `96dfb4946e` | `8` `(+)` commits | Preserved WIP branch from `GATE-02` audit; superseded by merged `crates/provider/src/discovery.rs` (`PR #31`). |

---

## 3. Retained Worktree Inventory (`4` Total After Wave 8 Cleanup)

1. `D:\Projects\agent-studios` — `integration/m15-unified-codebase` at `8525ec228c` (Canonical root `.git` object store, preserves 5 untracked recovery/inventory docs).
2. `D:\Projects\agent-studios-active\m15-1-joint-validation` — `feat/rc2-product-completion` (Clean active RC2 development worktree).
3. `D:\Projects\agent-studios-ags-cli` — `feat/provider-management-v1` at `a98129ef7e` (`HOLD` / PR `#20`).
4. `D:\Projects\agent-studios-mission-control` — `feat/m14-mission-control-workbench` at `3ee485cacd` (`HOLD`).
