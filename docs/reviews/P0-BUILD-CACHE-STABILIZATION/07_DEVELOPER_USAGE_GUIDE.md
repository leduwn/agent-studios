# P0-BUILD-CACHE-STABILIZATION: 07 Developer Usage Guide

**Document ID**: `P0-BCS-07-DEVELOPER-USAGE-GUIDE`  
**Date**: 2026-11-06  
**Status**: APPROVED INSTRUCTIONS  

---

## 1. Quick Start

All scripts reside in `scripts/build-cache/` and can be invoked from any worktree:

### A. Preflight Disk & Target Inspection
```powershell
.\scripts\build-cache\Get-BuildCacheDiagnostics.ps1
```
Reports Drive D: and Drive C: free space, active compilation processes, and mapped Cargo target paths for each worktree.

### B. Running Cargo Commands with Cache on Drive C:
```powershell
# Typecheck workspace
.\scripts\build-cache\Invoke-CargoWithCache.ps1 -CargoArgs @("check", "--workspace", "--all-targets")

# Run crate tests
.\scripts\build-cache\Invoke-CargoWithCache.ps1 -CargoArgs @("test", "-p", "agent-studios-workspace")

# Specify an external worktree explicitly
.\scripts\build-cache\Invoke-CargoWithCache.ps1 -WorktreePath "D:\Projects\agent-studios-active\m15-1-backend" -CargoArgs @("check")
```

### C. Building the Native agsd Daemon with Cryptographic Provenance
```powershell
# Build debug binary and record provenance
.\scripts\build-cache\Build-AgsdWithCache.ps1 -Profile debug

# Build optimized release binary
.\scripts\build-cache\Build-AgsdWithCache.ps1 -Profile release
```
Outputs the binary path under `C:\AgentStudiosBuild\cargo\<worktree-id>\<profile>\agsd.exe` and writes `C:\AgentStudiosBuild\provenance\<worktree-id>-<profile>.provenance.json`.

### D. Running Frontend E2E Suites with Provenance Enforcement
```powershell
# Run E2E tests using the verified daemon compiled from current HEAD
.\scripts\build-cache\Invoke-E2EWithCache.ps1 -Profile debug -TestCommand @("npm", "run", "test:e2e")

# Auto-rebuild the daemon if missing or stale
.\scripts\build-cache\Invoke-E2EWithCache.ps1 -Profile debug -AutoBuildIfMissing
```

---

## 2. Environment Customization

- **Default Root**: `C:\AgentStudiosBuild`
- **Custom Root Override**:
  Developers wishing to relocate cache storage (e.g. to another high-speed NVMe drive) can set:
  ```powershell
  $env:AGENT_STUDIOS_BUILD_ROOT = "E:\AgentStudiosBuild"
  ```
  or pass the `-BuildRoot <path>` parameter to any script.
