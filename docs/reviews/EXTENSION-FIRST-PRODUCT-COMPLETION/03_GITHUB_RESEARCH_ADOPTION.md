# Autonomous Research & Open-Source Reference Adoption Audit

> **Generated**: 2026-11-25T20:16:00+07:00
> **Compliance**: Only public OSS source, local installed extension manifests/layouts, and Apache/MIT-compatible patterns audited; zero proprietary code copied.

## Inspected References & Concrete Engineering Takeaways

| Candidate | Reference / Reviewed Version | License | Concrete Adopted Pattern in Agent Studios |
|---|---|---|---|
| **OpenAI Codex CLI & Engine** | `https://github.com/openai/codex.git` (`d25c114d49` -> synced target `44dd77b71e`) | Apache-2.0 | Minimal provider-neutral seams (Patches 001–004), `ModelInferenceBackend`, `ModelRuntimeOverride`, `SpawnRequest.thread_extension_init`, exec-server streaming & MCP sandbox checks. |
| **OpenAI Codex VS Code Extension** | Installed `openai.chatgpt` v`26.1117.41037` (`codex.exe` v`0.171.0-alpha.13`) | Proprietary wrapper around Apache-2.0 engine (manifest inspection only) | Single container view (`codexViewContainer`), bundled `bin/windows-x86_64/` binary layout + `codex-package.json`, `followUpQueueMode` (`steer` vs `queue`), `composerEnterBehavior`, clean-command palette entries. |
| **OpenCode** | Local `crates/opencode-runtime` adapter & public OpenCode config model | MIT | Clean Provider-vs-Runtime decoupling (`RuntimeInstanceId` vs `ModelRef`), local/self-hosted first-class provider configs. |
| **Cline (`saoudrizwan.claude-dev`)** | Local VS Code extension `saoudrizwan.claude-dev-4.1.27` layout | Apache-2.0 | Checkpoints before file mutation, explicit diff/hunk approval UX, context `@` mentions in composer. |
| **Continue / Aider** | Public repo-map & context budgeting patterns (documented in `docs/agent-studios/UPSTREAM_RESEARCH_AND_IDEAS.md`) | Apache-2.0 | Incremental symbol extraction + token-budgeted repo map + SHA-256 snippet deduplication (`ContextEngine.ts`). |
| **LiteLLM** | Public OpenAI-compatible multi-provider gateway spec | MIT | First-class gateway `ProviderDefinition` (`litellm`) in `crates/provider/src/builtin.rs` with `/v1/models` dynamic discovery. |
