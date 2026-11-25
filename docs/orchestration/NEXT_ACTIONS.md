# Next Actions Queue (`NEXT_ACTIONS.md`)

> **Updated Post-RC2**: 2026-11-25T19:46:00+07:00
> **Current Branch**: `feat/rc2-product-completion` (pushed cleanly; `main` untouched)

All autonomous RC2 engineering Waves 0–10 are **COMPLETE**. Remaining operator-gated actions:

1. **PR Review & Merge**: Open/review PR from `feat/rc2-product-completion` -> `main` (verified at `6a867172a8bf30cfac941391c88513579f22d6e7` + closeout docs) after CI validation.
2. **Optional Bounded Live Acceptance (`GATE-03`)**: When operator supplies provider credentials, run bounded `LiveAcceptanceHarness` with `AGENT_STUDIOS_LIVE_ACCEPTANCE=1` inside an isolated test budget (`max_budget_usd <= 0.25`).
3. **Production Signing & NSIS Build (`GATE-04`)**: Install NSIS (`makensis`) and sign `AgentStudios.exe`, `agsd.exe`, `ags.exe`, and generated installer binaries with production Authenticode certificate before GA release.
