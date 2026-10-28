# Milestone M12 — Architecture & Protocol Specification
# Claude Code CLI External Runtime Adapter & Subagent Event Correlation

- **Milestone**: `M12 — Claude Code CLI Runtime Adapter & Subagent Event Correlation`
- **Document**: `02_ARCHITECTURE_AND_PROTOCOL.md`
- **Execution Run**: `RUN-20261027-M12-claude-code`
- **Timestamp**: `2026-10-27T23:30:00+07:00` (UTC: `2026-10-27T16:30:00Z`)
- **Author**: Agent A (`runtime-architecture-and-protocol`)
- **Status**: **APPROVED SPECIFICATION FOR AGENT B & AGENT C**
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Target Crate**: `agent-studios-rs/crates/claude-code-runtime` (`agent-studios-claude-code-runtime`)
- **Base Commit**: `dc363d1e89fe8751c001f9829bd2cd0b9f7d37f9` (`main`)
- **Branch**: `feat/m12-claude-code-runtime`

---

## 1. Executive Summary & Architectural Goals

### 1.1 Context & Strategic Objectives
Milestone M12 introduces the **Claude Code CLI Runtime Adapter** into Agent Studios. Following the successful delivery of vendor-neutral lifecycle traits in M10/M10.1 (`agent-studios-external-runtime`) and the OpenCode adapter in M11 (`agent-studios-opencode-runtime`), M12 enables Agent Studios to orchestrate Anthropic's official Claude Code CLI (`claude`) as a first-class external execution engine.

Claude Code represents a distinct architectural paradigm compared to OpenCode:
1. **Black-Box Binary Supervision**: Claude Code is governed by Anthropic's proprietary commercial terms. Zero source code copying, internal reverse-engineering, or in-memory linking is permitted. All interactions occur strictly across standard OS process boundaries using the official CLI.
2. **Headless Bidirectional NDJSON IPC**: Rather than exposing an HTTP/SSE server like OpenCode, Claude Code provides an advanced, high-throughput newline-delimited JSON (NDJSON) streaming interface over standard input and standard output (`--output-format=stream-json` and `--input-format=stream-json`).
3. **Native Hierarchical Subagents**: Claude Code natively orchestrates subagents (via the built-in `Agent` and `Skill` tools). Subagent telemetry is interleaved over the main stdout stream using explicit correlation pointers (`parent_tool_use_id`), eliminating the need for synthetic ID invention.
4. **Host-Controlled Pre-Execution Approvals**: When run with `--permission-prompts=host`, the CLI pauses execution before running potentially destructive tools (e.g. bash commands, file modifications, git pushes) and delegates approval authority to the host process via typed control messages on stdin/stdout.

### 1.2 Core Architectural Invariants
- **Strict Black-Box Process Boundary**: The adapter supervises the official `claude.exe` (Windows) or `claude` (macOS/Linux) binary as an isolated child process.
- **Zero-Diff Immutability on `codex-rs/`**: In accordance with project governance, `codex-rs/` remains completely untouched.
- **Contract Adherence**: Implements the `AgentRuntime` trait defined in `agent-studios-external-runtime` without breaking existing M10/M11 contracts.
- **Win32 Job Object Containment**: Inherits and applies the zero-window Windows process sandboxing pattern established in M11 (`CREATE_SUSPENDED` -> `AssignProcessToJobObject` -> `IsProcessInJob` -> `NtResumeProcess`) with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
- **Non-Synthetic Subagent Correlation**: Preserves and surfaces native Claude Code subagent parentage directly from `parent_tool_use_id` and `agent_id` without generating synthetic correlation identifiers.

---

## 2. Claude Code CLI Headless Execution Architecture

### 2.1 CLI Version Baseline
- **Installed Version**: `2.1.310 (Claude Code)` (Bun-packaged standalone executable at `C:\Users\Duwn\.local\bin\claude.exe`).
- **Target Release Line**: Claude Code `v2.1.300` through `v2.1.311+`.
- **Compatibility Requirement**: The adapter must support both global PATH resolution and explicit custom binary paths via `RuntimeStartRequest` configuration or environment variable `CLAUDE_CODE_BIN`.

### 2.2 Execution Invocation Profile
To operate Claude Code in headless, real-time streaming mode within Agent Studios, the child process is launched with the following authoritative CLI flags:

