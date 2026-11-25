# Next Actions Queue (`NEXT_ACTIONS.md`)

All offline engineering Waves 1–7 are complete and verified locally + on GitHub Actions CI (`ubuntu-latest` & `windows-latest` green on `integration/m15-1-joint-validation` code SHA `41480cfa025e41df105a3c01ecda2307b51fb180`, Runs `45590072851` and `45590064391`).

Remaining actions require human operator authorization (`GATE-01`..`GATE-04`):

1. **`GATE-01` — PR Review & Merge to Protected `main`**:
   - Review PR `#35` (`integration/m15-1-joint-validation` -> `integration/m15-unified-codebase`, `mergeable=True`, `mergeable_state=clean`) and follow [PR_RECONCILIATION_AND_RELEASE_GATE.md](../../docs/reviews/M15-1-JOINT-INTEGRATION/PR_RECONCILIATION_AND_RELEASE_GATE.md) to merge `#35`, close superseded `#31`/`#33`/`#34`, and merge `#32` -> `main`.
2. **`GATE-02` — Legacy Worktree Cleanup**:
   - Operator approval before removing legacy worktrees listed in `09_SAFE_WORKTREE_CLEANUP_PLAN.md` (leave `D:\Projects\agent-studios` untouched unless explicitly approved).
3. **`GATE-03` — Live Cloud LLM Provider Acceptance (`$1`–`$5` cap)**:
   - Provision real credentials and authorize opt-in live model acceptance runs (`AGENT_STUDIOS_LIVE_CLI_SMOKE=1` / provider API keys) once ready to leave the `$0.00` offline boundary.
4. **`GATE-04` — Code Signing & Release Publishing**:
   - Sign `bin\agsd.exe`, `bin\ags.exe`, and generated Windows installer binaries in `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc1-win-x64` (replacing `UNSIGNED_RC1_REQUIRES_OPERATOR_CERT`) and optionally compile `AgentStudios.nsi` with NSIS for `.exe` setup distribution.
