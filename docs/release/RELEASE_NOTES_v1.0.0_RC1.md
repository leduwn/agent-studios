# Agent Studios v1.0.0-RC1 — Release Notes & Readiness Gate

> **Release Candidate**: `v1.0.0-rc1`  
> **Primary Target**: Windows 11 x64 (`win32-x64`)  
> **Secondary Targets**: Ubuntu 22.04+ / macOS ARM64  
> **Signing Status**: Unsigned Release Candidate (`GATE-04` pending operator Authenticode certificate)  
> **Live Cloud Token Acceptance**: Gated at `$0.00` (`GATE-03` pending operator budget & credential sign-off)

---

## 1. Delivered Architecture & Milestones

- **Deterministic Rust Control Plane & `agsd` Daemon**: Single-writer SQLite WAL event store, Kahn DAG task validation, Win32 Job Object sandboxing (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), POSIX `setpgid` containment, and HMAC-SHA256 authenticated IPC over Windows Named Pipes / UDS.
- **Multi-Runtime & Open Provider Ecosystem**: Unified `ModelRef(provider_instance_id, model_id)` identity across Internal Codex Engine, Claude Code runtime, and OpenCode runtime, with Provider Discovery v1.2, SecretStore zeroization, and Codex/OpenCode config import.
- **Official `ags` Developer CLI & TUI**: `ags doctor [--json]`, `ags provider list|add|get|remove|import|test|discover`, `ags model list|get|add|remove|test`, `ags daemon status|health`, `ags run [--headless] [--stdin]`, and M16 `ags skill|mcp|hooks|instructions`.
- **M16 Extensibility Engine (`agent-studios-extensibility`)**:
  - `SKILL.md` frontmatter parser, Global/Project/Agent scopes, progressive disclosure, dependency verification, and symlink/traversal-safe installation (no automatic script execution).
  - MCP configuration loader (`~/.agent-studios/mcp.json`, `.agent-studios/mcp.json`), secret redaction, risk-tiered approval gating, and stdio JSON-RPC tool discovery/invocation.
  - Lifecycle hooks (`pre_turn`, `post_task`, `on_reconciliation`, `on_approval`) with hard timeouts and fail-closed policies.
  - Hierarchical `AGENTS.md` and `CLAUDE.md` discovery strictly bounded inside Git worktree roots.
- **IDE Extension & Localization (`apps/agent-studios-vscode`)**:
  - One Activity Bar Icon (`Agents`), 9-tab Mission Control workbench, zero-loss mode switching (`openInIde` ↔ `switchToAgentMode`).
  - Bilingual English (`en-US`) and Vietnamese (`vi-VN`) UI localization strictly decoupled from LLM agent response language.
  - Canonical Windows paths (`%APPDATA%\AgentStudios\config.toml`, `%LOCALAPPDATA%\AgentStudios\state.db`, `%LOCALAPPDATA%\AgentStudios\artifacts\`).

## 2. Explicit GA Blockers (Operator Sign-Off Gates)

1. **Protected Branch Merges (`GATE-01`)**: Merge PR #33 → #31 → #34 → #32 into `main`.
2. **Live Cloud AI Spend Acceptance (`GATE-03`)**: Execute budgeted ($1–$5) live Anthropic/OpenAI/Gemini end-to-end tests once operator authorizes live credentials.
3. **Code Signing & Full Code-OSS Electron Shell Packaging (`GATE-04`)**: Apply production Authenticode signing certificate and bundle full Code-OSS Electron runtime binaries for public GA distribution.
