# Agent Studios M13 IPC Protocol Golden Fixtures

**Date**: 2026-10-30  
**Target Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Implementation Source**: `crates/ipc-transport/tests/golden_fixture_tests.rs`  
**Status**: CANONICAL & VERIFIED  

---

## 1. Framing Specification & Wire Format

All IPC commands and responses are transmitted across native operating system streams (Named Pipes on Windows, Domain Sockets on Unix) framed with a **4-byte big-endian unsigned integer length prefix**:

```
+------------------------------------+---------------------------------------+
|  Frame Length: u32 (4 bytes, BE)  |  Frame Payload: UTF-8 JSON (N bytes)  |
+------------------------------------+---------------------------------------+
```

- **Maximum Frame Size**: `DEFAULT_MAX_FRAME_SIZE = 4 * 1024 * 1024` (4 MiB = 4,194,304 bytes).
- Frames declaring length prefix `> 4,194,304` are rejected immediately prior to buffer allocation to prevent memory exhaustion DoS.
- Endianness: Network standard big-endian (`u32::to_be_bytes()`).

---

## 2. Authentication & Handshake Wire Vectors

### 2.1 Capability Negotiation

#### Command: `negotiate_capabilities`
```json
{
  "type": "negotiate_capabilities",
  "protocol_version": {
    "major": 1,
    "minor": 0,
    "patch": 0
  },
  "capabilities": [
    "event_streaming",
    "linearizable_replay"
  ]
}
```

#### Response: `capabilities_negotiated`
```json
{
  "type": "capabilities_negotiated",
  "selected_version": {
    "major": 1,
    "minor": 0,
    "patch": 0
  },
  "agreed_capabilities": [
    "event_streaming",
    "linearizable_replay"
  ]
}
```

---

### 2.2 HMAC-SHA256 Authentication

