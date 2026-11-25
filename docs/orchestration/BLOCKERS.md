# Blockers & Human Approval Gates (`BLOCKERS.md`)

> **Updated Post-Extension-First**: 2026-11-25T21:54:00+07:00 (`feat/extension-first-product-completion` branched from `feat/rc2-product-completion`; `main` untouched at `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c`)

## Resolved Gates

| Gate ID | Category | Resolution Summary |
|---|---|---|
| `GATE-01` | Protected Branch Merge | **COMPLETED**: Operator authorized merge of `integration/m15-1-joint-validation` (`00e5c862b8`) into `integration/m15-unified-codebase` and `main`; PRs `#31`–`#35` merged/closed cleanly and GitHub Actions Runs `45590072851`, `45590064391`, `45597809243` verified green. |
| `GATE-02` | Wave 8 Worktree & Cache Cleanup | **COMPLETED (Scoped to Wave 8)**: Operator authorized Wave 8 cleanup of 7 strictly merged worktrees, 279 temp worktree registry entries, 18 strictly merged remote branches, and excess build/test caches (`+96.85 GB` freed). Retained `D:\Projects\agent-studios`, `D:\Projects\agent-studios-active\m15-1-joint-validation`, `D:\Projects\agent-studios-ags-cli` (`HOLD`), `D:\Projects\agent-studios-mission-control` (`HOLD`), and `archive/v1.0.0-rc1/*` tags untouched. Future destructive cleanups still require explicit approval. |

## Active Human Approval Gates (Do Not Bypass)

| Gate ID | Category | Description | Current Impact | Mitigation / Unblocked Path |
|---|---|---|---|---|
| `GATE-03` | Live Billable LLM Acceptance | Executing real API calls against Anthropic/OpenAI/Gemini (`$1.00`–`$5.00` cap) | Live cloud model end-to-end token flow stays gated (`REAL_AI_MODEL_NOT_TESTED`) | Complete `LiveAcceptanceHarness` with budget caps, kill switch, and deterministic local verification at `$0.00` |
| `GATE-04` | Code Signing Certificate | Authenticode EV/OV signing of `AgentStudios.exe`, `ags.exe`, `agsd.exe`, and Windows installer | Distribution artifacts marked as unsigned RC1 / RC2 | Produce full unsigned release bundle + real sandbox install verification + `SHA256SUMS.txt` |
