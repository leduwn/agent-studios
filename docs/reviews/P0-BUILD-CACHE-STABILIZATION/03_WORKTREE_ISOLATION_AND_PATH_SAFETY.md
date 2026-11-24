# P0-BUILD-CACHE-STABILIZATION: 03 Worktree Isolation & Path Safety

**Document ID**: `P0-BCS-03-WORKTREE-ISOLATION-AND-PATH-SAFETY`  
**Date**: 2026-11-06  
**Status**: APPROVED ARCHITECTURE  

---

## 1. Multi-Worktree Isolation Invariants

Pointing multiple Git worktrees to a single shared Cargo target directory causes severe operational failures:
- **Cargo Build File Locks**: Concurrent or overlapping builds fail with `error: could not acquire lock`.
- **Daemon Binary Overwriting**: Branch-specific revisions of `agsd.exe` clobber each other.
- **Protocol Mismatches**: Unsynchronized schema changes between backend and UI branches trigger runtime IPC connection panics.
- **Stale Intermediate Dependencies**: Incremental compilation caches cross-pollinate, leading to corrupt rlibs and mysterious linker failures.

### Solution: Deterministic Collision-Resistant Target Naming
Each worktree is mapped to a strictly unique target subdirectory under `C:\AgentStudiosBuild\cargo`:
```powershell
$leaf = Split-Path $fullPath -Leaf
$hash8 = Get-PathHash8 $fullPath   # First 8 hex characters of SHA-256(normalized_full_path)
$worktreeId = "{0}-{1}" -f $leaf, $hash8
$targetDir = Join-Path $cargoRoot $worktreeId
```

### Verified Mapping Examples:
| Worktree | Branch | Target Directory on Drive C: |
| :--- | :--- | :--- |
| `D:\Projects\agent-studios` | `integration/m15-unified-codebase` | `C:\AgentStudiosBuild\cargo\agent-studios-bc99f1f0` |
| `D:\Projects\agent-studios-active\m15-1-backend` | `feat/m15-1-provider-cli-unification` | `C:\AgentStudiosBuild\cargo\m15-1-backend-411eb255` |
| `D:\Projects\agent-studios-active\ci-repair` | `fix/m15-unified-cross-platform-ci` | `C:\AgentStudiosBuild\cargo\ci-repair-021f16b1` |
| `D:\Projects\agent-studios-active\m15-ui-06` | `feat/m15-ui-06-agent-ide-continuity` | `C:\AgentStudiosBuild\cargo\m15-ui-06-ee061801` |
| `C:\AgentStudiosBuild\worktrees\build-cache-policy` | `chore/dev-build-cache-policy` | `C:\AgentStudiosBuild\cargo\build-cache-policy-153b2e8d` |

---

## 2. Path Safety and Security Assertions

The module `BuildCacheModule.psm1` enforces strict path validation via `Assert-SafeDirectoryPath`:
1. **Path Normalization**: Canonicalizes all drive letters and separators using `[System.IO.Path]::GetFullPath()`.
2. **Directory Traversal Rejection**: Rejects any path containing `..` or relative components attempting to escape `C:\AgentStudiosBuild\cargo`.
3. **Reparse Point / Junction Rejection**: Explicitly inspects `[System.IO.FileAttributes]::ReparsePoint`. If any target or parent directory is a symbolic link or junction, the operation fails closed with a `Security Violation`.
4. **Workspace Verification**: Requires `agent-studios-rs\Cargo.toml` to exist in the target worktree before executing any build command.
