# 08 — Upstream Synchronization & R&D Status

> **Policy Reference**: `MASTER MISSION 02`, `docs/agent-studios/UPSTREAM.md`, `docs/agent-studios/CODEX_PATCHES.md`.

---

## 1. Tracked Upstream & Reference Repositories

| Repository | URL | License | Local Baseline / Lock | Last Internal Touch / Reviewed | Strategy & Status | Compatibility Risks & Recommended Next Action |
|---|---|---|---|---|---|---|
| **OpenAI Codex** (`codex-rs/`) | https://github.com/openai/codex | Apache-2.0 | `d25c114d494ddb693290b76bf5e5f64ecbdb38fc` (`upstream/codex.lock.json`, `2026-10-02`) | `1f042241ce` (`2026-10-05`) | Snapshot fork maintaining **Patches 001–004** (`ModelInferenceBackend`, `ModelRuntimeOverride`, `SpawnRequest` overrides, `thread_extension_init`). `0` edits during Waves 1–8. | Any sync post-`d25c114d` must re-apply & unit-test Patches 001–004. Keep `codex-rs` frozen during RC2 desktop/extension sprint unless targeted security fix is backported. |
| **Model Context Protocol (MCP Spec & `rmcp`)** | https://github.com/modelcontextprotocol/rust-sdk | MIT / Apache-2.0 | `rmcp v3.3.0` rev `3e636cab` (in `Cargo.lock` / `codex-rs`) | Reviewed in Wave 5 audit | `crates/extensibility/src/mcp.rs` implements partial Stdio JSON-RPC (`tools/list`, `tools/call`) while returning `McpError::Config` on HTTP. | Upgrade `mcp.rs` directly against MCP `2024-11-05` Streamable HTTP + `initialize` + `resources` + `prompts` in Wave 5–6. |
| **Anthropic Claude Code** | https://github.com/anthropics/claude-code | Proprietary CLI (process harness only) | CLI v`2.1.310` detected locally (`claude.exe`) | Verified `--version` (`RealAgentExecutionE2E`) | External CLI subprocess adapter (`crates/claude-code-runtime`) using `--output-format stream-json --verbose --bare` + 2PC stdin. | CLI flag drift across minor releases (`--verbose` required for stream-json). Covered by `cli_smoke_tests` & `RealAgentExecutionE2E`. |
| **OpenCode** | https://github.com/anomalyco/opencode | MIT | External adapter HTTP/CLI spec (`crates/opencode-runtime`) | Reviewed M11 | External runtime adapter + JSON config importer (`crates/provider/src/importer.rs`). | Keep adapter schema decoupled from upstream UI forks. |
| **Code-OSS / VS Code** | https://github.com/microsoft/vscode | MIT | Extension API `^1.90.0` (tested against `1.147`/`1.148` test archive) | Verified Run `45597809243` | Permanent IDE foundation + isolated `AgentStudios.exe` host profile launcher in Wave 2–3. | Electron/extension API deprecations; keep isolated `--extensions-dir` and `product.json`. |
| **Developer Agent Reference Set** (Cline, Roo Code, Kilo Code, Continue, Gemini CLI, Aider, OpenHands, Pi Coding Agent) | Open-source ecosystem | Apache-2.0 / MIT | Design & UX benchmarks (`MISSION_CONTROL_PRODUCT_SPEC.md`) | Synthesized into RC2 Wave 4 backlog | Architectural inspiration ONLY (no copied proprietary/conflicting code): inline completion/editing, repo map context engine, hunk-level diff accept/reject, conversation CRUD, provider setup UX. | Maintain strict Apache-2.0 cleanliness. |
