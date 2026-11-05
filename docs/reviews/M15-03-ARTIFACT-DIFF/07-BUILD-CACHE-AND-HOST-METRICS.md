# M15-03 Build Cache, Host Footprint and Resource Metrics

**Milestone:** M15-03  
**Status:** Complete  
**Date:** 2026-11-01  
**Author:** Windows DevOps Engineer & Systems Architect  

---

## 1. Host Environment Context

The Agent Studios development environment runs on a resource-constrained Windows 11 host with concurrent active Claude Code sessions (including VS Code IDE UI session in `D:\Projects\agent-studios-ide-ui` and active root session in `D:\Projects\agent-studios`).

### Invariants Observed
1. **Zero Intrusion on Concurrent Sessions**: No processes associated with PID 52920 or PID 26780 were touched.
2. **Zero Modification to `codex-rs/`**: All upstream codex-rs submodules remain unmodified.
3. **Strict Enforcement of `CARGO_INCREMENTAL=0`**: Prevents unbounded incremental compilation database growth on drive D:.
4. **Targeted Workspace Builds**: Commands explicitly specify `--manifest-path "D:\Projects\agent-studios-m15-03\agent-studios-rs\Cargo.toml"` to prevent workspace ambiguity.

---

## 2. Disk & Memory Footprint

### 2.1 Disk Usage on Drive D:
- Baseline before cleanup: ~8.09 GB free (critical pressure).
- Disk space reclaimed during Phase 0 audit & safe cleanup: **+60.180 GB**.
- Current free space: **~68.27 GB free**.
- Total size of `agent-studios-m15-03/agent-studios-rs/target`: ~3.8 GB (bounded).

### 2.2 IPC Memory & Frame Bounds
- Max IPC Frame Size: 4,194,304 bytes (4 MiB).
- Max Artifact Blob Read Limit: 2,097,152 bytes (2 MiB).
- Max Diff Chunk Limit: 2,097,152 bytes (2 MiB).
- Ephemeral Git Index: allocated in OS `%TEMP%` and dropped within <100 ms per capture invocation.

---

## 3. Host Process Concurrency Profile

| Component | Concurrency Model | Resource Cap |
|---|---|---|
| Ephemeral Index Capture | Asynchronous spawn via `tokio::task::spawn_blocking` | Max 1 active Git process per task termination |
| Artifact Store Blob Writes | Atomic file rename | Single-threaded synchronous FS operation |
| IPC Server Connections | Tokio async tasks (`duplex` or TCP) | Bound to active client connections |
| Event Broadcast Buffer | Tokio broadcast channel | 1,024 events capacity |
| Control Plane Mutex | Async `RwLock` | Read-heavy, write-guarded |
