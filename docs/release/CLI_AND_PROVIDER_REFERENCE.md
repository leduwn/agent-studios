# Agent Studios — CLI & Provider Reference (`ags` & `agsd`)

## 1. `ags` CLI Commands

| Command | Purpose | Exit Codes |
|---|---|---|
| `ags` / `ags --demo` / `ags --preview` | Launch interactive TUI (or demo/preview mode) | `0` clean exit |
| `ags doctor [--json]` | Run 8-point environment & `agsd` diagnostic check | `0` |
| `ags provider list\|add\|get\|remove\|import\|test\|discover` | Manage BYOK provider instances & dynamic model discovery | `0` / non-zero on error |
| `ags model list\|get\|add\|remove\|test` | Inspect and manage registered `ModelRef` descriptors | `0` / non-zero on error |
| `ags run [--headless] [--stdin] [--demo] [--runtime <rt>] [--auto-approve <tier>] "<GOAL>"` | Execute goal with NDJSON stream & deterministic exit codes | `0` success, `1` failure/timeout, `2` approval blocked, `3` config/IPC error |
| `ags daemon status\|health [--json]` | Inspect `agsd` IPC bootstrap file, health & recovery status | `0` / non-zero if offline on `health` |
| `ags skill list\|inspect\|install\|enable\|disable` | Manage `SKILL.md` skills safely (Global/Project/Agent) | `0` / non-zero on validation error |
| `ags mcp list [--json]` | Inspect configured MCP servers with secrets redacted | `0` |
| `ags hooks list [--json]` | Inspect configured lifecycle hooks | `0` |
| `ags instructions [--cwd <path>] [--json]` | Resolve hierarchical `AGENTS.md`/`CLAUDE.md` within worktree bounds | `0` / non-zero on boundary escape |

## 2. Supported Wire Protocols & OpenAI-Compatible Custom Endpoints

- `openai-responses`: OpenAI Responses API & compatible endpoints
- `openai-chat`: OpenAI Chat Completions (DeepSeek, OpenRouter, Groq, vLLM, LM Studio, custom `--endpoint`)
- `anthropic-messages`: Anthropic Messages API (`x-api-key` & `anthropic-version`)
- `gemini-generate-content`: Google Gemini GenerateContent API
- `ollama`: Local Ollama (`http://localhost:11434`, auto-discovers `/api/tags`)
