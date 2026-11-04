# Milestone M12 — Implementation & Architecture Report

- **Milestone**: `M12 — Claude Code CLI Runtime Adapter & Subagent Event Correlation`
- **Execution Run**: `RUN-20261027-M12-claude-code`
- **Timestamp**: `2026-10-27T23:35:00+07:00`
- **Principal Technical Lead & Integration Owner**: Agent Studios Lead
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Base Commit**: `dc363d1e89fe8751c001f9829bd2cd0b9f7d37f9` (`origin/main`, PR #14 merged)
- **Active Branch**: `feat/m12-claude-code-runtime`
- **Crate**: `agent-studios-rs/crates/claude-code-runtime` (`agent-studios-claude-code-runtime`)
- **Status**: **IMPLEMENTED & VERIFIED**

---

## 1. Executive Implementation Summary

Milestone M12 delivers a vendor-neutral, production-grade CLI runtime adapter for Anthropic's Claude Code CLI (`agent-studios-claude-code-runtime`), integrating seamlessly with the `agent-studios-external-runtime` framework established in M10/M10.1 and complementing the OpenCode adapter delivered in M11.

The implementation strictly avoids mock-only or simulated completions, providing full bidirectional NDJSON communication (`stream-json`), native subagent correlation, pre-tool human-in-the-loop approvals, dollar budget enforcement, Windows OS process containment via Win32 Job Objects, and comprehensive error handling.

### Key Metrics
- **Crate Package**: `agent-studios-claude-code-runtime` (v0.1.0, Rust edition 2024).
- **Core Modules**: 11 modules (`discovery`, `protocol`, `subagent`, `stream`, `mapping`, `process`, `sandbox`, `approval`, `runtime`, `error`, `lib`).
- **Test Suites**: 4 integration test files containing 29 automated tests (100% pass rate).
- **Clippy Cleanliness**: 0 warnings under `-D warnings` across all targets.
- **Code Containment**: Zero lines modified in `codex-rs/` (strictly read-only and immutable).

---

## 2. Architecture & Module Design

```
+---------------------------------------------------------------------------------------------------------+
|                                    agent-studios-external-runtime                                       |
|                               (AgentRuntime trait, SessionEventHub)                                     |
+---------------------------------------------------------------------------------------------------------+
                                                     ▲
                                                     │ implements AgentRuntime
+----------------------------------------------------+----------------------------------------------------+
|                                    agent-studios-claude-code-runtime                                    |
|                                                                                                         |
|  +--------------------+   +-----------------------+   +----------------------+   +-------------------+  |
|  |     discovery      |   |        protocol       |   |       subagent       |   |      stream       |  |
|  | (CLI discovery &   |   | (NDJSON wire Serde    |   | (SubagentCorrelation |   | (Bounded NDJSON   |  |
|  |  capabilities)     |   |  data models)         |   |  Tracker & DAG)      |   |  reader 16MiB/64) |  |
|  +--------------------+   +-----------------------+   +----------------------+   +-------------------+  |
|            │                          │                          │                         ▲            |
|            ▼                          ▼                          ▼                         │            |
|  +--------------------+   +-----------------------+   +----------------------+             │            |
|  |      mapping       |   |        runtime        |   |       approval       |             │            |
|  | (Wire msg ->       |   | (ClaudeCodeRuntime &  |   | (ApprovalAuthority   |             │            |
|  |  RuntimeEventKind) |   |  ActiveSession)       |   |  fail-closed)        |             │            |
|  +--------------------+   +-----------------------+   +----------------------+             │            |
|                                       │                                                    │            |
|                                       ▼                                                    │            |
|                           +-----------------------+   +----------------------+             │            |
|                           |        process        |──▶|       sandbox        |             │            |
|                           | (Child supervision,   |   | (Win32 Job Objects,  |             │            |
|                           |  stdio piping)        |   |  memory/proc quotas) |             │            |
|                           +-----------------------+   +----------------------+             │            |
+---------------------------------------│----------------------------------------------------│------------+
                                        │ stdio pipes                                        │
                                        ▼                                                    │
                   +-----------------------------------------+                               │
                   |        claude.cmd / claude.exe          |───────────────────────────────+
                   |  --print --output-format=stream-json    | (stdout stream)
                   |  --input-format=stream-json             |
                   |  --permission-prompts=host              |
                   +-----------------------------------------+
```

### 2.1 `discovery.rs` — Binary Discovery & Capability Declaration
- **Resolution Pipeline**: Discovers Claude Code CLI through:
  1. Configured custom binary override (`custom_binary_path`), which fails closed without silent fallback if non-existent.
  2. Environment variables: `CLAUDE_CODE_BIN`, `CLAUDE_BIN`.
  3. Windows user profile directories: `%USERPROFILE%\.local\bin\claude.exe`.
  4. Global npm directory: `%APPDATA%\npm\claude.cmd` / `claude.exe`.
  5. Unix user/system paths: `~/.local/bin/claude`, `/usr/local/bin/claude`, `/opt/homebrew/bin/claude`.
  6. System `PATH` executable search.
- **Version Probing**: Executes candidate with `--version` and parses semver string (e.g. `2.1.310 (Claude Code)`).
- **Capability Profile**: Accurately advertises supported runtime capabilities:
  - `streaming_events`: Supported
  - `interrupt`: Supported
  - `resume`: Supported
  - `stop`: Supported
  - `persistent_session`: Supported
  - `workspace_binding`: Supported
  - `model_selection`: Supported
  - `provider_selection`: Unsupported (delegated internally by Claude Code to Anthropic)
  - `tools`: Supported
  - `approvals`: Supported
  - `mcp`: Supported
  - `skills`: Supported
  - `image_input`: Supported
  - `structured_output`: Supported
  - `subagents`: Supported
  - `usage_reporting`: Supported

### 2.2 `protocol.rs` — NDJSON Wire Serialization Models
- **Envelope Discriminated Union**: Complete enum `ClaudeStreamMessage` matching wire types:
  - `SystemMessage`: `init`, `task_started`, `task_notification`, `api_retry`, `status`.
  - `StreamEventWrapper`: Incremental streaming deltas (`content_block_delta` with `text_delta`, `thinking_delta`, `input_json_delta`).
  - `AssistantMessage`: Completed assistant turns containing `ContentBlock::Text`, `ContentBlock::ToolUse`, and `ClaudeTokenUsage`.
  - `UserMessage`: User turns and `ToolResultBlock::ToolResult`.
  - `ToolProgressMessage`: In-flight tool progress indicators with elapsed time.
  - `ControlRequest`: Inbound host approval requests (`can_use_tool`).
  - `ControlResponse`: Outbound host approval decisions (`allow` / `deny`).
  - `ResultSummary`: Final turn completion summary (`is_error`, `result`, `errors`, `usage`).
  - `ClaudeUserPromptInput`: Standard stdin prompt submission payload (`type: "user"`).
  - `ClaudeInterruptInput`: Stdin turn interruption signal (`type: "interrupt"`).

### 2.3 `subagent.rs` — Native Subagent Hierarchy Correlation
- **No Hallucinated Synthetic IDs**: Correlates subagents directly to upstream Claude Code native wire attributes (`parent_tool_use_id`, `tool_use_id`, `task_id`).
- **DAG Correlation**: `SubagentCorrelationTracker` maintains a thread-safe Directed Acyclic Graph of subagent task invocations:
  - Registers subagent spawns on `system/task_started` with parent resolution.
  - Tracks execution depth, status (`running`, `completed`, `failed`), and elapsed duration.
  - Resolves parent tool use ancestry without loss of hierarchical context.

### 2.4 `stream.rs` — Bounded NDJSON Streaming Pump
- **Defensive Buffering**: `NdjsonStreamReader` wraps child `stdout` with strict DoS limits:
  - Maximum line length: 16 MiB (`MAX_LINE_LENGTH = 16 * 1024 * 1024`).
  - Maximum buffer capacity: 64 MiB (`MAX_BUFFER_CAPACITY = 64 * 1024 * 1024`).
- **Line Parsing**: Normalizes CRLF and LF delimiters, skips blank lines, and provides diagnostic context on malformed payloads.

### 2.5 `mapping.rs` — Normalized Runtime Event Translation
- Translates `ClaudeStreamMessage` events into vendor-neutral `agent_studios_external_runtime::RuntimeEventKind`:
  - `SystemMessage::Init` -> `RuntimeEventKind::SessionStarted`
  - `SystemMessage::TaskStarted` -> `RuntimeEventKind::Diagnostic` (subagent telemetry)
  - `SystemMessage::TaskNotification` -> `RuntimeEventKind::ToolCompleted` + `UsageUpdated`
  - `StreamDelta::TextDelta` -> `RuntimeEventKind::OutputDelta`
  - `ContentBlock::ToolUse` -> `RuntimeEventKind::ToolStarted`
  - `ToolResultBlock::ToolResult` -> `RuntimeEventKind::ToolCompleted`
  - `ControlRequestPayload::CanUseTool` -> `RuntimeEventKind::ApprovalRequested`
  - `ResultSummary` -> `RuntimeEventKind::Completed` / `RuntimeEventKind::Failed`

### 2.6 `process.rs` — Process Supervision & Flag Assembly
- **Invocation Invariants**: Launches Claude Code CLI with headless automation flags:
  ```powershell
  claude --print --output-format=stream-json --input-format=stream-json --permission-prompts=host --forward-subagent-text --session-id <uuid>
  ```
- **Windows Process Creation**:
  - Sets `CREATE_NO_WINDOW` (0x08000000) on Windows to prevent console window flicker.
  - Pipes standard I/O: `stdin` (piped), `stdout` (piped), `stderr` (piped).
- **Diagnostics Ring Buffer**: Captures last 64 KiB of `stderr` in a circular ring buffer (`STDERR_RING_BUFFER_LIMIT`), ensuring diagnostic availability on child failure without unbounded memory growth.

### 2.7 `sandbox.rs` — Windows OS Process Containment
- **Win32 Job Object Isolation**:
  - Configures `JobObjectGuard` via Windows API `CreateJobObjectW`.
  - Sets `JOBOBJECT_EXTENDED_LIMIT_INFORMATION`:
    - `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`: Guarantees immediate child and descendant termination when the Job handle is closed or adapter drops.
    - `JOB_OBJECT_LIMIT_JOB_MEMORY`: Restricts total committed memory for the process tree (default: 4 GiB).
    - `JOB_OBJECT_LIMIT_ACTIVE_PROCESS`: Caps maximum concurrent child processes (default: 64).
  - Assigns child process via `AssignProcessToJobObject` and validates membership with `IsProcessInJob`.
- **Truthful Security Boundaries**:
  - `SandboxLevel::Strict` fails closed on Windows MSVC with `ClaudeCodeError::SandboxPolicyViolation` because Win32 Job Objects do not provide OS-level filesystem or network namespace isolation without Hyper-V containerization.
- **Workspace Sanitization**:
  - `validate_session_workspace` and `validate_workspace_path` canonicalize directories, reject path traversal escapes (`..`), and fail closed on `ExecutionWorkspace::SharedSource`.

### 2.8 `approval.rs` — Pre-Execution Tool Approvals & Budget Limits
- **Approval Authority**:
  - Generates deterministic UUIDv5 `ApprovalId` derived from wire `request_id`.
  - Asynchronous waiting with timeouts and fail-closed default (`ApprovalDecision::Deny`).
  - Wire formatter `to_control_response_value` emitting NDJSON frame:
    ```json
    {"type":"control_response","response":{"subtype":"success","request_id":"<id>","response":{"behavior":"allow"|"deny"}}}
    ```
- **Budget Validation**:
  - `validate_max_budget_usd` strictly validates `--max-budget-usd`.
  - Rejects negative numbers, `NaN`, positive/negative infinity, and non-numeric strings.
- **Canary Redaction**:
  - `sanitize_diagnostic` scrubs sensitive credentials (`sk-ant-*`, Bearer tokens, GitHub PATs, URI passwords) before publishing diagnostic events.

### 2.9 `runtime.rs` — `AgentRuntime` Trait Implementation
- **Session Lifecycle Supervision**:
  - Tracks active sessions in `Arc<RwLock<BTreeMap<RuntimeSessionId, ClaudeCodeActiveSession>>>`.
  - Integrates `SessionEventHub`: Emits `RuntimeEventKind::SessionStarted` as sequence 1.
  - Maintains monotonic sequence ordering and gapless replay subscription.
- **Contract Enforcement**:
  - Enforces `RuntimeLifecycleState` state machine (`Starting` -> `Running` -> `Interrupted` / `Stopped` / `Completed` / `Failed`).
  - Reject actions on terminal states with `RuntimeError::TerminalStateError`.
  - Supports non-destructive interruption (`interrupt`) and turn resumption (`resume`).
  - Idempotent session termination (`stop`).

---

## 3. Upstream Intellectual Property & License Containment

In accordance with Agent Studios governance policies established in M11:
1. **Quarantine of `777genius/agent-teams-ai`**:
   - Licensed under AGPL-3.0.
   - Clean-room design only. Zero source code, snippets, or derivative implementations copied into Agent Studios.
2. **`openai/codex` Synchronization & Immutability**:
   - `codex-rs/` remained strictly read-only and immutable throughout M12.
   - Zero lines modified. `git status` verifies clean tree in `codex-rs/`.
   - Upstream patches (001–004) in `CODEX_PATCHES.md` preserved intact.
3. **Automated Intelligence Monitoring**:
   - Scheduled GitHub Actions workflow `.github/workflows/upstream-intelligence.yml` active to monitor upstream releases weekly without auto-merging.

---

## 4. Test Suite Execution & Verification

### Test Breakdown
| Test Suite | File | Tests | Passed | Failed |
|---|---|:---:|:---:|:---:|
| **Runtime Lifecycle** | `tests/runtime_lifecycle_tests.rs` | 10 | 10 | 0 |
| **Sandbox Containment** | `tests/sandbox_containment_tests.rs` | 4 | 4 | 0 |
| **Security & Budget** | `tests/security_and_budget_tests.rs` | 7 | 7 | 0 |
| **Stream & Subagents** | `tests/stream_and_subagent_tests.rs` | 8 | 8 | 0 |
| **Total** | | **29** | **29** | **0** |

All tests pass consistently with zero failures. Quality gates (`cargo check`, `cargo clippy -- -D warnings`, `cargo test`) confirmed 100% compliant.