```bash
claude \
  -p \
  --output-format=stream-json \
  --input-format=stream-json \
  --include-partial-messages \
  --forward-subagent-text \
  --include-hook-events \
  --replay-user-messages \
  --session-id <UUIDv4> \
  --permission-mode <MODE> \
  --permission-prompts host \
  --no-session-persistence \
  [--max-budget-usd <AMOUNT>] \
  [--model <MODEL>] \
  [--allowedTools <TOOLS>] \
  [--disallowedTools <TOOLS>] \
  [--bare]
```

### 2.3 Detailed CLI Flag Analysis & Responsibilities

| Flag | Role in Agent Studios | Behavioral Specification |
|---|---|---|
| `-p`, `--print` | Headless Execution | Forces non-interactive mode. Suppresses TTY terminal UI, interactive confirmation prompts, spinners, and ANSI formatting. Skips workspace trust interactive dialog. |
| `--output-format=stream-json` | Real-Time Output Stream | Formats stdout as newline-delimited JSON (NDJSON). Every line is an independent JSON object terminating with `\n`. |
| `--input-format=stream-json` | Real-Time Input Stream | Enables stdin to accept NDJSON messages in real time for user turns, interactive tool approval responses, and host interrupt requests. |
| `--include-partial-messages` | Token-Level Streaming | Emits fine-grained streaming chunks (`stream_event` containing `content_block_delta` for text and thinking) for immediate UI responsiveness. |
| `--forward-subagent-text` | Subagent Transparency | Forwards subagent text and thinking blocks over stdout as `assistant` and `user` messages with `parent_tool_use_id` set to the spawning `Agent` tool call. |
| `--include-hook-events` | Lifecycle Visibility | Streams all hook lifecycle events (`hook_started`, `hook_progress`, `hook_response`) into the diagnostic pipeline. |
| `--replay-user-messages` | Stdin Acknowledgment | Causes CLI to re-emit stdin messages back on stdout as `type: "user"` frames, confirming ingestion into the agent transcript. |
| `--session-id <uuid>` | Deterministic Identity | Binds the CLI session to the exact `RuntimeSessionId` allocated by Agent Studios, ensuring end-to-end trace correlation. |
| `--resume [uuid]` | Session Continuity | Resumes an existing conversation history from disk (`~/.claude/transcripts/<project>/<uuid>.jsonl`). |
| `--fork-session` | Branching Context | Clones the transcript of the target session into a new session ID when resuming, preserving the original. |
| `--no-session-persistence` | Ephemeral Workspaces | Disables disk transcript persistence for stateless runs, sandboxed CI executions, or ephemeral tasks. |
| `--permission-mode <mode>` | Policy Base Mode | Sets base permission posture: `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan`. Default in Agent Studios: `manual` or `auto`. |
| `--permission-prompts host` | Approval Delegation | Bypasses local TTY prompts and emits `control_request` NDJSON events on stdout, pausing the loop until `control_response` is sent on stdin. |
| `--allowedTools` / `--disallowedTools` | Tool Sandboxing | Comma- or space-separated tool name filter (e.g. `Bash(git *) Edit`, `PowerShell`, `WebFetch`). |
| `--max-budget-usd <amount>` | Cost Ceiling | Hard upper dollar ceiling on Anthropic API spend. Exceeding triggers graceful abort with exit code 2. |
| `--bare` | Minimal Profile | Skips auto-memory, plugins, keychain reads, LSP prefetches, and background auto-discovery. Recommended for isolated task runs. |

### 2.4 Stdio Separation & Stream Health Invariants
1. **Standard Output (`stdout`)**:
   - **Strict NDJSON Invariant**: Every single line emitted on stdout is a valid JSON object string followed by `\n`.
   - **Zero Terminal Noise**: No ANSI escape codes, terminal cursor jumps, or raw text banners are written to stdout.
   - **Control Character Escaping**: Per Claude Code v2.1.311, control characters (ASCII 0x00–0x1F, DEL 0x7F, and Unicode control blocks) are strictly escaped as `\uXXXX`.
2. **Standard Error (`stderr`)**:
   - Reserved strictly for human-readable diagnostics, low-level process crash stacks, Node/Bun startup errors, and background worker status notices (`Waiting for background work to finish...`).
   - Stderr is captured into a bounded circular ring buffer (64 KiB) for diagnostic attribution and is never mixed into stdout.