#### Canonical Digest Format
```
{protocol_version}:{key_id}:{session_id}:{nonce}:{timestamp_millis}:{capabilities_str}:{payload_hash}
```
- `capabilities_str`: Lexicographically sorted, comma-separated capability list.
- `payload_hash`: Hexadecimal SHA-256 digest of payload bytes (`hex::encode(Sha256::digest(payload))`).
- Empty payload hash: `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.

#### Canonical Digest Example
Given:
- `protocol_version`: `1.0.0`
- `key_id`: `test-client-1`
- `session_id`: `sess-9a7c0000-0000-0000-0000-000000000001`
- `nonce`: `nonce-abcd-1234`
- `timestamp_millis`: `1793400000000`
- `capabilities`: `["event_streaming", "linearizable_replay"]`
- `payload`: `""` (empty)

Canonical Digest:
```text
1.0.0:test-client-1:sess-9a7c0000-0000-0000-0000-000000000001:nonce-abcd-1234:1793400000000:event_streaming,linearizable_replay:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
```

#### Command: `authenticate`
```json
{
  "type": "authenticate",
  "header": {
    "protocol_version": {
      "major": 1,
      "minor": 0,
      "patch": 0
    },
    "key_id": "test-client-1",
    "session_id": "sess-9a7c0000-0000-0000-0000-000000000001",
    "nonce": "nonce-abcd-1234",
    "timestamp_millis": 1793400000000,
    "signature": "8f3b25a1b0c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e"
  },
  "payload": null
}
```

#### Response: `authenticated`
```json
{
  "type": "authenticated",
  "session_id": "sess-9a7c0000-0000-0000-0000-000000000001"
}
```

---

## 3. Ping / Pong & Diagnostics

#### Command: `ping`
```json
{
  "type": "ping",
  "payload": "heartbeat",
  "timestamp": 1793400000000
}
```

#### Response: `pong`
```json
{
  "type": "pong",
  "payload": "heartbeat",
  "client_timestamp": 1793400000000,
  "server_timestamp": 1793400000005
}
```

#### Command: `get_health`
```json
{
  "type": "get_health"
}
```

#### Response: `health`
```json
{
  "type": "health",
  "status": "healthy",
  "active_clients": 2,
  "uptime_secs": 7200,
  "memory_events_count": 150
}
```

---

## 4. Control Plane Commands & Responses

### 4.1 Studio Lifecycle

#### Command: `create_studio`
```json
{
  "type": "create_studio",
  "name": "Studio Epsilon"
}
```

#### Response: `studio_created`
```json
{
  "type": "studio_created",
  "studio": {
    "id": "c0a80101-0000-0000-0000-000000000001",
    "name": "Studio Epsilon",
    "created_at": "2026-10-30T00:00:00Z"
  }
}
```

#### Command: `list_studios`
```json
{
  "type": "list_studios"
}
```

#### Response: `studios`
```json
{
  "type": "studios",
  "studios": [
    {
      "id": "c0a80101-0000-0000-0000-000000000001",
      "name": "Studio Epsilon",
      "created_at": "2026-10-30T00:00:00Z"
    }
  ]
}
```

#### Command: `get_studio`
```json
{
  "type": "get_studio",
  "studio_id": "c0a80101-0000-0000-0000-000000000001"
}
```

#### Response: `studio`
```json
{
  "type": "studio",
  "studio": {
    "id": "c0a80101-0000-0000-0000-000000000001",
    "name": "Studio Epsilon",
    "created_at": "2026-10-30T00:00:00Z"
  }
}
```

---

### 4.2 Task Management & Lifecycle

#### Command: `submit_task`
```json
{
  "type": "submit_task",
  "studio_id": "c0a80101-0000-0000-0000-000000000001",
  "task_spec": {
    "key": "task-test-spec",
    "title": "Test Task Spec",
    "description": "Verify golden fixture serialization",
    "assigned_agent_id": null,
    "parent_task_key": null,
    "parent_task_id": null,
    "dependency_keys": [],
    "dependency_task_ids": [],
    "dependency_output_policy": "task_success"
  }
}
```

#### Response: `tasks_submitted`
```json
{
  "type": "tasks_submitted",
  "task_ids": [
    "d0a80101-0000-0000-0000-000000000001"
  ]
}
```

#### Command: `cancel_task`
```json
{
  "type": "cancel_task",
  "studio_id": "c0a80101-0000-0000-0000-000000000001",
  "task_id": "d0a80101-0000-0000-0000-000000000001",
  "reason": "Manual abort by operator"
}
```

#### Response: `task_cancelled`
```json
{
  "type": "task_cancelled",
  "summary": {
    "cancelled_tasks": [
      "d0a80101-0000-0000-0000-000000000001"
    ],
    "cancelled_runs": [
      "e0a80101-0000-0000-0000-000000000001"
    ],
    "cancelled_approvals": []
  }
}
```

#### Command: `list_tasks`
```json
{
  "type": "list_tasks",
  "studio_id": "c0a80101-0000-0000-0000-000000000001"
}
```

#### Response: `tasks`
```json
{
  "type": "tasks",
  "tasks": [
    {
      "id": "d0a80101-0000-0000-0000-000000000001",
      "studio_id": "c0a80101-0000-0000-0000-000000000001",
      "parent_task_id": null,
      "assigned_agent_id": null,
      "title": "Test Task Spec",
      "description": "Verify golden fixture serialization",
      "state": "ready",
      "dependencies": [],
      "created_at": "2026-10-30T00:00:00Z",
      "updated_at": "2026-10-30T00:00:00Z",
      "retry_count": 0,
      "dependency_output_policy": "task_success"
    }
  ]
}
```

#### Command: `get_task`
```json
{
  "type": "get_task",
  "studio_id": "c0a80101-0000-0000-0000-000000000001",
  "task_id": "d0a80101-0000-0000-0000-000000000001"
}
```

#### Response: `task`
```json
{
  "type": "task",
  "task": {
    "id": "d0a80101-0000-0000-0000-000000000001",
    "studio_id": "c0a80101-0000-0000-0000-000000000001",
    "parent_task_id": null,
    "assigned_agent_id": null,
    "title": "Test Task Spec",
    "description": "Verify golden fixture serialization",
    "state": "ready",
    "dependencies": [],
    "created_at": "2026-10-30T00:00:00Z",
    "updated_at": "2026-10-30T00:00:00Z",
    "retry_count": 0,
    "dependency_output_policy": "task_success"
  }
}
```

---

### 4.3 Run Management

#### Command: `list_runs`
```json
{
  "type": "list_runs",
  "studio_id": "c0a80101-0000-0000-0000-000000000001"
}
```

#### Response: `runs`
```json
{
  "type": "runs",
  "runs": [
    {
      "id": "e0a80101-0000-0000-0000-000000000001",
      "task_id": "d0a80101-0000-0000-0000-000000000001",
      "agent_id": "a0a80101-0000-0000-0000-000000000001",
      "attempt": 1,
      "state": "queued",
      "started_at": null,
      "finished_at": null
    }
  ]
}
```

#### Command: `get_run`
```json
{
  "type": "get_run",
  "studio_id": "c0a80101-0000-0000-0000-000000000001",
  "run_id": "e0a80101-0000-0000-0000-000000000001"
}
```

#### Response: `run`
```json
{
  "type": "run",
  "run": {
    "id": "e0a80101-0000-0000-0000-000000000001",
    "task_id": "d0a80101-0000-0000-0000-000000000001",
    "agent_id": "a0a80101-0000-0000-0000-000000000001",
    "attempt": 1,
    "state": "queued",
    "started_at": null,
    "finished_at": null
  }
}
```

---

### 4.4 Provider & Model Catalog

#### Command: `list_providers`
```json
{
  "type": "list_providers"
}
```

#### Response: `providers`
```json
{
  "type": "providers",
  "providers": [
    {
      "instance_id": "b0a80101-0000-0000-0000-000000000001",
      "provider_id": "openai",
      "display_name": "OpenAI Instance",
      "protocol_family": "open_ai_responses",
      "endpoint_profile": {
        "base_url": "https://api.openai.com/v1"
      },
      "auth_scheme": "none"
    }
  ]
}
```

#### Command: `get_provider`
```json
{
  "type": "get_provider",
  "provider_id": "b0a80101-0000-0000-0000-000000000001"
}
```

#### Response: `provider`
```json
{
  "type": "provider",
  "provider": {
    "instance_id": "b0a80101-0000-0000-0000-000000000001",
    "provider_id": "openai",
    "display_name": "OpenAI Instance",
    "protocol_family": "open_ai_responses",
    "endpoint_profile": {
      "base_url": "https://api.openai.com/v1"
    },
    "auth_scheme": "none"
  }
}
```

#### Command: `list_models`
```json
{
  "type": "list_models",
  "instance_id": "b0a80101-0000-0000-0000-000000000001"
}
```

#### Response: `models`
```json
{
  "type": "models",
  "models": [
    {
      "provider_instance_id": "b0a80101-0000-0000-0000-000000000001",
      "model_id": "gpt-4o-mini",
      "display_name": "GPT-4o Mini",
      "capabilities": {
        "streaming": true,
        "function_calling": true,
        "vision": false
      },
      "limits": {
        "max_context_tokens": 128000,
        "max_output_tokens": 4096
      }
    }
  ]
}
```

---

### 4.5 Approvals & Decisions

#### Command: `resolve_approval` (Approved)
```json
{
  "type": "resolve_approval",
  "approval_id": "018f3a5e-0000-7000-8000-000000000001",
  "decision": {
    "decision": "approved",
    "modified_details": null,
    "user_note": "Operator confirmed action"
  }
}
```

#### Command: `resolve_approval` (Rejected)
```json
{
  "type": "resolve_approval",
  "approval_id": "018f3a5e-0000-7000-8000-000000000001",
  "decision": {
    "decision": "rejected",
    "reason": "Access denied by security policy"
  }
}
```

#### Response: `approval_resolved`
```json
{
  "type": "approval_resolved",
  "approval_id": "018f3a5e-0000-7000-8000-000000000001",
  "status": "authorized"
}
```

---

### 4.6 Events & Streaming

#### Command: `subscribe_events`
```json
{
  "type": "subscribe_events",
  "from_sequence": 42
}
```

#### Response: `event`
```json
{
  "type": "event",
  "event": {
    "sequence": 42,
    "timestamp": "2026-10-30T00:00:00Z",
    "event_type": "task_state_transitioned",
    "payload": {
      "task_id": "c0a80101-0000-0000-0000-000000000001",
      "from": "pending",
      "to": "ready"
    }
  }
}
```

---

### 4.7 Errors

#### Response: `error`
```json
{
  "type": "error",
  "message": "Permission denied: cross-session cancellation requires admin role"
}
```

---

## 5. Bootstrap Credential Discovery Fixtures

### 5.1 Standard Client Bootstrap (`agsd.json`)
```json
{
  "endpoint": "\\\\.\\pipe\\agent-studios-default",
  "key_id": "ags-client",
  "client_key": "f81d4fae-7dec-11d0-a765-00a0c91e6bf6",
  "pid": 12345,
  "created_at": 1793400000
}
```

### 5.2 Administrative Bootstrap (`agsd-admin.json`)
```json
{
  "endpoint": "\\\\.\\pipe\\agent-studios-default",
  "key_id": "ags-client-admin",
  "admin_key": "a81d4fae-7dec-11d0-a765-00a0c91e6bf6",
  "pid": 12345,
  "created_at": 1793400000
}
```

---

## 6. Rust Test Suite Reference

All fixtures defined in this document are automatically tested for serialization roundtrip, tagging consistency, and schema preservation in:
```
crates/ipc-transport/tests/golden_fixture_tests.rs
```
Verified passing via `cargo test -p agent-studios-ipc-transport --test golden_fixture_tests`.
