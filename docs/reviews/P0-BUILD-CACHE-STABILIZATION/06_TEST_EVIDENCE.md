# P0-BUILD-CACHE-STABILIZATION: 06 Test Evidence

**Document ID**: `P0-BCS-06-TEST-EVIDENCE`  
**Date**: 2026-11-06  
**Status**: VERIFIED & PASSING  

---

## 1. Unit Test Suite Execution (`tests/test_build_cache_policy.ps1`)

The dedicated test suite exercises core module logic, path safety assertions, capacity guards, and provenance tracking:

```text
================================================================================
RUNNING BUILD CACHE POLICY UNIT TESTS
================================================================================
  [PASS] Hash is deterministic
  [PASS] Different paths produce different hashes
  [PASS] Hash format is 8 hexadecimal characters
  [PASS] Safe path within base dir succeeds
  [PASS] Escaped path traversal throws Security Violation
  [PASS] Worktree path resolved correctly
  [PASS] Branch resolved as chore/dev-build-cache-policy
  [PASS] HEAD is non-empty SHA
  [PASS] WorktreeId matches format
  [PASS] Target directory starts with C:\AgentStudiosBuild\cargo
  [PASS] Target directory contains worktree id
  [PASS] Drive D: free space reported
  [PASS] Drive C: free space reported
  [PASS] Provenance record generated with SHA-256
  [PASS] Provenance record references current HEAD
  [PASS] Provenance verification succeeds on unmodified binary
  [PASS] Provenance verification detects tampered binary SHA-256
================================================================================
TEST SUMMARY: 17 PASSED | 0 FAILED
================================================================================
```

---

## 2. Live Scoped Compilation Evidence (`Invoke-CargoWithCache.ps1`)

A scoped compilation test was executed against `agent-studios-protocol` to verify end-to-end redirection:

```text
================================================================================
AGENT STUDIOS CARGO WRAPPER (PROCESS-SCOPED TARGET ON DRIVE C:)
Worktree:         C:\AgentStudiosBuild\worktrees\build-cache-policy
Branch:           chore/dev-build-cache-policy
Git HEAD:         8525ec228c37e38c35ee514dc886af295fbc4e00
CARGO_TARGET_DIR: C:\AgentStudiosBuild\cargo\build-cache-policy-153b2e8d
================================================================================
Initial Disk: D: 37.07 GB free | C: 293.49 GB free
Executing: cargo check -p agent-studios-protocol
   Compiling proc-macro2 v1.0.107
   Compiling serde v1.0.229
   Compiling serde_json v1.0.151
   Compiling schemars v0.8.22
   Compiling uuid v1.26.1
   Compiling chrono v0.4.45
    Checking agent-studios-protocol v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 43.70s
Cargo finished in 44.00s with exit code 0
================================================================================
Final Disk:   D: 37.07 GB free (delta: 0 GB) | C: 293.18 GB free (delta: -0.31 GB)
================================================================================
```

### Verification Findings:
1. **Drive D: Free Space Delta**: Exactly `0 GB` (unaffected).
2. **Drive C: Free Space Delta**: `-0.31 GB` (stored exclusively in `C:\AgentStudiosBuild\cargo\build-cache-policy-153b2e8d`).
3. **Environment Cleanup**: `$env:CARGO_TARGET_DIR` restored to `$null` upon process completion.
4. **Zero Files Lost / Zero PRs Merged**: Conforms to all safety constraints.