3. **Process Exit Codes**:
   - `0`: Normal completion of requested turns and final `result` emitted.
   - `1`: Fatal error (syntax error, missing credentials, tool crash, prompt rejection).
   - `2`: Budget ceiling reached (`--max-budget-usd`) or authentication rejected.
   - `130`: Interrupted via `SIGINT` (Ctrl+C).
   - `143`: Terminated via `SIGTERM`.

---

## 3. Claude Code NDJSON Stream Protocol Wire Specification

### 3.1 Common Envelope Structure
All top-level messages emitted on stdout adhere to the base envelope schema:

```json
{
  "type": "system | assistant | user | tool_progress | tool_use_summary | result | control_request | control_response | stream_event | prompt_suggestion",
  "uuid": "String (UUIDv4 of the event)",
  "session_id": "String (UUIDv4 matching --session-id)"
}
```

### 3.2 Wire Message Schemas (Stdout)

#### 1. `system` (Lifecycle, Diagnostics, Tasks, Hooks)
Discriminator: `subtype`.

##### `system/init` (Emitted once at startup)
```json
{
  "type": "system",
  "subtype": "init",
  "uuid": "8c0a2a4b-550e-42c2-8412-1a2b3c4d5e6f",
  "session_id": "9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d",
  "claude_code_version": "2.1.310",
  "model": "claude-sonnet-5",
  "tools": ["Bash", "Read", "Edit", "Write", "Glob", "Grep"],
  "mcp_servers": [{ "name": "memory", "status": "connected" }],
  "plugins": [],
  "capabilities": ["interrupt_receipt_v1", "interrupt_cancel_queued_v1"],
  "output_style": "default",
  "fast_mode_state": "off"
}
```

##### `system/task_started` (Subagent / Background Task Spawn)
```json
{
  "type": "system",
  "subtype": "task_started",
  "uuid": "...",
  "session_id": "...",
  "task_id": "task_abc123",
  "tool_use_id": "toolu_01_agent_spawn",
  "description": "Explore repository files",
  "task_type": "subagent",
  "is_backgrounded": false,
  "spawn_depth": 1
}
```

##### `system/task_notification` (Subagent / Background Task Complete)
```json
{
  "type": "system",
  "subtype": "task_notification",
  "uuid": "...",
  "session_id": "...",
  "task_id": "task_abc123",
  "status": "completed",
  "output_file": "/tmp/subagent_task_abc123.txt",
  "summary": "Finished inspecting 4 files",
  "usage": {
    "total_tokens": 3412,
    "tool_uses": 6,
    "duration_ms": 8200
  }
}
```

##### `system/api_retry` & `system/status`
```json
{
  "type": "system",
  "subtype": "api_retry",
  "uuid": "...",
  "session_id": "...",
  "attempt": 1,
  "max_retries": 3,
  "retry_delay_ms": 2000,
  "error_status": 429,
  "error": "rate_limit"
}
```

#### 2. `assistant` (Completed Model Turn Blocks)
Emitted when model content blocks complete:
```json
{
  "type": "assistant",
  "uuid": "...",
  "session_id": "...",
  "parent_tool_use_id": null,
  "agent_id": null,
  "user_message_uuid": "...",
  "message": {
    "id": "msg_01ABC",
    "type": "message",
    "role": "assistant",
    "model": "claude-sonnet-5",
    "content": [
      {
        "type": "text",
        "text": "I will examine the codebase."
      },
      {
        "type": "thinking",
        "thinking": "Need to check Cargo.toml first...",
        "signature": "..."
      },
      {
        "type": "tool_use",
        "id": "toolu_01_bash_call",
        "name": "Bash",
        "input": {
          "command": "cargo check"
        }
      }
    ],
    "stop_reason": "tool_use",
    "usage": {
      "input_tokens": 1250,
      "output_tokens": 84,
      "cache_read_input_tokens": 800,
      "cache_creation_input_tokens": 0
    }
  }
}
```

*Note on Subagents*: When emitted for a subagent (`--forward-subagent-text`), `parent_tool_use_id` is set to `"toolu_01_agent_spawn"` and `agent_id` is set to `"task_abc123"`.

#### 3. `user` (Tool Results, Subagent Ingestion, Replay Messages)
```json
{
  "type": "user",
  "uuid": "...",
  "session_id": "...",
  "parent_tool_use_id": null,
  "message": {
    "role": "user",
    "content": [
      {
        "type": "tool_result",
        "tool_use_id": "toolu_01_bash_call",
        "content": "Finished dev profile in 0.12s",
        "is_error": false
      }
    ]
  },
  "tool_use_result": { "status": 0 }
}
```

