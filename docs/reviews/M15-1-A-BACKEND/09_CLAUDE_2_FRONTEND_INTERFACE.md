# Claude 2 (Frontend) Integration Contract & Interface Specification
**Milestone:** M15.1-A — Backend and CLI Consolidation  
**Target Audience:** Claude 2 (Frontend & VS Code Extension Implementation Lead)  
**Author:** Claude 1 (Principal Rust Architect & Backend Lead)  
**Date:** 2026-11-06  

---

## 1. Scope & Ownership Boundaries

To guarantee zero merge conflicts and pristine separation of responsibilities:
- **Claude 1 Owns:**
  - Rust workspace (`agent-studios-rs/`)
  - Control plane, provider catalog, discovery engine, runtime bridges, IPC daemon (`agsd`)
  - Official CLI (`ags`) and TUI (`crates/tui`)
  - Backend/IPC contract tests and release gate reports
- **Claude 2 Owns:**
  - VS Code extension (`apps/agent-studios-vscode/`)
  - Webview panels (Agent Cockpit, Provider Manager, Task DAG Visualizer)
  - Mission Control web frontend
  - Frontend contract test suites and VS Code extension packaging

Under no circumstances will Claude 1 modify files within `apps/agent-studios-vscode/` or frontend webviews. All requirements are communicated through this specification and the accompanying `backend_frontend_contract_manifest.json`.

---

## 2. IPC Connection & Transport Specification

The VS Code extension communicates with the Agent Studios backend either via direct CLI invocation (for static configuration/diagnostics) or via the native IPC daemon (`agsd`) for real-time streaming.

### 2.1 Native IPC Endpoints
- **Windows:** Named Pipe `\\.\pipe\agent-studios-default`
- **macOS / Linux:** Unix Domain Socket `/tmp/agent-studios.sock` (or `$XDG_RUNTIME_DIR/agent-studios.sock`)

### 2.2 Wire Framing Protocol
Each frame consists of:
1. **Header (4 bytes):** Big-endian `u32` representing payload length in bytes (max 16 MB).
2. **Body:** UTF-8 encoded JSON string matching `IpcCommand` or `IpcResponse`.

### 2.3 Authentication Handshake
Clients must initiate the connection with a signed handshake:
```json
{
  "client_id": "vscode-extension",
  "protocol_version": 1,
  "key_id": "default",
  "capabilities": ["subscribe_events", "manage_approvals", "submit_goals"],
  "signature": "<HMAC-SHA256 signature or development bypass>"
}
```

---

## 3. Core IPC Commands & Responses

### 3.1 Health & Diagnostics
#### Command:
```json
{
  "type": "health"
}
```
#### Response:
```json
{
  "type": "health_response",
  "status": "ready",
  "active_clients": 1,
  "uptime_secs": 1240,
  "memory_events_count": 89
}
```

### 3.2 Subscribing to Real-Time Event Stream
#### Command:
```json
{
  "type": "subscribe",
  "since_sequence": 0
}
```
#### Streamed Event Payload (`EventEnvelope`):
```json
{
  "type": "event",
  "event": {
    "event_id": "ev_01JBCDEFGHJKMNPQRSTVWXYZ12",
    "schema_version": 1,
    "studio_id": "st_01JBCDEFGHJKMNPQRSTVWXYZ00",
    "sequence": 42,
    "timestamp": "2026-11-06T00:15:30.123456Z",
    "event": {
      "kind": "assistant_message_chunk",
      "data": {
        "agent_id": "ag_01JBCDEFGHJKMNPQRSTVWXYZ01",
        "task_id": "tk_01JBCDEFGHJKMNPQRSTVWXYZ02",
        "run_id": "rn_01JBCDEFGHJKMNPQRSTVWXYZ03",
        "content": "Running cargo check on workspace...",
        "is_thought": false
      }
    }
  }
}
```

### 3.3 Goal Submission (`SubmitGoal`)
#### Command:
```json
{
  "type": "submit_goal",
  "studio_id": "st_01JBCDEFGHJKMNPQRSTVWXYZ00",
  "title": "Fix Clippy warnings",
  "description": "Resolve all dead code and unused imports",
  "options": {
    "provider": "anthropic",
    "model": "claude-3-5-sonnet-20241022",
    "workspace_access_mode": "IsolatedWorktree"
  }
}
```

### 3.4 Approval Resolution (`ResolveApproval`)
#### Command:
```json
{
  "type": "resolve_approval",
  "approval_id": "ap_01JBCDEFGHJKMNPQRSTVWXYZ99",
  "decision": "approved",
  "reason": "Operator confirmed file modification"
}
```

---

## 4. CLI Direct Invocation Interface (Offline / Non-Daemon Mode)

For environments where `agsd` is not yet running as a background service, the VS Code extension can execute the bundled `ags` binary with `--json` flags:

### 4.1 System Doctor
```bash
ags doctor --json
```
Returns a `DoctorReport` object detailing Git status, worktree root, platform checks, and daemon reachability.

### 4.2 Provider & Model Queries
```bash
ags provider list --format json
ags model list --format json
```
Returns structured JSON arrays of configured provider instances and available models.

---

## 5. TypeScript Typings Reference

```typescript
export interface DoctorReport {
  cli_version: string;
  os: string;
  arch: string;
  git_available: boolean;
  in_git_repo: boolean;
  repo_root?: string;
  is_dirty?: boolean;
  terminal_dimensions?: [number, number];
  checks: DiagnosticCheck[];
}

export interface DiagnosticCheck {
  name: string;
  status: 'OK' | 'WARN' | 'FAIL' | 'INFO';
  details: string;
  is_critical: boolean;
}

export interface AssistantMessageChunk {
  agent_id: string;
  task_id?: string;
  run_id?: string;
  content: string;
  is_thought: boolean;
}
```
