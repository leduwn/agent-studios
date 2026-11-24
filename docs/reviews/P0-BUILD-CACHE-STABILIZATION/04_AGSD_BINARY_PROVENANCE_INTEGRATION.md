# P0-BUILD-CACHE-STABILIZATION: 04 agsd Binary Provenance Integration

**Document ID**: `P0-BCS-04-AGSD-BINARY-PROVENANCE-INTEGRATION`  
**Date**: 2026-11-06  
**Status**: APPROVED ARCHITECTURE  

---

## 1. Context: AgsdBinaryResolver Threat Model

The VS Code extension E2E test harness uses:
`apps/agent-studios-vscode/src/test/e2e/AgsdBinaryResolver.ts`

### Two Execution Modes:
1. **Strict CI Mode (`CI=true` or `GITHUB_ACTIONS=true`)**:
   Enforces that the daemon binary is located strictly within the checked-out repository root on the CI runner, actively rejecting external paths to prevent supply chain tampering.
2. **Local Development Mode (`CI` unset)**:
   Supports developer override via environment variable `AGSD_BIN_PATH`:
   - Validates that `AGSD_BIN_PATH` exists on disk.
   - Enforces that `AGSD_BIN_PATH` does NOT point to obsolete historical worktrees (`m15-02`, `m15-03`, `m15-04`).
   - Resolves the binary and launches the E2E daemon.

---

## 2. Local Provenance Integration Pipeline

When compiling `agsd.exe` using `Build-AgsdWithCache.ps1`:
1. The script compiles `agsd.exe` into the worktree-specific target directory on Drive C: (`C:\AgentStudiosBuild\cargo\<worktree-id>\<profile>\agsd.exe`).
2. It computes the SHA-256 cryptographic hash of the compiled binary.
3. It captures the Git worktree root, current branch, and exact Git source commit HEAD (`git rev-parse HEAD`).
4. It writes an immutable provenance record to:
   `C:\AgentStudiosBuild\provenance\<worktree-id>-<profile>.provenance.json`

### Provenance Schema:
```json
{
  "schema_version": "1.0",
  "recorded_at_utc": "2026-11-06T10:15:00Z",
  "worktree_path": "C:\\AgentStudiosBuild\\worktrees\\build-cache-policy",
  "worktree_id": "build-cache-policy-153b2e8d",
  "git_branch": "chore/dev-build-cache-policy",
  "git_head": "8525ec228c37e38c35ee514dc886af295fbc4e00",
  "cargo_target_dir": "C:\\AgentStudiosBuild\\cargo\\build-cache-policy-153b2e8d",
  "build_command": "cargo build --bin agsd -p agent-studios-ipc-transport",
  "build_profile": "debug",
  "binary_path": "C:\\AgentStudiosBuild\\cargo\\build-cache-policy-153b2e8d\\debug\\agsd.exe",
  "binary_sha256": "3a4b5c...64_hex_chars...",
  "binary_size_bytes": 15420312
}
```

---

## 3. Stale Binary Rejection Invariant

When `Invoke-E2EWithCache.ps1` runs:
1. It reads the provenance record and verifies that `git_head` matches the active worktree commit HEAD.
2. If the worktree has new commits, the script refuses to execute the E2E test with the stale binary, throwing `Strict Provenance Error` (or automatically rebuilding if `-AutoBuildIfMissing` is specified).
3. It verifies that the binary's live SHA-256 matches the recorded hash, catching filesystem corruption or tampering.
4. It exports `AGSD_BIN_PATH` strictly in process scope, ensuring `AgsdBinaryResolver.ts` consumes the exact verified binary on Drive C:.
