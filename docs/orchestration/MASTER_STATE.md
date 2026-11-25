# Agent Studios — Master Orchestration State (`MASTER_STATE.md`)

> **Last Updated**: 2026-11-25T21:54:00+07:00
> **Active Milestone**: MASTER MISSION 04 — Extension-First Complete (`feat/extension-first-product-completion` Ready for Operator Review)
> **Canonical `main` Baseline SHA**: `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c` (protected; not auto-merged)
> **Verified Code & Packaging SHA**: `2e3195367f5365953fc34a905a9afa32c93f17ca`
> **Active Isolated Worktree**: `D:\Projects\agent-studios-active\m15-1-joint-validation`
> **Shared Cargo Target Dir**: `C:\AgentStudiosBuild\cargo-target-rc2`
> **Preserved RC1 Bundle (Immutable)**: `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc1-win-x64.zip` (`SHA-256: 3ce6d4e8fd307c78042bb297171af0c174382ef01f88435dd11c483e4c8c3eb9`)
> **Preserved RC2 Bundle**: `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc2-6a867172a8-win-x64.zip` (`SHA-256: 49a7a6c45affb88e5f3f11cba43b3f36a392af9b617ecc0df50cd5e31425dbda`)
> **Packaged Extension-First VSIX (`v1.1.0`)**: `C:\AgentStudiosBuild\release-bundle\agent-studios-vscode-1.1.0-win32-x64.vsix` (`SHA-256: c5d33695419db6028a35adc9ece00d939185d36806e306596b473ce4a560a622`)

---

## 1. Completed Gates & Release Posture

- **RC1 & RC2 Preserved**: `main` remains at `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c`; RC1 and RC2 ZIP hashes re-verified unchanged.
- **Standalone Desktop Priority Adjustment**: Standalone Code-OSS desktop distribution (`crates/desktop`, `installer/windows/*`, NSIS installer, desktop auto-updater) marked `DEFERRED_BY_OPERATOR` in MASTER MISSION 04 and preserved in-tree.
- **Human Approval Gates**:
  - `GATE-01` & `GATE-02`: **RESOLVED**.
  - `GATE-03` (`$0.00` live LLM billable spend cap) and `GATE-04` (production Authenticode signing certificate) remain **ACTIVE** by design.

## 2. Extension-First Engineering Completion Matrix (Waves 0–12)

| Wave | Scope | Commit SHA | Status |
|---|---|---|---|
| Wave 0–1 | Parity Audit against `openai.chatgpt` `26.1117.41037` / `codex.exe` `0.171.0-alpha.13`, traceability & OSS research (`docs/reviews/EXTENSION-FIRST-PRODUCT-COMPLETION/`) | `8beed235ee` | `done` |
| Wave 2 | Upstream `codex-rs` sync (`d25c114d49` -> `44dd77b71e`, 26 commits / 152 files, 4 patches preserved) | `cf223bd286` | `done` |
| Wave 3–6 (Backend) | 12-provider catalog (`litellm`, `vllm` added) & authoritative IPC commands (`UpsertProvider`, `RemoveProvider`, `DiscoverModels`, `TestProviderConnection`, `ForkStudio`, `ListExtensibility`) | `04a3457578` | `done` |
| Wave 4–10 (Frontend & E2E) | `EmbeddedDaemonSupervisor`, auto-start wiring, composer controls, `ContextEngine` provenance, zero-stub inline completion, `HunkReviewManager`, Mission Control `#dag-viewport-group` + DAG controls, Journeys A–G E2E & `Build-ExtensionFirstBundle.ps1` | `2e3195367f` | `done` |
| Wave 11–12 | Full Rust (`120 suites: 1022 passed`) + TS (`706 unit`, `89 E2E`, `25 integration`) gates, self-contained `v1.1.0` VSIX + clean-profile VS Code install smoke test, closeout report & handoff ZIP | Closeout commit | `done` |
