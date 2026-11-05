# Milestone M13 IPC-02 — Final Handoff & Release Sign-Off

- **Milestone**: `M13 IPC-02 Native Control Plane Daemon Integration`
- **Execution Run**: `RUN-20261030-IPC-02`
- **Date**: `2026-10-30T00:35:00+07:00`
- **Active Branch**: `feat/m13-ipc-02-control-plane`
- **Base Branch**: `feat/m13-ipc-native-transport`
- **Status**: **READY FOR PULL REQUEST / STRICT NO SELF-MERGE**

---

## 1. Scope of Completed Deliverables

1. **`crates/ipc-protocol`**:
   - Extended `IpcCommand` framing with typed control plane and provider catalog commands:
     - `CreateStudio { name }`
     - `ListStudios`
     - `GetStudio { studio_id }`
     - `SubmitTask { studio_id, task_spec }`
     - `CancelTask { studio_id, task_id, reason }`
     - `ListTasks { studio_id }`
     - `GetTask { studio_id, task_id }`
     - `ListRuns { studio_id }`
     - `GetRun { studio_id, run_id }`
     - `ListProviders`
     - `GetProvider { provider_id }`
     - `ListModels { instance_id }`
   - Added unit serialization / deserialization tests for all new command variants.

2. **`crates/control-plane`**:
   - Added `grant_approval` and `reject_approval` convenience helper methods transitioning approval states through `self.resolve_approval(id, ApprovalState::Approved / ApprovalState::Denied)`.

3. **`crates/ipc-transport`**:
   - `Cargo.toml`: Added workspace dependencies on `agent-studios-control-plane`, `agent-studios-protocol`, and `agent-studios-provider`.
   - `src/server.rs`:
     - Embedded `pub control_plane: Arc<tokio::sync::RwLock<ControlPlane>>` and `pub provider_catalog: Arc<tokio::sync::RwLock<ProviderCatalog>>` into `ServerState`.
     - Added `publish_control_plane_events` to serialize committed `EventEnvelope`s, assign monotonic daemon sequences, record in `ReplayBuffer`, and broadcast to subscribers.
     - Updated `IpcServerBuilder` with `.with_control_plane()` and `.with_provider_catalog()`.
   - `src/connection.rs`:
     - Implemented authoritative request dispatch for studios, task batch submission, task cancellation, runs, approvals, and catalog inspection.
     - Dual-dispatch correlation on `ResolveApproval`: checks Control Plane approval requests first, falling back to transport-level contracts.
     - Race-free subscription: subscribes broadcast channel before acquiring buffer lock and deduplicates against buffered watermark.
   - `src/client.rs`:
     - Implemented typed async client helper methods: `create_studio`, `list_studios`, `get_studio`, `submit_task`, `cancel_task`, `list_tasks`, `get_task`, `list_runs`, `get_run`, `list_providers`, `get_provider`, `list_models`, `resolve_approval`.
   - `src/daemon.rs`:
     - Initialized `ControlPlane::new_default()` and `ProviderCatalog::with_builtin_definitions()` in daemon startup.
   - `tests/control_plane_ipc_tests.rs`:
     - Added 6 integration tests verifying studio discovery, task lifecycle, cancellation, approval resolution, catalog inspection, and linearizable replay with reconnect.

---

## 2. Release & Governance Checklist

- [x] All 86 tests passing across `agent-studios-ipc-protocol` and `agent-studios-ipc-transport`.
- [x] Clippy clean across all targets with zero warnings under `-D warnings`.
- [x] Rustfmt clean across all files (`cargo fmt -- --check`).
- [x] Subtree `codex-rs/` is 100% untouched.
- [x] Zero bearer secrets placed in git repositories or fixtures.
- [x] Single authority invariant maintained: Control Plane is the single source of truth; zero secondary DBs created.
- [x] Strict no self-merge rule observed.
