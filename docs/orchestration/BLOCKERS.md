# Blockers & Human Approval Gates (`BLOCKERS.md`)

## Active Human Approval Gates (Do Not Bypass)

| Gate ID | Category | Description | Current Impact | Mitigation / Unblocked Path |
|---|---|---|---|---|
| `GATE-01` | Protected Branch Merge | Merging PR #33, #31, #34, and #32 into `integration/m15-unified-codebase` and protected `main` | Cannot update `main` autonomously | Continue full engineering, testing, and packaging on `integration/m15-1-joint-validation` |
| `GATE-02` | Destructive Worktree Cleanup | Removing `HOLD` or `APPROVED_CANDIDATE` directories or cleaning `D:\Projects\agent-studios` untracked recovery docs | None (25.52 GB free on D:, 243.62 GB free on C:) | Build all Rust targets to `C:\AgentStudiosBuild\worktrees\m15-1-joint-validation` |
| `GATE-03` | Live Billable LLM Acceptance | Executing real API calls against Anthropic/OpenAI/Gemini ($1.00–$5.00 cap) | Live cloud model end-to-end token flow stays gated | Automated P0 real execution tests verify Git worktree, file edits, real local tool execution, 2PC approval, cancellation, and SQLite reconnect at `$0.00` |
| `GATE-04` | Code Signing Certificate | Authenticode EV/OV signing of `ags.exe`, `agsd.exe`, and Windows installer | Distribution artifacts marked as unsigned RC1 | Produce full unsigned release bundle + `SHA256SUMS.txt` + verification script and record signing gate in RC1 release notes |
