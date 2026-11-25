# 06 — Standalone Desktop Shell & Installer Audit

> **Audit Verdict**: RC1 ships an extension + daemon/CLI bundle (`ags.exe`, `agsd.exe`, `.vsix`), **NOT** a standalone desktop executable (`AgentStudios.exe`).

---

## 1. Direct Component Presence Check

| Question | Observed Status at RC1 | Evidence |
|---|---|---|
| Standalone `AgentStudios.exe` executable exists? | **NO** | `agent-studios-rs/Cargo.toml` defines only `ags` (`crates/cli`) and `agsd` (`crates/ipc-transport`). |
| Custom Code-OSS executable compiled? | **NO** | No `code-oss` submodule or Electron build exists in repo. Host machine has `C:\Users\Duwn\AppData\Local\Programs\Microsoft VS Code\Code.exe`. |
| Desktop launcher & isolated profile directory? | **NO** | Users must open VS Code manually and install `.vsix`. |
| Embedded daemon auto-start, health & crash respawn? | **NO** (Desktop side) | `agsd` must be started externally or via test harness (`ExtensionBridge` only connects to already-running `agsd.json`). |
| Agent Mode lives in extension vs standalone shell? | **Extension (`apps/agent-studios-vscode`)** | Agent Mode commands (`openAgents`, `switchToAgentMode`, `openInIde`) drive VS Code sidebar/chat inside the extension host. |
| Compiled NSIS `.exe` setup binary? | **NO** | `makensis` is not installed on the build host; `AgentStudios.nsi` is an uncompiled NSIS blueprint alongside `Install-AgentStudios.ps1`. |
| Real installation acceptance test executed? | **NO** | `Verify-ReleaseBundle.ps1` only executed `-DryRun` on installer/uninstaller scripts. |
| Auto-updater pipeline? | **NO** | Not implemented (`installer/windows/` contains static copy installer only). |
| Missing runtime dependencies? | Node.js / Electron host (`Code.exe` / Code-OSS), optional external CLIs (`claude`, `opencode`), Git on `PATH` | `ags doctor --json` checks Git, Rust binaries, external runtimes, and paths. |

---

## 2. Phase B (`Wave 2–3`) Architectural Resolution

To eliminate the gap honestly and ship a true standalone launcher without duplicating backends:
1. Build `agent-studios-rs/crates/desktop` producing **`AgentStudios.exe`**.
2. `AgentStudios.exe` supervises `agsd.exe` lifecycle (auto-start when absent, authenticated health probe, automatic respawn on daemon crash, graceful admin shutdown when owned).
3. `AgentStudios.exe` provisions an isolated canonical profile (`%APPDATA%\AgentStudios\desktop-profile`, `%LOCALAPPDATA%\AgentStudios\extensions`) pre-installing the bundled Agent Studios extension, keeping the operator's personal VS Code config untouched, and launches Code-OSS / `Code.exe` directly into `--mode agent` or `--mode ide`.
4. Upgrade `Verify-ReleaseBundle.ps1` to perform a **real sandbox install -> custom config preservation upgrade -> installed `AgentStudios.exe --headless-check` execution -> clean uninstall** test instead of relying on `-DryRun` alone.
