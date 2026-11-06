# P0-BUILD-CACHE-STABILIZATION: 01 Disk Baseline & Growth Investigation

**Document ID**: `P0-BCS-01-DISK-BASELINE-AND-CAUSE`  
**Date**: 2026-11-06  
**Status**: VERIFIED & COMPLETE  
**Canonical Baseline Commit**: `8525ec228c37e38c35ee514dc886af295fbc4e00`  
**Feature Branch**: `chore/dev-build-cache-policy`  

---

## 1. Measured Disk Capacity Ledger

Accurate logical disk metrics measured via `Get-CimInstance Win32_LogicalDisk`:

| Milestone / Event | Drive D: Free Bytes | Drive D: Free GiB | Drive D: Free % | Drive C: Free Bytes | Drive C: Free GiB | Drive C: Free % |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Phase A Baseline (Emergency)** | 579,452,928 | 0.54 GiB | 0.49% | 316,500,000,000 | 294.81 GiB | 61.96% |
| **Sum of Deleted Logical Bytes** | 41,117,772,529 | 38.29 GiB | - | - | - | - |
| **Post-Phase B Measured** | 39,904,423,936 | 37.16 GiB | 33.79% | 315,283,570,688 | 293.63 GiB | 61.71% |
| **Current Baseline** | 39,812,000,000 | 37.07 GiB | 33.70% | 314,800,000,000 | 293.18 GiB | 61.61% |

---

## 2. Forensic Analysis of the ~1.67 GiB Delta

The initial audit reported:
- **Starting Free Space**: `579,452,928` bytes (~0.54 GiB)
- **Reported Deleted Logical Bytes**: `41,117,772,529` bytes (~38.29 GiB)
- **Theoretical Free Space**: `41,697,225,457` bytes (~38.83 GiB)
- **Actual Measured Free Space**: `39,904,423,936` bytes (~37.16 GiB)
- **Net Delta**: `1,792,801,521` bytes (~1.67 GiB)

### Root Causes of Delta:
1. **NTFS Cluster Allocation vs Logical File Size (Slack Space)**:
   The deleted Cargo `target/` and `.vscode-test/` directories contained over 180,000 individual compilation artifacts (`.d`, `.fingerprint`, `.rlib`, `.rmeta`, `.json`). Many of these files are smaller than the standard NTFS cluster size (4 KiB). While `Scripting.FileSystemObject` sums raw file lengths, NTFS allocates and reclaims storage in cluster units.
2. **NTFS Master File Table (MFT) & Transaction Logging**:
   NTFS reserves MFT record sectors and records metadata transactions in `$LogFile` and `$UsnJrnl` during massive sequential directory deletions. These metadata blocks are not instantly compacted back into the volume's free bitmap.
3. **Audit Documentation Generation**:
   The Phase A and Phase B documentation packages (`docs/reviews/P0-DISK-RECOVERY-V2/*`) were written to Drive D:, consuming approximately 40 KiB.
4. **No External Rogue Processes**:
   Inspection of `Win32_Process` and filesystem timestamps confirmed that no external rogue build processes, compiler spawns, or background downloaders were active on Drive D:.
