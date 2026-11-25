# 00 — Executive Summary: Agent Studios Post-RC1 Forensic Audit

> **Audited Baseline**: `main` / `integration/m15-unified-codebase` / `integration/m15-1-joint-validation` at SHA `00e5c862b8173cb29914a4ff8dfe4dbb1147e03c`
> **Audit Date**: 2026-11-25
> **CI Reference**: GitHub Actions Run `45597809243` (and `45590072851`, `45590064391`) — Ubuntu & Windows `completed / success`
> **Preserved RC1 Bundle**: `C:\AgentStudiosBuild\release-bundle\AgentStudios-v1.0.0-rc1-win-x64.zip` (`SHA-256: 3ce6d4e8fd307c78042bb297171af0c174382ef01f88435dd11c483e4c8c3eb9`)

---

## 1. What Agent Studios Actually Is Today

Agent Studios (`19` custom Rust crates totaling `275` `.rs` files / `109,030` lines, linked to `162` inherited `codex-rs` crates, plus `150` TypeScript files / `48,890` lines in `apps/agent-studios-vscode`) has achieved a **verified multi-crate Rust control plane, native IPC daemon (`agsd`), CLI/TUI (`ags`), and VS Code Mission Control extension baseline** merged cleanly to `main`.

However, an honest forensic audit against `VISION_AND_ORIGINAL_REQUIREMENTS.md` and `PRODUCT_REQUIREMENTS.md` demonstrates that **RC1 is a strong backend/CLI/extension engineering foundation — NOT yet a complete, standalone everyday desktop AI IDE product (`v1.0.0 GA`)**.

---

## 2. Truly Completed vs. Incomplete vs. Mock-Backed

### Actually Completed & Cross-Process Verified
- **Control Plane & Persistence**: `agent-studios-control-plane` (`8,446` SLOC) SQLite WAL store, single-writer lease, Kahn DAG scheduler, crash recovery reconciler, replay watermark backfill, and M18 12-worker / 1k+ turn / 100k event stress tests.
- **Native Authenticated IPC (`agsd`)**: `agent-studios-ipc-protocol` (`3,082` SLOC) + `agent-studios-ipc-transport` (`18,019` SLOC) Named Pipes (Windows) / UDS (Linux), HMAC two-phase challenge-response, privilege separation (`agsd.json` vs `agsd-admin.json`), tested against compiled `agsd.exe` (`84` E2E tests).
- **Provider Catalog, SecretStore & Protocol Adapters**: `agent-studios-provider` (`6,332` SLOC) + `agent-studios-protocol-adapters` (`11,497` SLOC) with WireMock adapters for Chat Completions, Anthropic Messages, Gemini `generateContent`, and Codex `Responses`.
- **Worktree Isolation & Runtime Containment**: `agent-studios-workspace` (`2,363` SLOC) ephemeral index capture + SHA-256 chunked diffs; `agent-studios-claude-code-runtime` (`7,572` SLOC) with Win32 Job Object + 2PC stdin approval; `agent-studios-opencode-runtime` (`5,014` SLOC).
- **CLI & TUI Foundation**: `agent-studios-cli` (`2,685` SLOC) + `agent-studios-tui` (`5,149` SLOC) including `ags`, `ags run --headless|--stdin`, `ags daemon`, `ags provider discover`, `ags skill|mcp|hooks|instructions`.
- **VS Code Extension Core**: 1-icon `Agents` activity bar, 6 TreeViews, Chat Webview, 9-tab Mission Control panel, EN/VI `LocalizationManager`, `CanonicalPaths`, `25` Extension Host integration tests.

### Incomplete or Missing for Standalone Desktop & Daily Coding Use
1. **No Standalone Desktop Executable (`AgentStudios.exe`)**: RC1 only stages `ags.exe`, `agsd.exe`, and `agent-studios-vscode-0.1.0.vsix`. There is no compiled Electron/Code-OSS host binary, launcher binary, or embedded daemon auto-spawner in RC1.
2. **Incomplete VS Code Daily Coding Intelligence**: `apps/agent-studios-vscode` lacks real `InlineCompletionItemProvider`, inline editing / `CodeActionProvider` quick fixes, context engine (repo maps, incremental index, budgeter), hunk-level diff accept/reject + checkpoints/revert, interactive provider add/test commands, and UI surfacing for Skills/MCP/Hooks/Automations.
3. **MCP HTTP Transport Incomplete**: `agent-studios-rs/crates/extensibility/src/mcp.rs` (lines 260–264, 342–346) rejects `McpTransportConfig::Http` with `"HTTP MCP transport requires live endpoint client"` and lacks JSON-RPC `initialize` lifecycle, `resources/list`, `resources/read`, and `prompts/list`.
4. **Upstream `codex-rs` Frozen Since `2026-10-05`**: `upstream/codex.lock.json` locks the snapshot at `d25c114d494ddb693290b76bf5e5f64ecbdb38fc` (`2026-10-02`) and internal patches ended at `1f042241ce` (`2026-10-05`); no newer upstream synchronization has been run since.
5. **Release Engineering Defects in RC1 Scripts**: `Build-WindowsReleaseBundle.ps1` defaulted to `debug`, hardcoded `m15-1-joint-validation` target path, staged to an unversioned overwritten directory, used fragile `Select-Object -First 1` VSIX globbing, and `Verify-ReleaseBundle.ps1` only ran `-DryRun` rather than real sandbox install/upgrade/uninstall acceptance.

### What Runs Under Mocks / Zero-Billable Boundary
- **Live Cloud LLM Execution (`REAL_AI_MODEL_NOT_TESTED`)**: All automated LLM protocol tests hit `wiremock::MockServer`, `FixtureBridge`, `ags --demo`, or deterministic local runtimes due to `GATE-03` (`$0.00` spend cap).

---

## 3. Blockers & Risks
- **Blockers**: `GATE-03` (operator API keys / spend budget for live cloud models), `GATE-04` (production Authenticode EV/OV certificate for signing), and absence of local NSIS compiler (`makensis` not installed on build host; PowerShell installer + `.nsi` blueprint present).
- **Risks**: Unchecked `352` TypeScript ESLint warnings (mostly snake_case Rust IPC DTOs triggering `naming-convention`, plus 2 `curly` warnings); `AgentStudios` desktop shell dependency on Code-OSS / `Code.exe` availability if not bundled directly; non-ancestor historical PRs `#15`, `#16`, `#20`.

---

## 4. Release Readiness Verdict

- **RC1 (`v1.0.0-rc1`)**: **VERIFIED AS ENGINEERING CLI/DAEMON/EXTENSION PRE-RELEASE** (`00e5c862b8`, CI Run `45597809243` green, `9` file checksums + `.zip` verified), **NOT** ready for `v1.0.0 GA` until Phase B (`AgentStudios.exe` standalone desktop), Phase C (daily coding extension features), Phase D (MCP HTTP + upstream review), Phase E (release engineering repair + real sandbox install test), `GATE-03`, and `GATE-04` are resolved.
