# Agent Studios — Original Requirements & Deferred Scope Traceability

> **Generated**: 2026-11-25T20:16:00+07:00
> **Authority**: MASTER MISSION 04 (Extension-First Priority Update)

## 1. Classification of Requirements

| Source Requirement | Classification | Current Target | Status / Disposition |
|---|---|---|---|
| VS Code Extension AI Coding Workbench (`apps/agent-studios-vscode`) | `DIRECT_USER_REQUIREMENT` | Extension-First Primary Product | Active across Waves 1–12 |
| Native Codex Engine (`codex-rs`) synchronized cleanly via snapshot + Patches 001–004 | `CANONICAL_DECISION` | `codex-rs/` + `upstream/codex.lock.json` | Upgrading `d25c114d49` -> `44dd77b71e` |
| Authoritative Rust Control Plane (`agsd`) + SQLite WAL + Worktree Isolation | `CANONICAL_DECISION` | `crates/control-plane`, `crates/ipc-transport` | Extended with Provider/Discovery/Extensibility/Fork IPC |
| Single Activity Bar Icon Rule (`agent-studios-activitybar`) | `CANONICAL_DECISION` | `apps/agent-studios-vscode/package.json` | Enforced & regression-tested |
| Full BYOK 12-Provider Catalog + `SecretStore` + Dynamic `/v1/models` Discovery (`GATE-03` $0.00 tested via local HTTP mock) | `DIRECT_USER_REQUIREMENT` | `crates/provider`, `ProviderManagementCommands.ts` | Removing hardcoded `CATALOG_MODELS` fallback; wiring real IPC discovery |
| 9-Tab Mission Control & Interactive SVG DAG | `CANONICAL_DECISION` | `apps/agent-studios-vscode/src/mission-control/` | Interactive pan/zoom/fit/minimap/keyboard nav/follow |
| Shared `ags` CLI / `agsd` Daemon Session Parity | `DIRECT_USER_REQUIREMENT` | `crates/cli`, `crates/cli-client`, `crates/ipc-transport` | Maintained seamlessly; no competing stores |
| Self-Contained VSIX Bundling Native `agsd.exe` + `ags.exe` (mirroring `openai.chatgpt` `bin/windows-x86_64`) | `UPSTREAM_INSPIRED_IDEA` | `scripts/packaging/Build-ExtensionFirstBundle.ps1` + `EmbeddedDaemonSupervisor.ts` | Eliminates manual `cargo build` requirement for end users |
| Standalone Code-OSS Electron/Chromium Bundle, NSIS Desktop Installer, Desktop Auto-Updater | `DEFERRED_BY_OPERATOR` | `crates/desktop`, `installer/windows/*` | Preserved intact; deferred by operator priority update |
