# Agent Studios — Milestone M15.1 Joint Integration Preflight
## Document 07: Real Feature Acceptance Evidence & Execution Logs

**Milestone:** M15.1 (Joint Integration Preflight & Cross-Platform Release Gate)  
**Date:** 2026-11-06  
**Role:** Principal Integration Engineer & Release QA Lead  
**Worktree:** `D:\Projects\agent-studios-active\m15-1-joint-validation`  
**Status:** REAL FEATURE ACCEPTANCE VERIFIED (100%)  

---

### 1. Acceptance Scope & Live Execution Summary

In accordance with the M15.1 mandate, all feature acceptance is evaluated against authentic, compiled binaries (`ags`, `agsd.exe`) and real runtime environments (unmocked VS Code Extension Development Host via `@vscode/test-electron`). Synthetic mocks are restricted strictly to isolated unit testing.

| Feature Area / Test Suite | Scope & Target | Tests Executed | Passed | Failed | Billable Cost |
| :--- | :--- | :---: | :---: | :---: | :---: |
| **System Diagnostics (`ags doctor`)** | Native CLI binary | 8 checks | 8 | 0 | $0.00 |
| **Provider Discovery CLI** | Native CLI binary | 2 commands | 2 | 0 | $0.00 |
| **Frontend Pure Unit Suite** | Mocha / In-memory | 685 | 685 | 0 | $0.00 |
| **Real Native Daemon E2E Suite** | Live `agsd.exe` PID & WAL | 84 | 84 | 0 | $0.00 |
| **Real Extension Host Integration** | VS Code Electron v1.145.0 | 25 | 25 | 0 | $0.00 |
| **VSIX Package Packaging** | `vsce package` | 87 files | 1 pkg | 0 | $0.00 |
| **Total Acceptance Surface** | **Full Integrated Platform**| **794 + CLI** | **All Pass**| **0** | **$0.00** |

---

### 2. Native CLI Acceptance Evidence

#### 2.1 System Diagnostics: `ags doctor --json`
Executed directly on Windows workstation:
```json
{
  "arch": "x86_64",
  "checks": [
    {
      "details": "windows (x86_64, windows)",
      "is_critical": false,
      "name": "Platform",
      "status": "OK"
    },
    {
      "details": "git version 2.56.0.windows.3",
      "is_critical": true,
      "name": "Git Executable",
      "status": "OK"
    },
    {
      "details": "Root: D:/Projects/agent-studios-ide-ui | Status: clean worktree",
      "is_critical": false,
      "name": "Git Repository & Worktree",
      "status": "OK"
    },
    {
      "details": "120x30 (Meets recommended Standard geometry >= 100x28)",
      "is_critical": false,
      "name": "Terminal Geometry",
      "status": "OK"
    },
    {
      "details": "NO_COLOR environment variable active; monochrome fallback enforced",
      "is_critical": false,
      "name": "Color & ANSI Capabilities",
      "status": "INFO"
    },
    {
      "details": "Verified 69 grapheme clusters with intact NFC tone marks and zero diacritic clipping",
      "is_critical": false,
      "name": "Unicode NFC & Vietnamese Tone Pipeline",
      "status": "OK"
    },
    {
      "details": "Deterministic in-process engine ready (InMemoryStore + SystemClock, initialized 4 default agent roles)",
      "is_critical": false,
      "name": "Control Plane Engine",
      "status": "OK"
    },
    {
      "details": "Daemon (agsd) offline at \\\\.\\pipe\\agent-studios-default; standalone in-process EmbeddedClient fallback active",
      "is_critical": false,
      "name": "Daemon Service (agsd)",
      "status": "INFO"
    }
  ],
  "cli_version": "0.1.0",
  "git_available": true,
  "in_git_repo": true,
  "is_dirty": false,
  "os": "windows",
  "repo_root": "D:/Projects/agent-studios-ide-ui",
  "terminal_dimensions": [120, 30]
}
```

---

### 3. Real Daemon E2E Lifecycle & SQLite WAL Evidence

The E2E suite (`npm run test:e2e`) executes against a freshly spawned native `agsd.exe` process:

1. **Deterministic Binary Resolution:**
   `AgsdBinaryResolver.ts` located `D:\Projects\agent-studios\agent-studios-rs\target\debug\agsd.exe` via canonical parent repository traversal, rejecting stale worktree roots.
2. **Native Process Lifecycle:**
   - Real daemon spawned on Named Pipe `\\.\pipe\agent-studios-default`.
   - Process PID recorded: invariant throughout session switches.
3. **Authentication & Role Separation:**
   - Standard client authenticated with HMAC challenge; verified that `shutdown` request fails closed (`PermissionDenied`).
   - Admin client authenticated with admin key; verified clean exit code 0 on termination.
4. **SQLite WAL Persistence & Reconnect:**
   - Bridge disconnected, daemon preserved, bridge reconnected.
   - Verified that `session_id`, task progress, and message history rehydrate with zero task resubmission.
- **Result:** **84 / 84 tests passing cleanly**.

---

### 4. Real Extension Host Mode Continuity Evidence

Executed via unmocked VS Code Electron Host (`npm run test:integration`):
- Downloaded and launched authentic VS Code stable archive `v1.145.0`.
- Activated `agent-studios-vscode` extension inside Extension Host process (PID 57220).
- Verified registration and execution of commands:
  - `agent-studios.openInIde`
  - `agent-studios.switchToAgentMode`
  - `agent-studios.continueSession`
  - `agent-studios.openMissionControl`
  - `agent-studios.toggleFixturePreview`
- Verified virtual document provider (`agent-studios-diff` URI scheme) without workspace disk mutations.
- **Result:** **25 / 25 passing (13s elapsed, clean exit code 0)**.
