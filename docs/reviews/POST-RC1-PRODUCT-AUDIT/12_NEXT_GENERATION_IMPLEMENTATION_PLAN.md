# 12 — Next-Generation Implementation Plan (`RC2` → `RC3` → `GA`)

## 1. Execution DAG (`Waves 0–10`)

```mermaid
flowchart LR
    W0["Wave 0-1: RC1 Check & 16-File Post-RC1 Audit"] --> W2["Wave 2-3: crates/desktop (AgentStudios.exe + agsd Supervisor)"]
    W0 --> W4["Wave 4: VS Code Extension Groups 1-6"]
    W0 --> W5["Wave 5-6: Complete MCP (HTTP + Stdio) & LiveAcceptanceHarness"]
    W2 --> W7["Wave 7: RC2 Release Pipeline Repair & Sandbox Install Smoke"]
    W4 --> W7
    W5 --> W7
    W7 --> W8["Wave 8-10: Full Regression Gates, Desktop Launch & RC2 Report"]
```

## 2. Release Trajectory

- **RC2 (`v1.0.0-rc2`, Current Execution Target)**: Delivers `AgentStudios.exe` standalone launcher/supervisor, daily coding VS Code extension features (Groups 1–6), complete HTTP+Stdio MCP engine, `$0.00`-guarded `LiveAcceptanceHarness`, repaired version+SHA release packaging, and verified real sandbox install/upgrade/uninstall smoke tests.
- **RC3 (`GATE-03` Live Acceptance)**: Upon operator provision of API keys and `$1`–`$5` test budget, execute `LiveAcceptanceHarness` and `AGENT_STUDIOS_LIVE_CLI_SMOKE=1` against live Anthropic/OpenAI/Gemini models.
- **v1.0.0 GA (`GATE-04` Production Signing)**: Bundled Code-OSS Electron runtime distribution signed with production Authenticode EV/OV certificate (`makensis` `.exe` installer), auto-updater channel activation, and public release publish.
