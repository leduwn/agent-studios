# P0-BUILD-CACHE-STABILIZATION: 02 Cargo Target Directory Policy

**Document ID**: `P0-BCS-02-CARGO-TARGET-DIRECTORY-POLICY`  
**Date**: 2026-11-06  
**Status**: APPROVED ARCHITECTURE  

---

## 1. Objective and Problem Statement

Cargo workspaces default to creating compilation artifacts in `<workspace_root>/target`. In multi-worktree environments on disk-constrained systems (Drive D: at 110 GB total), multiple checkouts (`agent-studios`, `m15-1-backend`, `ci-repair`, `m15-ui-06`) accumulated over **77.71 GB** of build targets, driving available storage down to 0.49%.

Drive C: provides **293+ GiB** of available capacity. This policy establishes a non-destructive, opt-in mechanism to route all future Rust build outputs to Drive C: without moving, deleting, or altering existing checked-in files or targets on Drive D:.

---

## 2. Architecture: Local Build Root on Drive C:

The standardized local build root is established at:
```text
C:\AgentStudiosBuild\
  cargo\             # Isolated per-worktree Cargo target directories
  provenance\        # Cryptographic build provenance records (.provenance.json)
  diagnostics\       # Preflight and disk health logs
  worktrees\         # Dedicated development worktrees hosted on Drive C:
```

### Process-Scoped `CARGO_TARGET_DIR`
1. **No Global System Mutation**:
   The policy strictly prohibits modifying the system-wide environment or machine `PATH`. `CARGO_TARGET_DIR` is set exclusively within the execution scope of individual PowerShell wrapper scripts (`Invoke-CargoWithCache.ps1`, `Build-AgsdWithCache.ps1`).
2. **Deterministic Cleanup**:
   Upon script exit or termination (via `try...finally`), the environment variable is restored to its prior state or unset, ensuring standard terminal sessions remain unaffected.
3. **Preservation of Existing Targets**:
   The existing canonical target on Drive D: (`28.92 GB`) and backend target (`13.06 GB`) remain untouched.

---

## 3. GitHub Actions CI Separation

- **Strict Runner Isolation**:
  GitHub CI workflows running on `ubuntu-latest` and `windows-latest` do not have a `C:\AgentStudiosBuild` drive structure.
- **No CI Workflow Mutation**:
  The CI workflows compile `agsd` within the runner's native workspace checkout. This policy applies exclusively to local Windows developer environments and does not modify the GitHub CI pipeline configuration.
