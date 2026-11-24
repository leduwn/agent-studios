# P0-BUILD-CACHE-STABILIZATION: 05 Disk Capacity Monitoring & Guards

**Document ID**: `P0-BCS-05-DISK-CAPACITY-MONITORING`  
**Date**: 2026-11-06  
**Status**: APPROVED ARCHITECTURE  

---

## 1. Capacity Thresholds & Action Matrix

To ensure that heavy Rust compilation tasks never drive Drive D: into an emergency state again, the module implements proactive runtime capacity guards:

| Drive | Metric | Condition | System Action | Operator Override |
| :--- | :--- | :--- | :--- | :--- |
| **D:** | Free Space | `< 15.0 GiB` | **BLOCKED**: Halts Cargo compilation immediately | `-IgnoreLimits` |
| **D:** | Free Space | `< 25.0 GiB` | **WARNING**: Emits warning in build logs | None required |
| **C:** | Free Space | `< 20.0 GiB` | **WARNING**: Emits warning in build logs | None required |

---

## 2. Preflight Diagnostics Command

Developers and automation tools can inspect current storage health without executing any build:
```powershell
.\scripts\build-cache\Get-BuildCacheDiagnostics.ps1
```

### Diagnostic Output Sections:
1. **Drive Capacity Baseline**: Real-time free GiB and status indicator for Drive D: and Drive C:.
2. **Active Build Processes**: Detection of running `cargo.exe`, `rustc.exe`, or `agsd.exe` instances.
3. **Known Worktrees & Target Mappings**: Enumeration of active checkouts, their current branch and commit, their Drive C: target directory size, and their legacy Drive D: target size.
4. **Recorded Provenance Records**: Inventory of verified daemon binaries with SHA-256 summaries.

---

## 3. Real-Time Before/After Build Delta Reporting

Every invocation through `Invoke-CargoWithCache.ps1` records disk metrics prior to launching `cargo` and immediately upon termination:
```text
================================================================================
Initial Disk: D: 37.07 GB free | C: 293.49 GB free
Executing: cargo check -p agent-studios-protocol
Finished in 44.00s with exit code 0
================================================================================
Final Disk:   D: 37.07 GB free (delta: 0 GB) | C: 293.18 GB free (delta: -0.31 GB)
================================================================================
```
This guarantees immediate observability of build artifact growth on Drive C: and proves zero consumption on Drive D:.
