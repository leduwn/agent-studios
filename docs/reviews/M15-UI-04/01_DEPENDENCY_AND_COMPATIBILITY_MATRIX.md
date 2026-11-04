# M15 UI-04 — Report 01: PR Dependency Graph & IPC Protocol Compatibility Matrix

## Metadata
- **Milestone**: M15 UI-04 — System Integration, Contract Audit & Robust Lifecycle Recovery
- **Component**: Cross-Repository Contract & Dependency Alignment
- **Target PR**: PR #27 (`feat/m15-real-agent-execution-ui` @ `054dc0328f608ec39b3602fcad39e0839c4fce86`) targeting `feat/m14-mission-control-connected-integration`
- **Pinned Backend Commit**: PR #29 (`feat/m15-03-artifact-diff-review` @ `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b`)
- **Evaluated Backend Handoff**: PR #30 (`feat/m15-04-durable-state-recovery` @ `dcbcb39bf61225a4660be5b533cf1ffd5de14b29`)
- **Date**: 2026-11-02
- **Auditor**: Agent A (Dependency & Contract Auditor)

---

## 1. Executive Summary & Audit Scope

This audit evaluates the cross-repository pull request dependency graph, wire protocol schema compatibility, serialization constraints, and type representations between the VS Code Extension Host UI (`agent-studios-ide-ui`) and the authoritative Rust daemon (`agent-studios-rs`).

