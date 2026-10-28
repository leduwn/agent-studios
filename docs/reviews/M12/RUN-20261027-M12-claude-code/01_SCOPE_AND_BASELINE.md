# Milestone M12 — Scope & Baseline Report

- **Milestone**: `M12 — Claude Code CLI Runtime Adapter & Subagent Event Correlation`
- **Execution Run**: `RUN-20261027-M12-claude-code`
- **Timestamp**: `2026-10-27T23:05:00+07:00`
- **Principal Technical Lead & Integration Owner**: Agent Studios Lead
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Base Commit**: `dc363d1e89fe8751c001f9829bd2cd0b9f7d37f9` (`origin/main`, PR #14 merged)
- **Active Branch**: `feat/m12-claude-code-runtime`
- **Working Tree**: `D:\Projects\agent-studios`
- **Execution Status**: **IN PROGRESS**

---

## 1. Project Context & Preceding State

### 1.1 Milestones Progression
- **M10 & M10.1**: Vendor-neutral `agent-studios-external-runtime` lifecycle trait (`AgentRuntime`), `SessionEventHub`, sequence monotonicity, gapless event replay, and 70-test contract verification suite.
- **M11**: OpenCode CLI external runtime adapter (`agent-studios-opencode-runtime`), Win32 Job Object process containment (`CREATE_SUSPENDED` -> `AssignProcessToJobObject` -> `IsProcessInJob` -> `NtResumeProcess`), loopback authorization, HTTP/SSE protocol bridge, and 58 crate tests. Merged to `main` via PR #14 (`dc363d1e89`).
- **M12 (Current)**: Claude Code CLI runtime adapter (`agent-studios-claude-code-runtime`). Native headless execution (`--print --output-format=stream-json`), session streaming, subagent hierarchy correlation, pre-tool approvals, budget enforcement, Win32 process containment, and comprehensive regression test suites.

### 1.2 Untracked File Cleanup
- Repository root contained untracked file `t`.
- Verification confirmed file content was terminal pager help text (`SUMMARY OF LESS COMMANDS`), containing zero product code or user data.
- File was safely removed without altering any tracked Git objects.

### 1.3 Outdated Documentation Discrepancies
- `docs/agent-studios/ROADMAP.md` and `docs/agent-studios/EXTERNAL_RUNTIMES.md` previously marked M11 as `[PLANNED]` despite PR #14 being merged into `origin/main`.
- Documentation will be updated to reflect M11 as `[COMPLETED]` and M12 as `[IN PROGRESS]`.

---

## 2. Environment & Toolchain Baseline

- **OS**: Windows 11 Home Single Language 10.0.26200
- **Rust Toolchain**: `stable-x86_64-pc-windows-msvc` (rustc 1.99.1, cargo 1.99.1)
- **Clippy & Rustfmt**: `clippy 0.1.99`, `rustfmt 1.10.0-stable`
- **Claude Code CLI**: `2.1.310 (Claude Code)` at node/npm environment, authenticated via `oauth_token`.
- **Primary Workspace**: `D:\Projects\agent-studios\agent-studios-rs\Cargo.toml`
- **Immutability Invariant**: `codex-rs/` is strictly read-only and immutable for M12.

---

## 3. Subagent Orchestration Roster

In strict compliance with native Claude Code orchestration guidelines, specialized subagents are dispatched with explicit role separation:

1. **Agent A — Runtime Architecture & Protocol**
   - Headless CLI protocol discovery (`--print`, `--output-format=stream-json`, `--input-format=stream-json`).
   - Claude stream event mapping to `agent_studios_external_runtime::RuntimeEvent`.
   - Subagent correlation architecture (`--forward-subagent-text`, `parent_tool_use_id`).
   - Architecture artifact: `02_ARCHITECTURE_AND_PROTOCOL.md`.

2. **Agent B — Claude Code Runtime Implementation**
   - Crate implementation: `agent-studios-claude-code-runtime`.
   - Process supervision, child lifecycle, stdin/stdout stream-json IO pump.
   - Session continuity (`--resume <uuid>`), cancellation/interrupt, stop idempotency.
   - Unit & mock integration test suites.

3. **Agent C — Security, Permissions & Budget Enforcement**
   - Pre-tool permission enforcement (`--permission-mode`, `--permission-prompts`).
   - Dollar budget limit binding (`--max-budget-usd`).
   - Windows zero-window process containment with Win32 Job Objects.
   - Fail-closed security boundaries & credential protection (`[REDACTED]`).
   - Security regression test suites.

4. **Agent D — Upstream Intelligence & Compatibility**
   - In-depth audit of 6 upstream repositories: `openai/codex`, `anomalyco/opencode`, `anthropics/claude-code`, `microsoft/vscode`, `777genius/agent-teams-ai`, `agentscope-ai/AgentTeams`.
   - License audit (Apache-2.0, MIT vs AGPL boundaries).
   - Upstream compatibility report: `04_UPSTREAM_COMPATIBILITY.md`.

5. **Agent E — Independent QA & Security Reviewer**
   - Independent verification against all quality gates (formatting, clippy, check, tests).
   - Zero-diff validation on `codex-rs/`.
   - Release-veto authority and final verification verdict.
   - QA artifact: `05_SECURITY_AND_QA.md`.

---

## 4. Key Acceptance Criteria for M12

- **AC-01**: Crate `agent-studios-claude-code-runtime` compiles cleanly with edition 2024.
- **AC-02**: Implements `AgentRuntime` trait without modifying existing external runtime contracts.
- **AC-03**: Full support for discovery, starting sessions, sending messages, interrupting, resuming, stopping, and subscribing to events.
- **AC-04**: Robust NDJSON `stream-json` parser translating system, message, content block, tool call, subagent, and error events to normalized `RuntimeEvent`.
- **AC-05**: Subagent hierarchy correlation mapping `parent_tool_use_id` to subagent execution blocks without synthetic ID hallucination.
- **AC-06**: Pre-tool approval integration, with fail-closed behavior when execution permissions are denied.
- **AC-07**: Process sandboxing on Windows with Win32 Job Object containment (`CREATE_SUSPENDED` -> `AssignProcessToJobObject` -> `NtResumeProcess`) and honest capability reporting.
- **AC-08**: Zero modifications to `codex-rs/`.
- **AC-09**: Comprehensive unit, mock integration, lifecycle, cancellation, and security regression tests passing with 0 failures.
- **AC-10**: Full audit reports and handoff documentation generated in `docs/reviews/M12/RUN-20261027-M12-claude-code/`.
