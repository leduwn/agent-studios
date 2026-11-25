# 07 — Runtime, Provider & Live Acceptance Audit

> **Zero-Billable Invariant**: Every automated run up to RC1 operated strictly at `$0.00` (`AGENT_STUDIOS_LIVE_CLI_SMOKE` unset, WireMock / deterministic runtimes).

---

## 1. Execution Tier Separation by Runtime Adapter

| Runtime Adapter | Source Crate | Fixtures / Mock Tests | Local Real Process / Worktree Tests | Live Cloud LLM Acceptance |
|---|---|---|---|---|
| **Internal Codex Engine** | `crates/internal-agent`, `crates/runtime-session`, `codex-rs` | `thread_e2e_tests` (`7/7` default-stack) via `wiremock::MockServer` (Chat Completions, Anthropic, Gemini, Responses) | Real SQLite state DB, temp workspace, tool budget enforcement, child/fork inheritance | **BLOCKED (`GATE-03`)** — requires real API keys & budget |
| **Claude Code Adapter** | `crates/claude-code-runtime` | `approval_concurrency_tests` (`12/12`), `runtime_lifecycle_tests` (`14/14`) | Real `claude.exe --version` verified in `RealAgentExecutionE2E`; `cli_smoke_tests` guarded by `AGENT_STUDIOS_LIVE_CLI_SMOKE=1` | **BLOCKED (`GATE-03`)** — guarded at `$0.00` |
| **OpenCode Adapter** | `crates/opencode-runtime` | `m11_provider_mapping_tests`, `m11_runtime_lifecycle_tests` (`wiremock` / fake CLI) | Cross-platform process containment (`setpgid` / Win32 Job) | **BLOCKED (`GATE-03`)** |
| **CLI Client (`ConnectedClient` / `EmbeddedClient`)** | `crates/cli-client`, `crates/cli` | `ags --demo run --headless --stdin` (`cli_acceptance_tests` `7/7`) | Real Git worktree diff + reconciliation + file writes (`p0_real_execution_acceptance_tests` `3/3`) | Missing unified `LiveAcceptanceHarness` with USD/token cap & kill switch (`Wave 6`) |

---

## 2. Verified Protocol, Security & Routing Controls

- **Secret Isolation**: `ProviderCatalog` stores only `SecretReference` locators (`env:VAR`, `keyring:service/acct`), never raw secrets. `SecretRedactor` scrubs API keys from NDJSON, diagnostics, hook stdout/stderr, and MCP summaries.
- **No Silent Protocol Fallback**: `test_thread_e2e_custom_protocol_failure_no_silent_fallback_to_responses` proves custom HTTP 500 provider failures surface explicitly rather than falling back to OpenAI `/responses`.
- **Two-Phase Commit (2PC) Approval Safety**: Failure to deliver approval on child `stdin` terminates the child process fail-closed (`claude-code-runtime`), and daemon IPC rejects forged/expired/replayed HMAC challenge tokens.