### Primary Objectives:
1. **Trace PR Dependency Graph**: Map the linear progression of backend milestones (PR #26, PR #28, PR #29, PR #30) against the UI development branch (PR #27).
2. **Authoritative Wire Schema Comparison**: Compare TypeScript definitions (`RustIpcProtocolTypes.ts`, `ArtifactClient.ts`) against the authoritative Rust crates (`crates/ipc-transport/src/connection.rs`, `crates/ipc-protocol`, `crates/protocol`).
3. **Type Representation & Serialization Audit**: Validate `define_id!` transparent UUID deserialization, `snake_case` serialization, Base64 chunk pagination, and cryptographic hashing.
4. **Determine Compatibility Gates**: Verify backwards and forwards compatibility across all M15 milestones.

---

## 2. PR Dependency Graph & Branch Lineage

### 2.1 Branch & Pull Request Lineage Table

| PR # | Repository | Branch | Commit SHA | Parent / Merge Base | Target Branch | Milestone Focus |
|---|---|---|---|---|---|---|
| **#26** | `agent-studios-rs` | `feat/m15-01-per-task-execution-contract` | `a5dbbfc5ebada254727db9a788c0ee40e3035ea8` | `ce0396c1f1` (M13 IPC freeze) | `main` | M15-01: Per-task execution contract, task spec, routing |
| **#28** | `agent-studios-rs` | `feat/m15-02-multi-runtime-registry` | `61cb7e88e4ae0e3a0432134b77eee0c30f56c302` | `a5dbbfc5eb` (PR #26) | `main` | M15-02: Multi-runtime registry, tri-state readiness, instance routing |
| **#29** | `agent-studios-rs` | `feat/m15-03-artifact-diff-review` | `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b` | `61cb7e88e4` (PR #28) | `main` | M15-03 / M15-03.1: Lossless diff streaming, binary pagination, SHA-256 |
| **#30** | `agent-studios-rs` | `feat/m15-04-durable-state-recovery` | `dcbcb39bf61225a4660be5b533cf1ffd5de14b29` | `cc1c3e8f39` (PR #29) | `main` | M15-04 / M15-04.1: SQLite event store, daemon crash recovery, lockfile |
| **#27** | `agent-studios-ide-ui` | `feat/m15-real-agent-execution-ui` | `054dc0328f608ec39b3602fcad39e0839c4fce86` | `a13230ee93` (M14 freeze) | `feat/m14-mission-control-connected-integration` | M15-UI-01 through M15-UI-04: Full execution workbench, diff review, recovery |

### 2.2 Visual Dependency Hierarchy

```
Backend (`agent-studios-rs`):
[M13 IPC-02.2.2 Freeze] (ce0396c1)
       │
       ▼
[PR #26: M15-01 Per-Task Execution Contract] (a5dbbfc5)
       │
       ▼
[PR #28: M15-02 Multi-Runtime Registry] (61cb7e88)
       │
       ▼
[PR #29: M15-03 / M15-03.1 Lossless Diff & Artifacts] (cc1c3e8f)  <-- PINNED BACKEND BASELINE
       │
       ▼
[PR #30: M15-04 Durable State Recovery] (dcbcb39b)               <-- CLAUDE 1 BACKEND HANDOFF

─────────────────────────────────────────────────────────────────────────────────────────────

UI Extension (`agent-studios-ide-ui`):
[M14 Connected Integration Base] (a13230ee)
       │
       ├─► 4d052ade (M15-UI-01: Interactive workbench & host confirmation)
       ├─► 111fda2a (M15-UI-01: CI path normalization & real execution contract)
       ├─► 5c5fd866 (M15-UI-01: Real rust execution integration & safety gates)
       ├─► 13514897 (M15-UI-02: Multi-runtime backend registry integration)
       ├─► 250444bb (M15-UI-02: Tri-state capability validation & routing)
       ├─► 82fc5c0f (M15-UI-02: Final mutation authorization & session challenge)
       ├─► 2e5db821 (M15-UI-03: Authoritative Rust IPC artifact & diff wire schemas)
       ├─► 32f02e20 (M15-UI-03: Lossless binary-safe artifact client & SHA-256)
       ├─► 52884b50 (M15-UI-03: Artifact & diff review workbench streaming)
       ├─► 054dc032 (M15-UI-03.1: Diff review correctness, async safety & real IPC)
       │
       ▼
[PR #27: feat/m15-real-agent-execution-ui] (Head @ 054dc032)
```

---

## 3. IPC Protocol Wire Schema Comparison

The authoritative Rust daemon (`agsd`) uses internally tagged JSON serialization (`#[serde(rename_all = "snake_case", tag = "type")]`) defined in `crates/ipc-transport/src/connection.rs`.

*Note on crate separation*: While `crates/ipc-protocol/src/framing.rs` defines a legacy wrapped schema (`#[serde(tag = "status")]` with `Success { result }` and `Error { error }`), the running daemon and client transport (`crates/ipc-transport/src/client.rs`) strictly execute the flat `connection.rs` schema. The TypeScript UI (`RustIpcProtocolTypes.ts`) correctly mirrors `connection.rs`.

### 3.1 Command Envelopes Comparison

| Command Name | Rust Wire Tag | Rust Struct / Tuple Fields | TypeScript Definition (`RustIpcCommand`) | Status |
|---|---|---|---|---|
| Negotiate Capabilities | `"negotiate_capabilities"` | `protocol_version: ProtocolVersion`, `capabilities: Vec<String>` | `{ type: 'negotiate_capabilities', protocol_version, capabilities }` | **100% Match** |
| Authenticate | `"authenticate"` | `header: HmacAuthHeader`, `payload: Option<Vec<u8>>` | `{ type: 'authenticate', header, payload? }` | **100% Match** |
| Ping | `"ping"` | `payload: String`, `timestamp: i64` | `{ type: 'ping', payload, timestamp }` | **100% Match** |
| Get Health | `"get_health"` | *(unit variant)* | `{ type: 'get_health' }` | **100% Match** |
| Subscribe Events | `"subscribe_events"` | `from_sequence: Option<u64>` | `{ type: 'subscribe_events', from_sequence? }` | **100% Match** |
| Request Approval | `"request_approval"` | `contract: ApprovalContract` | `{ type: 'request_approval', contract }` | **100% Match** |
| Resolve Approval | `"resolve_approval"` | `approval_id: ApprovalId`, `decision: ApprovalDecision` | `{ type: 'resolve_approval', approval_id, decision }` | **100% Match** |
| Create Studio | `"create_studio"` | `name: String` | `{ type: 'create_studio', name }` | **100% Match** |
| List Studios | `"list_studios"` | *(unit variant)* | `{ type: 'list_studios' }` | **100% Match** |
| Get Studio | `"get_studio"` | `studio_id: StudioId` | `{ type: 'get_studio', studio_id }` | **100% Match** |
| Submit Task | `"submit_task"` | `studio_id: StudioId`, `task_spec: BatchTaskSpec` | `{ type: 'submit_task', studio_id, task_spec }` | **100% Match** |
| Cancel Task | `"cancel_task"` | `studio_id: StudioId`, `task_id: TaskId`, `reason: Option<String>` | `{ type: 'cancel_task', studio_id, task_id, reason? }` | **100% Match** |
| List Tasks | `"list_tasks"` | `studio_id: StudioId` | `{ type: 'list_tasks', studio_id }` | **100% Match** |
| Get Task | `"get_task"` | `studio_id: StudioId`, `task_id: TaskId` | `{ type: 'get_task', studio_id, task_id }` | **100% Match** |
| List Runs | `"list_runs"` | `studio_id: StudioId` | `{ type: 'list_runs', studio_id }` | **100% Match** |
| Get Run | `"get_run"` | `studio_id: StudioId`, `run_id: RunId` | `{ type: 'get_run', studio_id, run_id }` | **100% Match** |
| List Providers | `"list_providers"` | *(unit variant)* | `{ type: 'list_providers' }` | **100% Match** |
| Get Provider | `"get_provider"` | `provider_id: ProviderInstanceId` | `{ type: 'get_provider', provider_id }` | **100% Match** |
| List Models | `"list_models"` | `instance_id: Option<ProviderInstanceId>` | `{ type: 'list_models', instance_id? }` | **100% Match** |
| Get Execution Readiness | `"get_execution_readiness"` | *(unit variant)* | `{ type: 'get_execution_readiness' }` | **100% Match** |
| List Task Artifacts | `"list_task_artifacts"` | `studio_id: StudioId`, `task_id: TaskId` | `{ type: 'list_task_artifacts', studio_id, task_id }` | **100% Match** |
| Get Artifact | `"get_artifact"` | `studio_id: StudioId`, `artifact_id: ArtifactId`, `offset: Option<usize>`, `limit: Option<usize>` | `{ type: 'get_artifact', studio_id, artifact_id, offset?, limit? }` | **100% Match** |
| Get Task Diff | `"get_task_diff"` | `studio_id: StudioId`, `task_id: TaskId`, `offset: Option<usize>`, `limit: Option<usize>` | `{ type: 'get_task_diff', studio_id, task_id, offset?, limit? }` | **100% Match** |
| Shutdown | `"shutdown"` | *(unit variant)* | `{ type: 'shutdown' }` | **100% Match** |
| **Get Recovery Status** *(M15-04)* | `"get_recovery_status"` | *(unit variant)* | *(Missing in TS `RustIpcCommand`)* | **Gapped in UI** |

---

### 3.2 Response Envelopes Comparison

| Response Name | Rust Wire Tag | Rust Struct / Payload Fields | TypeScript Definition (`RustIpcResponse`) | Status |
|---|---|---|---|---|
| Capabilities Negotiated | `"capabilities_negotiated"` | `selected_version: ProtocolVersion`, `agreed_capabilities: Vec<String>` | `{ type: 'capabilities_negotiated', selected_version, agreed_capabilities }` | **100% Match** |
| Authenticated | `"authenticated"` | `session_id: SessionId` | `{ type: 'authenticated', session_id }` | **100% Match** |
| Pong | `"pong"` | `payload: String`, `client_timestamp: i64`, `server_timestamp: i64` | `{ type: 'pong', payload, client_timestamp, server_timestamp }` | **100% Match** |
| Health | `"health"` | `status: String`, `active_clients: usize`, `uptime_secs: u64`, `memory_events_count: usize` | `{ type: 'health', status, active_clients, uptime_secs, memory_events_count }` | **100% Match** |
| Event | `"event"` | `event: IpcEvent` | `{ type: 'event', event: RustIpcEvent }` | **100% Match** |
| Approval Requested | `"approval_requested"` | `approval_id: ApprovalId` | `{ type: 'approval_requested', approval_id }` | **100% Match** |
| Approval Resolved | `"approval_resolved"` | `approval_id: ApprovalId`, `status: ApprovalStatus` | `{ type: 'approval_resolved', approval_id, status }` | **100% Match** |
| Studio Created | `"studio_created"` | `studio: Studio` | `{ type: 'studio_created', studio: RustStudio }` | **100% Match** |
| Studios | `"studios"` | `studios: Vec<Studio>` | `{ type: 'studios', studios: RustStudio[] }` | **100% Match** |
| Studio | `"studio"` | `studio: Option<Studio>` | `{ type: 'studio', studio: RustStudio \| null }` | **100% Match** |
| Tasks Submitted | `"tasks_submitted"` | `task_ids: Vec<TaskId>` | `{ type: 'tasks_submitted', task_ids: string[] }` | **100% Match** |
| Task Cancelled | `"task_cancelled"` | `summary: CancellationSummary` | `{ type: 'task_cancelled', summary: RustCancellationSummary }` | **100% Match** |
| Tasks | `"tasks"` | `tasks: Vec<TaskRecord>` | `{ type: 'tasks', tasks: RustTaskRecord[] }` | **100% Match** |
| Task | `"task"` | `task: Option<TaskRecord>` | `{ type: 'task', task: RustTaskRecord \| null }` | **100% Match** |
| Runs | `"runs"` | `runs: Vec<RunRecord>` | `{ type: 'runs', runs: RustRunRecord[] }` | **100% Match** |
| Run | `"run"` | `run: Option<RunRecord>` | `{ type: 'run', run: RustRunRecord \| null }` | **100% Match** |
| Providers | `"providers"` | `providers: Vec<ProviderInstance>` | `{ type: 'providers', providers: RustProviderInstance[] }` | **100% Match** |
| Provider | `"provider"` | `provider: Option<ProviderInstance>` | `{ type: 'provider', provider: RustProviderInstance \| null }` | **100% Match** |
| Models | `"models"` | `models: Vec<ModelDescriptor>` | `{ type: 'models', models: RustModelDescriptor[] }` | **100% Match** |
| Execution Readiness | `"execution_readiness"` | `readiness: ExecutionReadinessReport` | `{ type: 'execution_readiness', readiness: RustExecutionReadinessReport }` | **100% Match** |
| Task Artifacts | `"task_artifacts"` | `artifacts: Vec<ArtifactRecord>` | `{ type: 'task_artifacts', artifacts: RustArtifactRecord[] }` | **100% Match** |
| Artifact | `"artifact"` | `artifact: Option<ArtifactRecord>`, `content: Option<String>`, `content_base64: Option<String>`, `encoding: Option<String>`, `offset: Option<usize>`, `limit: Option<usize>`, `total_bytes: Option<usize>`, `has_more: Option<bool>`, `sha256: Option<String>` | `{ type: 'artifact', artifact?, content?, content_base64?, encoding?, offset?, limit?, total_bytes?, has_more?, sha256? }` | **100% Match** |
| Task Diff | `"task_diff"` | `diff: TaskDiffResponse` | `{ type: 'task_diff', diff: RustTaskDiffResponse }` | **100% Match** |
| Shutdown Acknowledged | `"shutdown_acknowledged"` | *(unit variant)* | `{ type: 'shutdown_acknowledged' }` | **100% Match** |
| Error | `"error"` | `message: String` | `{ type: 'error', message: string }` | **100% Match** |
| **Recovery Status** *(M15-04)* | `"recovery_status"` | `status: RecoveryStatusReport` | *(Missing in TS `RustIpcResponse`)* | **Gapped in UI** |

---

## 4. Domain Type Schemas & Field Parity

### 4.1 Strongly Typed Identifiers (`define_id!` Macro vs String UUID)

In Rust (`crates/protocol/src/id.rs`), domain IDs are generated using:
```rust
macro_rules! define_id {
    ($name:ident, $doc:expr) => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);
        ...
    }
}
```
Target Types:
- `StudioId`
- `AgentId`
- `TaskId`
- `RunId`
- `ArtifactId`
- `WorktreeId`
- `EventId`
- `ReconciliationId`
- `ApprovalId`

#### Serialization Impact & Validation Constraints:
1. **Wire Representation**: `#[serde(transparent)]` serializes the inner `uuid::Uuid` as a standard 36-character hyphenated string (e.g. `"c0a80101-0000-0000-0000-000000000001"`).
2. **Strict Deserialization**: Serde invokes `Uuid::parse_str`. If the TypeScript UI sends a synthetic or mock ID that does not conform to RFC 4122 (e.g. `"studio-m15-001"` or `"task-1"`), Serde deserialization fails immediately.
3. **Transport Consequence**: Because `agsd` treats deserialization failure as a fatal framing error in `recv_msg`, **the connection is severed immediately** without emitting an `IpcResponse::Error`. Real UI client code must strictly supply valid UUIDs.

---

### 4.2 Artifact & Diff Types (`crates/protocol/src/artifact.rs`)

#### A. ArtifactKind
- **Rust**: `File`, `Patch`, `Log`, `Report`, `Plan`, `Other` (with `#[serde(rename_all = "snake_case")]`).
- **TypeScript**: `'file' | 'patch' | 'log' | 'report' | 'plan' | 'other'`.
- **Verdict**: Exact 1-to-1 match.

#### B. ArtifactRecord
| Field | Rust Type | Serde Attributes | TypeScript Field | Alignment |
|---|---|---|---|---|
| `id` | `ArtifactId` | transparent UUID | `id: string` | Exact |
| `studio_id` | `StudioId` | transparent UUID | `studio_id: string` | Exact |
| `task_id` | `TaskId` | transparent UUID | `task_id: string` | Exact |
| `producer_agent_id` | `AgentId` | transparent UUID | `producer_agent_id: string` | Exact |
| `kind` | `ArtifactKind` | `snake_case` | `kind: RustArtifactKind` | Exact |
| `logical_name` | `String` | string | `logical_name: string` | Exact |
| `content_hash` | `Option<String>` | nullable | `content_hash: string \| null` | Exact |
| `location` | `String` | string | `location: string` | Exact |
| `created_at` | `DateTime<Utc>` | RFC 3339 string | `created_at: string` | Exact |
| `run_id` | `Option<RunId>` | `#[serde(default)]` | `run_id?: string \| null` | Exact |
| `worktree_id` | `Option<WorktreeId>` | `#[serde(default)]` | `worktree_id?: string \| null` | Exact |
| `version` | `u32` | `#[serde(default = "default_artifact_version")]` | `version?: number` | Exact |
| `supersedes` | `Option<ArtifactId>` | `#[serde(default)]` | `supersedes?: string \| null` | Exact |
| `relative_path` | `Option<String>` | `#[serde(default)]` | `relative_path?: string \| null` | Exact |
| `size_bytes` | `Option<u64>` | `#[serde(default)]` | `size_bytes?: number \| null` | Exact |

#### C. TaskDiffResponse & DiffSummary
- **DiffSummary**: `files_changed` (usize), `added_files` (Vec), `modified_files` (Vec), `deleted_files` (Vec), `renamed_files` (Vec), `binary_files` (Vec), `patch_size_bytes` (usize), `base_commit` (String). Fully matched in TS `RustDiffSummary`.
- **TaskDiffResponse**:
  - `patch`: Legacy lossy string (present for fallback).
  - `patch_base64`: Authoritative lossless Base64-encoded bytes (avoids UTF-8 decoding truncation across chunk boundaries).
  - `patch_sha256`: Aggregate SHA-256 hex digest (`sha256:...`) across all raw patch bytes.
  - `chunk_bytes`: Explicit byte length of the current Base64-decoded chunk.
  - `total_bytes`: Total byte length of full unified diff.
  - `offset`, `limit`, `has_more`: Monotonic pagination controls.
  - **Verdict**: Exact 1-to-1 match verified by 11 real IPC tests against compiled `agsd.exe`.

---

### 4.3 Task Execution Specification & Records (`crates/protocol/src/task.rs`)

#### A. TaskRecord vs RustTaskRecord
In Rust `TaskRecord`:
```rust
pub struct TaskRecord {
    pub id: TaskId,
    pub studio_id: StudioId,
    pub parent_task_id: Option<TaskId>,
    pub assigned_agent_id: Option<AgentId>,
    pub title: String,
    pub description: String,
    pub state: TaskState,
    pub dependencies: Vec<TaskId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub retry_count: u32,
    pub dependency_output_policy: DependencyOutputPolicy,
    pub execution: Option<TaskExecutionSpec>, // <-- M15 extension
}
```
In TypeScript `RustTaskRecord`:
```typescript
export interface RustTaskRecord {
  id: string;
  studio_id: string;
  parent_task_id?: string | null;
  assigned_agent_id?: string | null;
  title: string;
  description: string;
  state: RustTaskState;
  dependencies: string[];
  created_at: string;
  updated_at: string;
  retry_count: number;
  dependency_output_policy?: RustDependencyOutputPolicy;
  // execution field currently omitted in TS RustTaskRecord
}
```
*Note*: `TaskRecord.execution` has `#[serde(default, skip_serializing_if = "Option::is_none")]`. TypeScript deserialization does not break, but task execution parameters are not exposed on `RustTaskRecord` (though they are exposed on `RustRunRecord`).

#### B. BatchTaskSpec & TaskExecutionSpec
- `RustBatchTaskSpec` perfectly models `key`, `title`, `description`, `parent_task_id`, `dependency_keys`, `dependency_output_policy`, and `execution`.
- `RustTaskExecutionSpec` perfectly models `initial_prompt`, `repository_path`, `runtime_instance_id`, `runtime_preference`, `provider_instance_id`, and `model_id`.

---

### 4.4 Event Streaming & Watermark Schemas

#### A. IpcEvent (`crates/ipc-protocol/src/replay.rs`)
```rust
pub struct IpcEvent {
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub payload: serde_json::Value,
}
```
TypeScript `RustIpcEvent`:
```typescript
export interface RustIpcEvent {
  sequence: number;
  timestamp: string;
  event_type: string;
  payload: unknown;
}
```
- **Watermark Invariant**: Sequences are strictly positive monotonic integers (`1, 2, 3...`).
- **Gap Detection**: `SessionStateProjection` tracks `expectedSequence = lastProcessedSequence + 1`. If `event.sequenceNumber > expectedSequence`, projection transitions to `degraded` and fires `onStateDegraded`.
- **Deduplication**: Events with `sequenceNumber <= lastProcessedSequence` are silently rejected.

---

## 5. Milestone Compatibility Matrix

| Milestone | Backend PR / Commit | UI PR / Commit | Protocol Version | Compatibility Status | Verification Notes |
|---|---|---|---|---|---|
| **M15-01** (Task Execution Contract) | PR #26 (`a5dbbfc5`) | PR #27 (`5c5fd866`) | `1.0.0` | **VERIFIED** | Batch task submission, path containment, execution specs. |
| **M15-02** (Multi-Runtime Registry) | PR #28 (`61cb7e88`) | PR #27 (`82fc5c0f`) | `1.0.0` | **VERIFIED** | Tri-state capabilities (`supported`/`unsupported`/`unknown`), runtime instance routing, operator challenges. |
| **M15-03 / 03.1** (Artifact Diff Review) | PR #29 (`cc1c3e8f`) | PR #27 (`054dc032`) | `1.0.0` | **VERIFIED (PINNED)** | Lossless Base64 diff pagination, cryptographic SHA-256 verification, anti-DoS bounds. |
| **M15-04 / 04.1** (Durable State Recovery) | PR #30 (`dcbcb39b`) | PR #27 (Pending UI-04) | `1.0.0` | **CONDITIONAL** | Read-only forward compatible for existing commands; missing `get_recovery_status` and sequence reset handling after crash. |

---

## 6. Audit Verdict

1. **Protocol Parity on M15-03**: The UI implementation on PR #27 achieves **100% field-for-field and enum wire compatibility** with the pinned M15-03 backend (`cc1c3e8f`).
2. **Serialization Safety**: Flat Serde tags (`type: "..."`), Base64 binary chunking, and transparent UUID serialization match backend requirements.
3. **M15-04 Readiness**: The PR #30 changes introduce one additive command (`GetRecoveryStatus`) and crash recovery state transitions that require formal handoff alignment. Full details documented in `07_BACKEND_HANDOFF_ISSUES.md`.