#### 4. `tool_progress` (Real-Time Long-Running Tool Feedback)
```json
{
  "type": "tool_progress",
  "uuid": "...",
  "session_id": "...",
  "tool_use_id": "toolu_01_bash_call",
  "tool_name": "Bash",
  "parent_tool_use_id": null,
  "elapsed_time_seconds": 15,
  "task_id": "task_abc123"
}
```

#### 5. `stream_event` (Token-Level Streaming with `--include-partial-messages`)
```json
{
  "type": "stream_event",
  "uuid": "...",
  "session_id": "...",
  "parent_tool_use_id": null,
  "user_message_uuid": "...",
  "event": {
    "type": "content_block_delta",
    "index": 0,
    "delta": {
      "type": "text_delta",
      "text": "Searching"
    }
  }
}
```
Supported delta types: `text_delta`, `thinking_delta`, `input_json_delta`.

#### 6. `result` (Turn Termination / Completion Summary)
Emitted at the end of each turn:
```json
{
  "type": "result",
  "subtype": "success",
  "uuid": "...",
  "session_id": "...",
  "result": "Task completed successfully.",
  "duration_ms": 12430,
  "duration_api_ms": 10120,
  "is_error": false,
  "num_turns": 4,
  "stop_reason": "end_turn",
  "total_cost_usd": 0.0421,
  "usage": {
    "input_tokens": 5420,
    "output_tokens": 420,
    "cache_read_input_tokens": 3100,
    "cache_creation_input_tokens": 0
  },
  "modelUsage": {
    "claude-sonnet-5": {
      "inputTokens": 5420,
      "outputTokens": 420,
      "costUSD": 0.0421
    }
  },
  "permission_denials": [],
  "terminal_reason": "completed"
}
```
Error subtypes: `"error_max_turns"`, `"error_during_execution"`, `"error_max_budget_usd"`. When `is_error: true`, the object includes an `"errors": ["..."]` array.

#### 7. `control_request` (Approval Delegated to Host)
Emitted when a tool call requires human-in-the-loop authorization under `--permission-prompts=host`:
```json
{
  "type": "control_request",
  "request_id": "req_84f93a12",
  "request": {
    "subtype": "can_use_tool",
    "tool_name": "Bash",
    "input": {
      "command": "git push origin main --force"
    },
    "tool_use_id": "toolu_01_push",
    "agent_id": null,
    "decision_reason": "Command matches destructive pattern",
    "blocked_path": null,
    "default_to_no": true,
    "suggestions": []
  }
}
```

---

### 3.3 Stdin Input Format (`--input-format=stream-json`)

All messages written to child stdin must be formatted as single-line JSON followed by `\n`.

#### 1. User Message (Turn Submission or Continuation)
```json
{
  "type": "user",
  "uuid": "41c888e2-550e-42c2-8412-1a2b3c4d5e6f",
  "session_id": "9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d",
  "parent_tool_use_id": null,
  "message": {
    "role": "user",
    "content": "Run cargo check and fix warnings"
  },
  "priority": "now",
  "origin": { "kind": "human" },
  "shouldQuery": true,
  "client_composed": true
}
```

#### 2. Control Response (Approval Answering)
Matches the incoming `request_id` from a `control_request`:

```json
{
  "type": "control_response",
  "request_id": "req_84f93a12",
  "response": {
    "behavior": "allow",
    "updatedInput": {
      "command": "git push origin main --force"
    }
  }
}
```

Denying a tool call:
```json
{
  "type": "control_response",
  "request_id": "req_84f93a12",
  "response": {
    "behavior": "deny",
    "message": "Operator denied force-push operation.",
    "interrupt": false
  }
}
```

#### 3. Host Interrupt Request
Sent by the host on stdin to abort the current turn:
```json
{
  "type": "control_request",
  "request_id": "int_req_01",
  "request": {
    "subtype": "interrupt",
    "cancel_queued": true
  }
}
```

---

## 4. Protocol Translation: Claude Code -> `RuntimeEvent`

### 4.1 Authoritative Event Mapping Matrix

The adapter normalizes raw Claude Code NDJSON wire frames into Agent Studios `agent_studios_external_runtime::RuntimeEventKind`.

