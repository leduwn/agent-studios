# Agent Assignments & File Boundary Contracts

| Role | Scope & Responsibilities | Allowed Mutating Paths | Forbidden Paths |
|---|---|---|---|
| **Agent A — Backend & Control Plane** | Control plane invariants, SQLite WAL durability, crate architecture | `agent-studios-rs/Cargo.toml`, `crates/control-plane/**`, `crates/extensibility/**` | `codex-rs/**`, `apps/**` |
| **Agent B — Frontend & Desktop Experience** | VS Code / Code-OSS extension, EN/VI localization, canonical path settings | `apps/agent-studios-vscode/**` | `agent-studios-rs/**`, `codex-rs/**` |
| **Agent C — Runtime & Provider Engineering** | `cli-client` `ConnectedClient`, `ClaudeCodeRuntime`/`OpenCodeRuntime` probing, local worktree actions | `agent-studios-rs/crates/cli-client/**` | `apps/**`, `codex-rs/**` |
| **Agent D — CLI, TUI & Extensibility** | `ags run`, `ags daemon`, `ags provider discover`, `ags skill/mcp/hooks/instructions` | `agent-studios-rs/crates/cli/**`, `agent-studios-rs/crates/extensibility/**` | `apps/**`, `codex-rs/**` |
| **Agent E — Independent QA & Security** | P0 real execution tests, CLI acceptance tests, M18 stress & concurrency/security tests | `**/tests/**` | `codex-rs/**` |
| **Agent F — Release & DevOps** | Git integration, Windows release bundler, NSIS/PowerShell installer, M19 release guides | `scripts/packaging/**`, `installer/**`, `docs/release/**`, `docs/orchestration/**` | `codex-rs/**` |

**Cross-Agent Invariant**: No two mutating workers modify the same file concurrently. `codex-rs/**` is strictly read-only upstream heritage.
