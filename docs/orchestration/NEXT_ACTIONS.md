# Next Actions Queue (`NEXT_ACTIONS.md`)

All offline engineering Waves 1–5 are complete and verified on `integration/m15-1-joint-validation` (code SHA `58293983af34ccf46ac44f2b44e224a33af31ba8`).

Remaining actions require human operator authorization (`GATE-01`..`GATE-04`):

1. **`GATE-01` — PR Review & Merge to Protected `main`**:
   - Review consolidated `integration/m15-1-joint-validation` and align `PR #33`, `PR #31`, `PR #34`, and `PR #32` before merging into protected `main`.
   - Note CI parity: run GitHub Actions matrix (`ubuntu-latest` + `windows-latest`, including `npm run test:integration` Extension Host tests) on the pushed integration branch.
2. **`GATE-02` — Legacy Worktree Cleanup**:
   - Operator approval before removing legacy worktrees listed in `09_SAFE_WORKTREE_CLEANUP_PLAN.md` (leave `D:\Projects\agent-studios` untouched unless explicitly approved).
3. **`GATE-03` — Live Cloud LLM Provider Acceptance (`$1`–`$5` cap)**:
   - Provision real credentials and authorize opt-in live model acceptance runs (`AGENT_STUDIOS_LIVE_CLI_SMOKE=1` / provider API keys) once ready to leave the `$0.00` offline boundary.
4. **`GATE-04` — Code Signing & Release Publishing**:
   - Sign `bin\agsd.exe`, `bin\ags.exe`, and generated Windows installer binaries in `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc1-win-x64` (replacing `UNSIGNED_RC1_REQUIRES_OPERATOR_CERT`) and optionally compile `AgentStudios.nsi` with NSIS for `.exe` setup distribution.