| Claude Code Event (`type` / `subtype`) | Condition / Payload Extraction | `RuntimeEventKind` Target | Translation Notes |
|---|---|---|---|
| `system/init` | Startup event | `SessionStarted { session_id, instance_id }` | Confirms child process initialized and ready. |
| `stream_event` (`content_block_delta`) | `delta.type == "text_delta"` | `OutputDelta { text }` | Token-level streaming chunk. |
| `assistant` | `content[].type == "text"` (when partial streaming disabled) | `OutputDelta { text }` | Completed text block. |
| `assistant` | `content[].type == "tool_use"` | `ToolStarted { tool_name, tool_call_id }` | Extracted from tool_use block. |
| `user` | `content[].type == "tool_result"` | `ToolCompleted { tool_name, tool_call_id, duration_ms, success }` | Inferred from tool_result block and `is_error`. |
| `control_request` (`can_use_tool`) | Permission prompt | `ApprovalRequested { approval_id, description, command }` | `approval_id` derived deterministically from `request_id`. |
| `system/task_started` | `task_type == "subagent"` | `Diagnostic` + Subagent Tree Entry | Registers subagent node in correlation tracker. |
| `system/task_notification` | `status == "completed"` | `ToolCompleted` (for `Agent` tool) | Marks spawning `Agent` tool call completed. |
| `result` | `is_error == false` | `Diagnostic` (Turn Completed) | Turn completion does not terminate interactive session! |
| `result` | `is_error == true` | `Failed { safe_error_summary }` | Fatal error across turns. |
| `result.usage` / `modelUsage` | Usage blocks | `UsageUpdated { prompt_tokens, completion_tokens, total_tokens }` | Token accounting. |
| `control_response` (interrupt ack) | Interrupt acknowledged | `Interrupted { reason }` | Active turn aborted cleanly. |
| Process termination / Stop | Child process exit | `Stopped` or `Completed` | Terminal lifecycle state. |
| `system/api_retry`, `hook_*` | Background notifications | `Diagnostic { level, message }` | Non-fatal diagnostic logging. |

### 4.2 Lifecycle State Mapping (Turn vs Session)
A critical invariant established in M11 and preserved in M12: **A single turn completing (`result`) is NOT a terminal session completion.**
In interactive multi-turn workflows, an Agent Studios session remains active across multiple user prompts until explicitly stopped or terminated:

```
[Start Session]
      │
      ▼
┌──────────────┐
│   Starting   │ (Child process spawned, waiting for system/init)
└──────┬───────┘
      │
      ▼
┌──────────────┐◄───────────────┐
│   Running    │                │ (Next Turn / Continuation)
└──────┬───────┘                │
      │   ▲                     │
      │   │ Turn done           │
      │   │ (result event)      │
      ▼   │                     │
┌──────────────┐                │
│  Awaiting    ├────────────────┘
│    Input     │
└──────┬───────┘
       │
       ├───► [Approval Requested] ──► BlockedOnApproval ──► [Response Sent] ──► Running
       ├───► [Interrupt Sent]     ──► Interrupted       ──► [Resume Turn]   ──► Running
       └───► [Stop / Fatal Exit]  ──► Stopped / Failed / Completed (Terminal)
```

---

## 5. Subagent Event Architecture & Correlation Without Synthetic IDs

### 5.1 The Subagent Challenge in Multi-Agent Runtimes
In multi-agent architectures, agents frequently spawn child subagents (e.g. `claude-code-guide`, `Explore`, `Plan`, or custom subagent definitions). Many external adapters suffer from two common defects:
1. **Synthetic ID Hallucination**: Inventing random, synthetic session or task IDs that lose alignment with the native agent transcript.
2. **Context Orphanage**: Flattening subagent output into the parent output stream, stripping parent-child hierarchy and making UI tree visualization impossible.

### 5.2 Native Claude Code Subagent Hierarchy
Claude Code solves this cleanly at the protocol level:
1. When a parent agent invokes a subagent, it emits a standard `tool_use` block where `name` is `"Agent"` or a registered subagent name (e.g. `"claude-code-guide"`), generating a native `id: "toolu_01ABC..."`.
2. The CLI emits a `system/task_started` event:
   ```json
   { "task_id": "task_sub_123", "tool_use_id": "toolu_01ABC...", "task_type": "subagent", "spawn_depth": 1 }
   ```
