# M15-04 Cross-Platform Compatibility Audit: Windows & Unix

## 1. Operating System Differences

The persistence and recovery mechanisms interact with file systems, directory defaults, and IPC mechanisms across platforms:
- **Windows:** Named Pipes (`\\.\pipe\agent-studios-...`), `%APPDATA%\AgentStudios` state path, `%LOCALAPPDATA%\agent-studios` runtime path, Windows Security Attributes (DACLs).
- **Unix (Linux / macOS):** Unix Domain Sockets (`/tmp/agent-studios-...`), `~/.local/share/agent-studios/` or `$XDG_DATA_HOME` state path, `$XDG_RUNTIME_DIR` runtime path, POSIX file permissions (`0700` dirs, `0600` files).

---

## 2. Directory Resolution & Isolation

`DaemonConfig` implements platform-native default paths:

```rust
pub fn default_state_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(app_data) = std::env::var("APPDATA") {
            return PathBuf::from(app_data).join("AgentStudios");
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".local").join("share").join("agent-studios");
        }
    }
    std::env::temp_dir().join("agent-studios-state")
}
```

---

## 3. SQLite Locking & Cross-Platform Reliability

1. **WAL Shared Memory (`-shm` and `-wal`):**
   - On Windows, SQLite maps `-shm` via memory-mapped files. Process termination without clean close is safely recovered on next open via SQLite's built-in WAL recovery algorithm.
   - On Unix, POSIX advisory locks and shared memory (`mmap`) provide identical guarantees.
2. **File Path Canonicalization (`dunce`):**
   - Path comparisons across Git checkouts and worktree roots use `dunce::canonicalize` to strip Windows verbatim UNC prefixes (`\\?\`), preventing false-positive worktree mismatches.
3. **Database Pre-Flight Check:**
   - Works identically across platforms via `PRAGMA quick_check;`.
