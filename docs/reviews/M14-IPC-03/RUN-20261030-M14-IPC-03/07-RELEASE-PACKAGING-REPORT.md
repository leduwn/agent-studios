# Release Packaging Report: M14-IPC-03

**Milestone:** M14-IPC-03  
**Run ID:** RUN-20261030-M14-IPC-03  
**Date:** 2026-10-30  

---

## 1. Packaging Summary

Production packaging was verified using the official VS Code Extension packaging toolchain (`@vscode/vsce`).

```bash
npm run package
# Executes: vsce package --no-dependencies
```

### Key Artifact Metrics:
- **Output Artifact:** `agent-studios-vscode-0.1.0.vsix`
- **File Count:** 51 files
- **Total Compressed Size:** 143.54 KB
- **Packaging Verdict:** **SUCCESS**

---

## 2. Included File Breakdown

```
agent-studios-vscode-0.1.0.vsix
├─ [Content_Types].xml
├─ extension.vsixmanifest
└─ extension/
   ├─ LICENSE.txt [10.87 KB]
   ├─ package.json [7.54 KB]
   ├─ readme.md [6.81 KB]
   ├─ media/
   │  └─ agents-activitybar.svg [0.46 KB]
   └─ out/
      ├─ extension.js [10.38 KB]
      ├─ bridge/ (16 files) [120.14 KB]
      ├─ cards/ (4 files) [41.30 KB]
      ├─ chat/ (5 files) [97.48 KB]
      ├─ commands/ (3 files) [22.98 KB]
      ├─ composer/ (2 files) [30.49 KB]
      ├─ controls/ (2 files) [24.93 KB]
      ├─ state/ (1 file) [16.12 KB]
      ├─ views/ (7 files) [48.52 KB]
      └─ webviews/ (4 files) [82.03 KB]
```

---

## 3. Hygiene & Exclusion Checks

- **Zero Test Leakage:** `out/test/` directory is strictly excluded by `.vscodeignore`.
- **Zero Node Modules:** Production dependencies are zero, preventing binary bloat and native dependency mismatches.
- **Zero Uncompiled TypeScript:** Only compiled `.js` and required media assets are included.
- **Strict License Compliance:** Apache-2.0 license file included at package root.
