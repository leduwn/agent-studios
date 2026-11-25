# 04 — Granular Feature Capability Matrix (Post-RC1 Audit)

> **Audit Standard**: Distinguishes cleanly across 8 verification tiers instead of using a single coarse `VERIFIED_IMPLEMENTED` badge:
> `Exists` | `Implemented` | `Unit Tested` | `Integration Tested` | `E2E Tested` | `Works in Real App` | `Live-Model Verified` | `Production-Ready`.

| ID | Feature / Product Requirement | Source Files | Exists | Impl | Unit | Integ | E2E | Real App | Live LLM | Prod Ready | Remaining Gap |
|---|---|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|---|
| `F-01` | Control Plane DAG & SQLite WAL Recovery | `crates/control-plane/src/*` | Y | Y | Y | Y | Y | Y (`agsd`) | N/A | **Y** | Production telemetry tuning only |
| `F-02` | Native IPC Named Pipe/UDS & HMAC Auth | `crates/ipc-protocol`, `crates/ipc-transport` | Y | Y | Y | Y | Y (`84`) | Y (`agsd.exe`) | N/A | **Y** | Production cert hardening (`GATE-04`) |
| `F-03` | Provider Catalog, Keyring & 6-Stage Discovery | `crates/provider/src/*`, `crates/cli/src/commands/provider.rs` | Y | Y | Y | Y (WireMock) | Y | Partial (CLI only) | Blocked (`GATE-03`) | Partial | Interactive VS Code UI provider CRUD missing |
| `F-04` | Internal Codex Multi-Agent Supervisor | `crates/internal-agent`, `crates/runtime-session`, `codex-rs` | Y | Y | Y | Y | Y (WireMock) | Partial (`ags --demo`) | Blocked (`GATE-03`) | Partial | Upstream sync tracking (`codex.lock.json`) missing |
| `F-05` | Claude Code CLI Adapter & Job Object Sandbox | `crates/claude-code-runtime/src/*` | Y | Y | Y | Y | Y (shim) | Cli `--version` | Blocked (`GATE-03`) | Partial | Opt-in live smoke (`AGENT_STUDIOS_LIVE_CLI_SMOKE=1`) gated |
| `F-06` | OpenCode External Runtime Adapter | `crates/opencode-runtime/src/*` | Y | Y | Y | Y | Mock/CLI | Partial | Blocked (`GATE-03`) | Partial | Live OpenCode server test gated |
| `F-07` | Physical Git Worktree & Lossless SHA-256 Diff | `crates/workspace/src/*` | Y | Y | Y | Y | Y (`17` + E2E) | Y | N/A | **Y** | Chunk pagination verified lossless |
| `F-08` | `ags` CLI & Headless NDJSON Automation | `crates/cli/src/*`, `crates/cli-client/src/*` | Y | Y | Y | Y | Y (`7` CLI + `3` P0) | Y (`ags.exe`) | Blocked (`GATE-03`) | Partial | Live acceptance harness wrapper needed (`Wave 6`) |
| `F-09` | Interactive `ratatui` TUI Connected to `agsd` | `crates/tui/src/*`, `crates/cli/src/main.rs` | Y | Y | Y | Y | Partial | Y (`ags`) | Blocked (`GATE-03`) | Partial | Needs interactive PTY test for connected mode |
| `F-10` | M16 Skills Engine (`SKILL.md` scopes + install) | `crates/extensibility/src/skills.rs` | Y | Y | Y | Partial | Partial | CLI only | N/A | Partial | Not wired into live VS Code UI (`Wave 4`) or runtime prompts |
| `F-11` | M16 MCP Engine (Stdio + Streamable HTTP) | `crates/extensibility/src/mcp.rs` | Y | Partial | Partial (Stdio) | N | N | CLI `list` only | N/A | **N** | **GAP**: HTTP transport returns unsupported; missing `initialize`, `resources`, `prompts` (`Wave 5`) |
| `F-12` | M16 Lifecycle Hooks & Hierarchical Instructions | `crates/extensibility/src/{hooks.rs,instructions.rs}` | Y | Y | Y | Partial | N | CLI only | N/A | Partial | Needs automatic `pre_turn`/`post_task` execution in agent pipeline |
| `F-13` | VS Code Workbench Core (Chat, 6 Trees, Mission Control) | `apps/agent-studios-vscode/src/*` | Y | Y | Y (`700`) | Y (`25`) | Y (`84`) | Y (Ext Host) | Blocked (`GATE-03`) | Partial | Needs daily coding Groups 1–6 (`Wave 4`) |
| `F-14` | Bilingual Localization (`en-US`/`vi-VN`) & Decoupling | `apps/agent-studios-vscode/src/i18n/*`, `extension.ts` | Y | Y | Y | Y | N | Y | N/A | Partial | Webviews still use mostly hardcoded English literals; need i18n wiring |
| `F-15` | Daily Coding Intelligence (Inline Comp/Edit, Context, Hunks) | *(Missing modules)* | **N** | **N** | **N** | **N** | **N** | **N** | **N** | **N** | **CRITICAL GAP**: Planned for `Wave 4` (`RC2`) |
| `F-16` | Standalone Desktop Shell (`AgentStudios.exe`) & Daemon Auto-Spawn | *(Missing `crates/desktop`)* | **N** | **N** | **N** | **N** | **N** | **N** | N/A | **N** | **CRITICAL GAP**: Planned for `Wave 2–3` (`RC2`) |
| `F-17` | Windows Installer, Upgrade, Rollback & Real Install Acceptance | `installer/windows/*`, `scripts/packaging/*` | Y | Partial | N | N | N (`DryRun` only) | **N** | N/A | **N** | **GAP**: Packaging fixes (`debug` default, hardcoded paths, missing real install test) in `Wave 7` |
