# MASTER MISSION 05 — Clean Isolated VSIX Acceptance Receipts

## 1. Packaged VSIX Provenance
- **Artifact Path**: `C:\AgentStudiosBuild\release-bundle\agent-studios-vscode-1.1.1-f87e284915-win32-x64.vsix`
- **Manifest Path**: `C:\AgentStudiosBuild\release-bundle\EXTENSION_MANIFEST_1.1.1-f87e284915.json`
- **Version**: `1.1.1`
- **Commit SHA**: `f87e284915c34695219214afd46cdde6b0bf421e` (`f87e284915`)
- **Profile**: `release`
- **Package Size / Contents**: 71.08 MB, 101 files (self-contained)
- **VSIX SHA-256**: `1da4a7ac5d393768c3ae391df8a5e9e0bb5d0b76d9e56c6fcb3fc42bdef23e51`
- **Bundled Binaries**:
  - `extension/bin/win32-x64/agsd.exe`: `9c7f1a2cdfdd6312a17f9759880f73c3fbc25abb018fbd4099b80fb19c771da6` (185.95 MB)
  - `extension/bin/win32-x64/ags.exe`: `dccc503169609d191e451bd276136a61e92d0f6f7a0055ed5932be4cb3785ccf`

## 2. Isolated Profile Installation & Extension Host Execution (AGSD_BIN_PATH Unset)
- **Environment Condition**: `Remove-Item Env:\AGSD_BIN_PATH` (explicitly unset; zero fallback to cargo target dir).
- **VS Code Binary**: `apps/agent-studios-vscode/.vscode-test/vscode-win32-x64-archive-1.148.0/Code.exe` & `bin/code.cmd`
- **Clean Profile**: Isolated `--user-data-dir` and `--extensions-dir` in temporary profile `ags-clean-vsix-f22df2db`.
- **CLI Install Verification**:
  - `--install-extension ...agent-studios-vscode-1.1.1-f87e284915-win32-x64.vsix --force` -> succeeded
  - `--list-extensions --show-versions` -> `agent-studios.agent-studios-vscode@1.1.1`
- **Real Extension Host Verification Output**:
```json
{
  "cleanVsixVerified": true,
  "installedExtRoot": "C:\\Users\\Duwn\\AppData\\Local\\Temp\\ags-clean-vsix-f22df2db\\extensions\\agent-studios.agent-studios-vscode-1.1.1",
  "resolvedBundledAgsd": "C:\\Users\\Duwn\\AppData\\Local\\Temp\\ags-clean-vsix-f22df2db\\extensions\\agent-studios.agent-studios-vscode-1.1.1\\bin\\win32-x64\\agsd.exe",
  "extensionVersion": "1.1.1",
  "daemonPid": 29904,
  "readinessRuntimes": [
    "codex"
  ],
  "discoveredModels": [
    "codex-clean-vsix"
  ],
  "replyThreadId": "01a21425-1a25-71c1-9e40-6c1495fe7211",
  "replyContent": "CLEAN_VSIX_TURN_VERIFIED: TURN1-TOKEN-VSIX"
}
```
- **Extension Host Exit Status**: `Extension host with pid 31192 exited with code: 0, signal: unknown.` (`==> Clean VSIX Acceptance PASSED (exit 0)!`).
- **Verified Behaviors Inside Installed VSIX**:
  1. Installed VSIX activated inside real VS Code Extension Host (`vscode.extensions.all` -> `agent-studios-vscode@1.1.1`, `isActive === true`).
  2. `EmbeddedDaemonSupervisor.resolveAgsdBinary` resolved the bundled `bin\win32-x64\agsd.exe` directly from the installed extension directory with `AGSD_BIN_PATH` unset.
  3. Bundled `agsd.exe` booted autonomously over Windows Named Pipe and reported `available_runtimes: ["codex"]`.
  4. Wrong credential test provider rejected with HTTP 401; valid `SecretStorage` -> `DaemonSecretVault` credential discovered `codex-clean-vsix` via HTTP 200 `/v1/models`.
  5. Streaming Codex chat turn created real `ThreadId` (`01a21425-1a25-71c1-9e40-6c1495fe7211`), received streamed `text_delta`, and injected both `AGENTS.md` (`ALWAYS_MENTION_VSIX_OK`) and `$audit` `SKILL.md` (`CLEAN_VSIX_SKILL_RULE`) into the real HTTP inference request payload.
  6. Gracefully shut down bundled daemon via authenticated admin IPC `shutdown`.
