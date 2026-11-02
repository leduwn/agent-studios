# M15-04.1 Durable Worktree and Artifact Layout Architecture

## 1. Storage Layout

The default temporary workspace directory (`temp_dir()/agent-studios-workspaces`) has been superseded by a persistent, platform-native state hierarchy:

### Directory Structure
```
<state_dir>/
├── agsd.db                      # SQLite WAL event store
├── agsd.db-wal                  # SQLite write-ahead log
├── agsd.db.lock                 # Single-writer cross-process lockfile
├── artifacts/                   # Content-addressed artifact storage
│   ├── blobs/
│   │   └── sha256/
│   │       ├── 3a7bd...         # Raw artifact blobs keyed by SHA-256
│   │       └── ...
│   └── tmp/                     # Staging directory for atomic writes
└── worktrees/                   # Managed worktree checkouts
    ├── wt-studio1-task1/
    └── ...
```

### Platform Defaults
- **Windows:** `%APPDATA%\AgentStudios\`
- **Linux/macOS:** `~/.local/share/agent-studios/`
- **Configurable:** Overridden by `--state_dir <path>` or programmatically via `IpcServerBuilder::with_workspace_roots`.

---

## 2. Integrity and Path Safety Invariants

1. **Path Traversal Protection:**
   All worktree roots and working directories are canonicalized via `dunce::canonicalize`. Any path escaping the managed root (`<state_dir>/worktrees/`) is rejected with `WorkspaceError::PathValidationFailed`.
2. **SHA-256 Content Addressed Integrity:**
   All artifacts retrieved via `GetArtifact` verify that the on-disk blob hash strictly matches the cataloged SHA-256 hash. Corrupted or missing blobs fail with `ArtifactStoreError::CorruptedBlob` or `BlobNotFound`.
3. **Preservation of Partial Work on Crash:**
   When a daemon crashes or restarts during task execution, the physical worktree and any partial git commits/diffs remain intact on disk. Worktrees are never deleted automatically during recovery.
4. **Startup Missing Worktree Reconciliation:**
   If a user or external tool deletes a managed worktree directory from disk while the daemon is offline, startup reconciliation detects `!root.exists()` and transitions the worktree to `WorktreeState::Failed`, preventing undefined execution states. Surviving worktrees remain valid and accessible.
