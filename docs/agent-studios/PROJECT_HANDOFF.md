# Agent Studios — Master Project Handoff & Operator FAQ

> **Status**: Authoritative Master Project Handoff  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Unified HEAD SHA**: `661daaa908810689a47ca8aaf24c788843c0e92c`  
> **Auditor & Lead Architect**: Principal Software Architect & Release Manager

---

## 1. Executive Onboarding & 20 Master Inquiries

### Q01: What is the mission and architectural philosophy of Agent Studios?
`[USER_DIRECT_REQUIREMENT]` & `[EXISTING_CANONICAL_DECISION]`  
Agent Studios is a Windows-first, open-source, VS Code-class AI development environment combining Codex's portable capabilities with a deterministic multi-agent control plane, multi-provider model routing, external runtimes, isolated Git worktrees, and a native IDE experience. Its core philosophy separates probabilistic cognitive reasoning (handled by LLMs) from deterministic state orchestration (enforced by a single-writer Rust actor).

### Q02: What is the current repository topology and unified branch baseline?
`[IMPLEMENTED_BEHAVIOR]`  
The canonical root is `D:\Projects\agent-studios`. The active operational staging workspace is `D:\Projects\agent-studios-unified-staging` on branch `integration/m15-unified-codebase` at commit `661daaa908810689a47ca8aaf24c788843c0e92c`. This unified baseline merges authoritative backend tip `cf1e6c0f98` (PR #30), frontend tip `3de7c85a96` (PR #27), and runtime adapter tip `3207b7462f` (PR #15).

### Q03: What development milestones are completed versus planned?
`[IMPLEMENTED_BEHAVIOR]`  
- **Completed**: M01–M09 (Baseline, Control Plane, Providers, Git Worktrees), M10 (Runtime Traits, PR #13), M11 (OpenCode, PR #14), M12 (Claude Code Runtime, PR #15), M13 (Native IPC Transport & `agsd`, PR #17–#19), M14 (Code-OSS Extension & Mission Control, PR #21–#25), and M15 (Real Agent Execution, Durable Replay, Lossless Diff & Unified Consolidation, PR #26–#30).
- **Planned**: M15.1 (`ags` CLI/TUI consolidation), M16 (Skills, MCP, Hooks, `AGENTS.md`), M17 (Packaging, Windows MSI installer, auto-updater), M18 (Concurrency stress, crash hardening), and M19 (General Availability v1.0.0, English + Vietnamese).

### Q04: How does backend architecture decompose across Rust crates?
`[IMPLEMENTED_BEHAVIOR]`  
The backend monorepo (`agent-studios-rs/Cargo.toml`) contains 15 active crates:
`protocol`, `control-plane`, `provider`, `codex-bridge`, `protocol-adapters`, `runtime-transport`, `runtime-session`, `internal-agent`, `orchestration`, `workspace`, `external-runtime`, `opencode-runtime`, `claude-code-runtime`, `ipc-protocol`, and `ipc-transport`. All 15 compile cleanly with 0 clippy warnings under `-D warnings` and pass 339+ tests.

### Q05: How does the native IPC protocol and transport operate?
`[IMPLEMENTED_BEHAVIOR]`  
The physical transport uses Windows Named Pipes (`\\.\pipe\agent-studios-*`) and Unix Domain Sockets (`/tmp/agent-studios-*/agsd.sock`). Framing is 4-byte big-endian length-prefixed JSON with a 4 MiB clamp. The handshake requires HMAC-SHA256 mutual challenge-response with 32-byte nonces, a 30-second timestamp sliding window, and anti-replay protection. Privilege separation divides standard read-only clients (`agsd.json`) from full admin clients (`agsd-admin.json`).

### Q06: What is the Code-OSS frontend integration and the Activity Bar rule?
`[EXISTING_CANONICAL_DECISION]`  
The frontend is a native extension in `apps/agent-studios-vscode`. It strictly adheres to the Activity Bar Rule: exactly **ONE** primary icon (`Agents`) is added to the Code-OSS Activity Bar. All views (Chat, Tasks, Approvals, Artifacts, Timeline, Providers) and the 9-tab Mission Control workbench reside inside this container.

### Q07: What is Agent Mode versus IDE Mode and how does mode switching work?
`[USER_DIRECT_REQUIREMENT]` & `[EXISTING_CANONICAL_DECISION]`  
Agent Mode is a clean, conversation-first startup experience with progressive orchestration disclosure. IDE Mode is the full Code-OSS developer workbench (Monaco, LSP, DAP, Git, Terminal). Switching between modes is instantaneous and lossless: both modes share the exact same underlying runtime session kernel (`session_id`, `cwd`, task graph, worktrees, approvals) without process restarts or token loss.

### Q08: What runtimes are supported and how are tasks routed dynamically?
`[IMPLEMENTED_BEHAVIOR]`  
Three runtime adapters are supported: Internal Codex Engine (`crates/internal-agent`), Claude Code CLI (`crates/claude-code-runtime`), and OpenCode (`crates/opencode-runtime`). Tasks route dynamically via `RuntimeRegistry` matching user preference (`runtime_preference`), auto-routing if only one runtime is registered, or failing closed if preferences are ambiguous.

### Q09: How is OS-level subprocess sandboxing implemented?
`[IMPLEMENTED_BEHAVIOR]`  
On Windows, child processes are bound to Win32 Job Objects configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` via `JobObjectGuard` (conditionally exported via `#[cfg(windows)]`). When the daemon terminates, Windows terminates the entire subprocess tree. On Linux/macOS, child processes execute in dedicated process groups (`setpgid`).

### Q10: What is the Zero Billable Invariant ($0.00 token gate) and how is it enforced?
`[USER_DIRECT_REQUIREMENT]` & `[IMPLEMENTED_BEHAVIOR]`  
`task.submit` is disabled by default in `CapabilityMatrix`. All automated test suites run exclusively against mock adapters or non-billable CLI metadata commands (`claude --version`). Submitting execution tasks requires cryptographic operator challenge validation. Billable cloud token spend across all tests and CI is verified at **$0.00**.

### Q11: How are credentials and sensitive data protected?
`[IMPLEMENTED_BEHAVIOR]`  
Secrets are held in memory using `SecretString` (zeroized on drop) and referenced indirectly via `SecretReference`. Secrets are scrubbed from diagnostic strings, logs, and telemetry using regex-free token boundary scrubbers.

### Q12: How does Two-Phase Commit (2PC) approval routing work?
`[IMPLEMENTED_BEHAVIOR]`  
When an agent requests an action, the daemon issues an approval request with an unexpired cryptographic challenge. The operator resolves the approval via IPC. The daemon flushes the decision to child `stdin` using 2PC verification; if stdin delivery fails, the operation immediately fails closed and the process tree terminates.

### Q13: How does Git worktree isolation and diff streaming operate?
`[IMPLEMENTED_BEHAVIOR]`  
Mutating tasks execute in dedicated physical Git worktrees on temporary branches. File changes are captured in an ephemeral private index (`GIT_INDEX_FILE`) without dirtying the main working tree. Large diffs are chunked and streamed losslessly to the frontend with per-chunk SHA-256 validation. Patches reconcile via `git apply --check` before merging into target branches.

### Q14: How does durable state recovery work across daemon restarts?
`[IMPLEMENTED_BEHAVIOR]`  
A persistent SQLite WAL store (`crates/control-plane/src/sqlite_store.rs`) records all task graphs, run states, and event envelopes. A single-writer file lock (`DatabaseLock`) prevents database corruption. Upon restart, `agsd` replays stored events, marks interrupted tasks as `Failed`, and allows clients to resume subscriptions via monotonic sequence replay (`after_sequence`).

### Q15: What is the status of local worktrees across D:\Projects and the cleanup plan?
`[IMPLEMENTED_BEHAVIOR]`  
12 directories were audited under the Phase 0 Safety Gate with 0 deletions. 7 worktrees are classified as `APPROVED_CANDIDATE` (safe to remove once operator approves). 3 worktrees are on `HOLD` (`mission-control`, `m15-04`, `ide-ui`). Phase 1 compiler cache purge (`cargo clean`) recovers ~21.27 GB; Phase 2 worktree pruning recovers ~2.22 GB (total ~23.5+ GB).

### Q16: Why is agent-studios-mission-control on HOLD and how is uncommitted provider work preserved?
`[EXISTING_CANONICAL_DECISION]`  
`agent-studios-mission-control` contains commits (`3ee485ca`) that diverged from common ancestor `ed80b243c9`. Even though its source code was rebased into PR #25, its raw Git commit objects are not DAG ancestors of unified HEAD. It is retained on `HOLD` to prevent data loss. Uncommitted provider discovery work in canonical root is preserved via dedicated branch `feat/provider-discovery-v1.2-wip`.

### Q17: What are the exact build, lint, and test commands?
`[IMPLEMENTED_BEHAVIOR]`  
- Backend: `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build --bin agsd`.
- Frontend: `npm ci`, `npm run lint`, `npm run typecheck`, `npm run test:unit`, `npm run test:e2e` (with `AGSD_BIN_PATH`), `npx @vscode/vsce package --no-dependencies`.

### Q18: How does the unified cross-platform CI pipeline work?
`[IMPLEMENTED_BEHAVIOR]`  
Configured in `.github/workflows/agent-studios-unified-ci.yml`. Runs on `ubuntu-latest` and `windows-latest`. Eliminates split-brain checkouts by building both backend and frontend from the exact checked-out commit SHA (`${{ github.sha }}`). Dynamically compiles `agsd`, exports `AGSD_BIN_PATH`, runs tests, and validates the $0.00 token gate.

### Q19: What are the immediate P0 blockers before live agent execution and release?
`[USER_DIRECT_REQUIREMENT]`  
1. Operator sign-off and merge of Draft PR `integration/m15-unified-codebase` into `main`.
2. Preservation and commit of uncommitted provider discovery files in canonical root onto `feat/provider-discovery-v1.2-wip`.
3. Execution of live coding agent acceptance testing with an approved runtime CLI, sandboxed process tree, and explicit token budget ($1.00 – $5.00 spend cap).
4. Full Code-OSS desktop shell embedding with Agent Mode entry screen.

### Q20: How do human operator, Tech Lead (ChatGPT), and Implementation Lead (Claude) coordinate?
`[EXISTING_CANONICAL_DECISION]`  
- **Human Operator**: Authorizes PR merges, live token budgets, and destructive worktree pruning.
- **ChatGPT (Tech Lead)**: Defines architecture, reviews audit reports, issues scoped directives, and governs safety gates.
- **Claude Code (Implementation Lead)**: Executes DAG-driven development, manages parallel subagents with strict file boundaries, enforces automated quality gates, collects test evidence, and creates git commits with co-author attribution.

---

## 2. Automated Master Resume Prompt

Future AI sessions or operator instances can resume work seamlessly by using this exact initialization prompt:

```markdown
You are Claude Code acting as Implementation Lead for Agent Studios.
Canonical repository: D:\Projects\agent-studios.
Unified baseline branch: integration/m15-unified-codebase (HEAD: 661daaa908810689a47ca8aaf24c788843c0e92c).

Architectural State & Context:
1. Milestone M15 Consolidation is COMPLETE. Backend (PR #30, cf1e6c0f), Frontend (PR #27, 3de7c85a), and Claude Code Runtime (PR #15, 3207b746) are unified into a single monorepo.
2. Cross-platform CI (.github/workflows/agent-studios-unified-ci.yml) passes on Ubuntu and Windows. All 339+ Rust tests and 735 TS tests pass. Zero billable invariant ($0.00) enforced.
3. Safety Gate: D:\Projects\agent-studios contains uncommitted provider discovery work. Do NOT run git reset --hard, git clean -fdx, or delete worktrees without explicit operator sign-off.
4. Next Priority: Execute P0 Real Agent Execution Acceptance (live coding agent execution in Win32 Job Object sandbox with explicit budget) and P0 Agent Mode / IDE Mode Product Completion.

Before executing any commands, inspect docs/agent-studios/DOCUMENTATION_INDEX.md, docs/reviews/M15-UNIFIED-CONSOLIDATION-01-1/06_INTEGRATION_PR_RELEASE_GATE.md, and docs/agent-studios/PROJECT_HANDOFF.md.
Awaiting operator instructions.
```