3. With `--forward-subagent-text`, all child messages carry:
   - `parent_tool_use_id: "toolu_01ABC..."` (links directly to the parent's `Agent` tool use).
   - `agent_id: "task_sub_123"` (identifies the specific subagent instance).
4. Nested subagents (subagents of subagents) continue this pointer chain: child subagents set `parent_tool_use_id` to their respective spawning tool call, forming an unambiguous directed acyclic graph (DAG).

```
Main Session (parent_tool_use_id = null)
   │
   ├─► ToolUse: Agent ("Explore") [tool_use_id = toolu_explore_01]
   │      │
   │      ├─► Subagent Task Started [task_id = task_exp_01, tool_use_id = toolu_explore_01]
   │      ├─► Assistant (text/thinking) [parent_tool_use_id = toolu_explore_01]
   │      ├─► ToolUse: Grep [tool_use_id = toolu_grep_02, parent_tool_use_id = toolu_explore_01]
   │      ├─► ToolResult: Grep [parent_tool_use_id = toolu_explore_01]
   │      └─► Task Notification: Completed [task_id = task_exp_01]
   │
   └─► ToolCompleted: Agent [tool_use_id = toolu_explore_01]
```

### 5.3 Correlation Tracker Specification (`SubagentCorrelationTracker`)
The adapter maintains an in-memory, thread-safe correlation tracker:

```rust
pub struct SubagentNode {
    pub task_id: String,
    pub parent_tool_use_id: String,
    pub description: String,
    pub spawn_depth: u32,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub success: bool,
}

pub struct SubagentCorrelationTracker {
    /// Maps tool_use_id -> SubagentNode
    nodes_by_tool_call: std::collections::BTreeMap<String, SubagentNode>,
    /// Maps task_id -> tool_use_id
    tool_calls_by_task_id: std::collections::BTreeMap<String, String>,
}
```

#### Mapping Invariant:
- When translating `stream_event` or `assistant` content, the adapter inspects `parent_tool_use_id`.
- If `parent_tool_use_id` is `None`: Content belongs to root agent.
- If `parent_tool_use_id` is `Some(parent_id)`: Content belongs to the registered subagent node. The adapter tags downstream diagnostics and control plane events with the verified `parent_id` and `agent_id` directly from wire telemetry. Zero synthetic UUIDs are generated.

---

## 6. Interactive Session Management, Approvals, & Interruption

### 6.1 Approval Delegation Architecture (`ApprovalAuthority`)
Under `--permission-prompts=host`, the CLI delegates all tool permission decisions to the host.

#### Translation Sequence:
1. Child CLI reaches tool boundary (e.g. `Bash` command `cargo check`).
2. Child CLI writes `control_request` to stdout:
   ```json
   { "type": "control_request", "request_id": "req_84f93a12", "request": { "subtype": "can_use_tool", "tool_name": "Bash", ... } }
   ```
3. Adapter intercepts `control_request`:
   - Extracts `request_id` (`req_84f93a12`).
   - Derives deterministic `ApprovalId`:
     ```rust
     let approval_id = ApprovalId::from(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, request_id.as_bytes()));
     ```
   - Stores mapping `ApprovalId -> (request_id, tool_name, tool_input)` in pending approval registry.
   - Emits normalized `RuntimeEventKind::ApprovalRequested` to `SessionEventHub`.
4. Operator / UI responds via `AgentRuntime::send(session, RuntimeInput::ApprovalResponse { approval_id, approved, reason })`.
5. Adapter looks up pending `request_id` and writes `control_response` to child stdin:
   ```json
   {
     "type": "control_response",
     "request_id": "req_84f93a12",
     "response": {
       "behavior": if approved { "allow" } else { "deny" },
       "message": reason
     }
   }
   ```
6. **Fail-Closed Safety Guarantee**: If the session is interrupted, stopped, or if stdin closes while an approval is pending, the adapter immediately drops or denies the request. Execution never proceeds unapproved.

### 6.2 Interruption Protocol
Claude Code supports clean mid-turn cancellation:
1. Operator invokes `AgentRuntime::interrupt(session)`.
2. Adapter writes `control_request` interrupt frame to child stdin:
   ```json
   { "type": "control_request", "request_id": "int_req_01", "request": { "subtype": "interrupt", "cancel_queued": true } }
   ```
3. Child halts current LLM query / tool loop and acknowledges on stdout:
   ```json
   { "type": "control_response", "request_id": "int_req_01", "response": { "cancelled": [...] } }
   ```
4. Adapter emits `RuntimeEventKind::Interrupted` and transitions lifecycle state to `RuntimeLifecycleState::Interrupted`. Context and session remain alive for subsequent turns.

### 6.3 Idempotent Stop Protocol
When terminating a session permanently via `AgentRuntime::stop(session)`:
1. Close child stdin pipe (signals EOF).
2. Send termination signal (or close Win32 Job Object).
3. Allow up to 3000ms grace period for child process to drain stdout and exit.
4. If process fails to exit within grace period, trigger Win32 Job Object tree kill (`TerminateJobObject`).
5. Emit `RuntimeEventKind::Stopped` and transition state to `RuntimeLifecycleState::Stopped`.
6. Subsequent calls to `stop()` return `Ok(())` idempotently without error.

---

## 7. Windows OS Process Sandboxing & Containment Architecture

### 7.1 The Windows Child Process Isolation Hazard
On Windows, standard `std::process::Command` launches processes that can spawn detached grandchildren or ignore SIGINT/SIGTERM, leaving orphaned Bun, Node, and Git processes consuming CPU/RAM.

### 7.2 Zero-Window Win32 Job Object Implementation
The adapter leverages the proven containment architecture from M11 (`agent-studios-opencode-runtime::sandbox`):

```
[Start Session Request]
       │
       ▼
1. Create Win32 Job Object with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
       │
       ▼
2. Spawn child process with CREATE_SUSPENDED (via CommandExt)
       │
       ▼
3. AssignProcessToJobObject(job_handle, child_process_handle)
       │
       ▼
4. Verify IsProcessInJob(child_process_handle, job_handle) == TRUE
       │
       ▼
5. NtResumeProcess(child_process_handle)
       │
       ▼
[Child Process Runs - Contained in Job]
```

### 7.3 Truthful Capability Reporting
- Win32 Job Objects provide robust process accounting, active process limits, and guaranteed tree-kill on close.
- Win32 Job Objects do **not** provide Linux-style filesystem or network namespace isolation.
- To prevent false security assumptions, the adapter:
  - Supports `SandboxLevel::Standard` (Job Object process tree containment).
  - Explicitly fails closed with a typed error if `SandboxLevel::Strict` is requested on Windows without Hyper-V container backing.

---

## 8. Crate Structure & Architecture for `agent-studios-claude-code-runtime`

### 8.1 Workspace Placement & Cargo Manifest
The crate is placed at `agent-studios-rs/crates/claude-code-runtime` and registered in `agent-studios-rs/Cargo.toml` workspace members:

```toml
[package]
name = "agent-studios-claude-code-runtime"
version = "0.1.0"
edition = "2024"
license = "Apache-2.0"
authors = ["Agent Studios Contributors"]

[dependencies]
agent-studios-protocol = { path = "../protocol" }
agent-studios-external-runtime = { path = "../external-runtime" }
agent-studios-provider = { path = "../provider" }

async-trait = "0.1"
tokio = { version = "1.43", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1.10", features = ["v4", "v5", "serde"] }
thiserror = "2.0"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
bytes = "1.10"

[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.59", features = [
    "Win32_Foundation",
    "Win32_System_JobObjects",
    "Win32_System_Threading",
    "Win32_Security",
] }

[dev-dependencies]
tempfile = "3.17"
```

### 8.2 Internal Module Layout

```
crates/claude-code-runtime/src/
├── lib.rs              # Crate root, public exports
├── error.rs            # ClaudeCodeError enum, conversions to RuntimeError
├── discovery.rs        # CLI discovery, binary verification, capabilities declaration
├── protocol.rs         # NDJSON wire structs (System, Assistant, User, ControlRequest, Result)
├── stream.rs           # Bounded NDJSON stream reader, line splitter, buffer protection
├── mapping.rs          # Event translation (Wire types -> RuntimeEventKind)
├── subagent.rs         # SubagentCorrelationTracker, DAG correlation
├── approval.rs         # ApprovalAuthority (ApprovalId <-> request_id, control_response pump)
├── process.rs          # Child process supervisor, stdio pump, exit detection
├── sandbox.rs          # Win32 Job Object containment (inheriting M11 pattern)
└── runtime.rs          # AgentRuntime implementation (start, send, interrupt, resume, stop)
```

---

## 9. Design Contract for Agent B and Agent C

### 9.1 Responsibilities Breakdown

| Role | Assigned Subagent | Primary Files & Responsibilities | Key Quality Targets |
|---|---|---|---|
| **Protocol & Core Runtime** | **Agent B** (`devcore`) | `lib.rs`, `runtime.rs`, `process.rs`, `protocol.rs`, `stream.rs`, `mapping.rs`, `subagent.rs`, `discovery.rs`, `error.rs` | Clean compilation (2024 edition), complete `AgentRuntime` lifecycle, unit & mock tests. |
| **Security, Approvals & Sandboxing** | **Agent C** (`devsec`) | `sandbox.rs`, `approval.rs`, budget checks (`--max-budget-usd`), permission mode enforcement, credential scrubbing, Win32 Job Object tests. | Fail-closed approvals, zero credential leakage, reliable process tree-kill, security regression tests. |

### 9.2 Interface Contract for Agent B (Implementation)

1. **`ClaudeCodeRuntime` Struct**:
   ```rust
   pub struct ClaudeCodeRuntime {
       implementation_id: RuntimeImplementationId,
       discovery_config: ClaudeCodeDiscoveryConfig,
       active_sessions: Arc<RwLock<BTreeMap<RuntimeSessionId, ClaudeCodeActiveSession>>>,
   }
   ```
2. **`AgentRuntime` Trait Implementation**:
   - `implementation_id(&self)`: Returns `"claude-code"`.
   - `capabilities(&self)`: Advertises `StreamingEvents`, `Interrupt`, `Resume`, `Stop`, `PersistentSession`, `Tools`, `Approvals`, `Subagents`, `UsageReporting`.
   - `start(&self, req)`: Spawns child process in Job Object, opens stream-json stdio pipes, registers session in `SessionEventHub`.
   - `send(&self, session, input)`: Serializes `RuntimeInput` to stdin NDJSON frame.
   - `interrupt(&self, session)`: Writes `control_request { subtype: "interrupt" }` to stdin.
   - `resume(&self, session, input)`: Submits prompt to continue turn.
   - `stop(&self, session)`: Idempotently kills child process tree and marks session `Stopped`.
3. **NDJSON Stream Ingestion Loop**:
   - Must use bounded line parser (`MAX_LINE_LENGTH = 16 * 1024 * 1024`, `MAX_BUFFER = 64 * 1024 * 1024` to support large tool outputs).
   - Ingestion loop must yield on `\n` and parse line into `WireMessage`.
   - Forward translated events into `SessionEventHub`.

### 9.3 Interface Contract for Agent C (Security, Permissions, Budget)

1. **`ApprovalAuthority` Interface**:
   ```rust
   pub struct ApprovalAuthority { ... }
   impl ApprovalAuthority {
       pub fn register_request(&self, request: &ControlRequest) -> ApprovalRequestedEvent;
       pub fn resolve_approval(&self, id: ApprovalId, approved: bool, reason: Option<String>) -> Result<ControlResponse, ClaudeCodeError>;
       pub fn cancel_all_pending(&self);
   }
   ```
2. **Win32 Job Object Invariants**:
   - Must enforce `CREATE_SUSPENDED` -> `AssignProcessToJobObject` -> `NtResumeProcess`.
   - `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` must be set unconditionally.
3. **Budget Limit Binding**:
   - If `RuntimeStartRequest` specifies a spending limit or budget metadata, bind `--max-budget-usd <amount>`.
   - Translate exit code 2 with budget message to `RuntimeError::BudgetExceeded` or safe terminal `Failed` event.
4. **Diagnostic Sanitization**:
   - All error summaries and diagnostic logs must pass through `agent_studios_external_runtime::sanitize_error_message` to scrub URLs, tokens, and API keys.

---

## 10. Summary Verification Checklist for M12

- [x] **CLI Capability Research**: Verified local CLI `2.1.310`, full flag roster audited.
- [x] **NDJSON Stream Wire Protocol**: Complete JSON schemas for stdout events and stdin input commands documented.
- [x] **Subagent Event Correlation**: Explicit correlation architecture via `parent_tool_use_id` specified without synthetic IDs.
- [x] **Event Translation Mapping**: Complete bidirectional mapping table to `RuntimeEventKind` and `RuntimeLifecycleState` established.
- [x] **Interactive Protocol**: Host-delegated approvals (`ApprovalRequested`) and mid-turn interruption specified.
- [x] **Sandboxing & Containment**: Win32 Job Object zero-window containment architecture aligned with M11 precedent.
- [x] **Crate & Module Layout**: Full layout for `agent-studios-claude-code-runtime` specified.
- [x] **Contract Handoff**: Clear interface contracts defined for Agent B and Agent C.
