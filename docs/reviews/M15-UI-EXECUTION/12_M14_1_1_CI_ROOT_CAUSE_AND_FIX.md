# M14.1.1 Root Cause Analysis & Cross-Platform Path Normalization Fix

**Document ID**: `M15-UI-EXEC-DOC-12`  
**Milestone**: M14.1.1 Final Cross-Platform & Authorization Corrections  
**Author**: Principal VS Code Extension Engineer, Cross-Platform QA Lead  
**Date**: 2026-10-30  
**PR Reference**: PR #25 (`feat/m14-mission-control-connected-integration`)  
**Verified Baseline HEAD**: `ee76e0fc133a991115017873f17242b19512d5d8`  
**Failed CI Run**: [GitHub Actions Run 41067831592](https://github.com/leduwn/agent-studios/actions/runs/41067831592) (Job: `Ubuntu VS Code Extension Suite`)

---

## 1. Executive Summary & Defect Overview

During the M14 freeze verification, remote GitHub Actions CI succeeded on Windows (`windows-latest`) but failed on Ubuntu (`ubuntu-latest`) in run `41067831592`. The failure anchored strictly in `apps/agent-studios-vscode/src/test/unit/TaskSubmissionSafety.test.ts` on test case:

```text
M14.1 Task Submission Safety & Fail-Closed Regression Tests
  4. Workspace Mismatch & Boundary Enforcement
    ✕ allows submission when workspace directory matches daemon repository root across path variations
```

The failure was caused by platform-dependent filesystem semantics: hardcoded Windows drive letters (`D:/Projects/agent-studios-ide-ui`) and arbitrary backslash conversions (`replace(/\//g, '\\')`) in POSIX environments, where backslash (`\`) is a valid filename character rather than a path separator, and Windows drive letters are parsed as relative paths.

This document details the root cause, the cross-platform architectural fix in `normalizeWorkspacePath` and `TaskSubmissionSafety.test.ts`, and verification proving that path traversal, symlink resolution, and cross-repository boundaries remain strictly enforced.

---

## 2. Root Cause Analysis (GitHub Actions Run 41067831592)

### 2.1 The Failing Test Implementation in `ee76e0fc13`

In commit `ee76e0fc13`, `TaskSubmissionSafety.test.ts` lines 507 and 682 contained:

```typescript
// TaskSubmissionSafety.test.ts (Lines 507 & 682)
describe('4. Workspace Mismatch & Boundary Enforcement', () => {
  const daemonRoot = path.resolve('D:/Projects/agent-studios-ide-ui');

  it('allows submission when workspace directory matches daemon repository root across path variations', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.submit', true);

    // Normalization: test alternate slash separators and casing
    const wsPathAlternate = daemonRoot.replace(/\//g, '\\');
    const prompt = 'Clean build cache';
    const challenge = OperatorConfirmationManager.createChallenge(
      validStudioId,
      prompt,
      { workspacePath: wsPathAlternate }
    );
    ...
```

### 2.2 POSIX vs Windows Path Discrepancy

1. **Drive Letter Semantics**:
   - On Windows (`win32`), `D:/Projects/...` is an absolute path anchored at drive `D:`.
   - On POSIX (`linux`/`darwin`), drive letters do not exist. `D:/Projects/...` is treated by POSIX `path.resolve` as a **relative path** from the current working directory, resolving to `/home/runner/work/agent-studios/agent-studios/D:/Projects/...`.
2. **Backslash Character Semantics**:
   - On Windows, `\` is the native directory separator.
   - On POSIX, `\` is **NOT** a directory separator; it is a legal character in file and directory names.
   - When `daemonRoot.replace(/\//g, '\\')` was executed on Ubuntu, POSIX `path.resolve` treated the entire string containing backslashes as a single relative filename in the working directory rather than directory segments.
3. **Casing Normalization**:
   - Windows filesystems (NTFS/FAT32) are case-insensitive by default.
   - Linux filesystems (ext4/btrfs) are strictly case-sensitive. Lowercasing paths indiscriminately or expecting case-insensitive equivalence on Linux violates POSIX filesystem integrity.

---

## 3. The Cross-Platform Normalization Architecture

The solution implements strict, platform-aware path canonicalization in `normalizeWorkspacePath` inside `apps/agent-studios-vscode/src/bridge/ipc/CapabilityMatrix.ts`.

### 3.1 `normalizeWorkspacePath` Specification

```typescript
/**
 * Normalizes workspace and repository paths for cross-platform comparison.
 * - Handles null/empty safely.
 * - Rejects foreign-platform Windows drive paths and UNC paths on POSIX.
 * - Resolves symlinks canonically via realpathSync when path exists on disk.
 * - Strips trailing slashes.
 * - Applies case-insensitive lowercasing strictly on Windows (win32).
 */
export function normalizeWorkspacePath(p: string): string {
  if (!p || typeof p !== 'string') {
    return '';
  }
  const trimmed = p.trim();
  if (trimmed.length === 0) {
    return '';
  }

  // Reject foreign-platform Windows drive letters and UNC paths on POSIX
  if (process.platform !== 'win32') {
    if (/^[a-zA-Z]:[/\\]/.test(trimmed) || trimmed.startsWith('\\\\')) {
      throw new Error(
        `Foreign-platform Windows path '${trimmed}' cannot be safely interpreted on POSIX platform.`
      );
    }
  }

  // Canonicalize via path.resolve
  let resolved = path.resolve(trimmed);

  // If path exists on disk, resolve symlinks canonically
  try {
    if (fs.existsSync(resolved)) {
      resolved = fs.realpathSync(resolved);
    }
  } catch {
    // If not accessible or does not exist on disk, keep path.resolve result
  }

  // Strip trailing slashes
  resolved = resolved.replace(/[/\\]+$/, '');

  // Case-insensitive comparison strictly on Windows
  return process.platform === 'win32' ? resolved.toLowerCase() : resolved;
}
```

### 3.2 Key Invariants of the Fix

1. **Foreign-Platform Path Rejection**:
   Attempting to pass a Windows drive path (`C:\...`) or UNC path (`\\server\share`) to a POSIX daemon is an uninterpretable foreign path format. It is immediately rejected with `Error: Foreign-platform Windows path cannot be safely interpreted on POSIX platform` rather than guessing or performing lossy path string surgery.
2. **Canonical Symlink Resolution**:
   `fs.realpathSync` resolves filesystem symlinks (e.g. `/tmp` -> `/private/tmp` or `/var/run` -> `/run`) to their physical storage path, preventing false-positive workspace mismatches when symbolic aliases are used.
3. **Trailing Slash Invariance**:
   Trailing slashes and backslashes are stripped uniformly across all platforms (`resolved.replace(/[/\\]+$/, '')`).
4. **Conditional Case Sensitivity**:
   Lowercasing is applied **exclusively on `win32`** (`process.platform === 'win32' ? resolved.toLowerCase() : resolved`). POSIX platforms retain exact byte casing.

---

## 4. Test Suite Fixture Refactoring

`TaskSubmissionSafety.test.ts` was refactored to replace static Windows paths with OS-native temporary directory fixtures and separate platform-specific assertions:

### 4.1 Native Temporary Directory Fixtures

```typescript
describe('4. Workspace Mismatch & Boundary Enforcement', () => {
  let tempDaemonDir: string;
  let tempForeignDir: string;
  let daemonRoot: string;
  let foreignWorkspace: string;

  before(() => {
    tempDaemonDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ags-daemon-'));
    tempForeignDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ags-foreign-'));
    daemonRoot = fs.realpathSync(tempDaemonDir);
    foreignWorkspace = fs.realpathSync(tempForeignDir);
  });

  after(() => {
    try {
      fs.rmSync(tempDaemonDir, { recursive: true, force: true });
      fs.rmSync(tempForeignDir, { recursive: true, force: true });
    } catch {}
  });
```

### 4.2 Cross-Platform Positive Equivalence

```typescript
it('allows submission when workspace directory matches daemon repository root across trailing slashes', async () => {
  CapabilityMatrix.setCapabilityEnabled('task.submit', true);

  const wsPathWithSlash = `${daemonRoot}${path.sep}`;
  const prompt = 'Clean build cache';
  const challenge = OperatorConfirmationManager.createChallenge(
    validStudioId,
    prompt,
    { workspacePath: wsPathWithSlash }
  );

  const req: SubmitTaskRequest = {
    studioId: validStudioId,
    prompt,
    confirmedByUser: true,
    confirmationChallenge: challenge,
    daemonRepoRoot: daemonRoot,
    workspacePath: wsPathWithSlash,
  };

  const result = await SubmitTaskAdapter.submitTask(transport, req);
  assert.strictEqual(result.success, true);
  assert.strictEqual(transport.sentCommands.length, 1);
});
```

### 4.3 Platform-Specific Branching

- **Windows-Specific Tests (`process.platform === 'win32'`)**:
  - Alternate forward slashes: `daemonRoot.replace(/\\/g, '/')`
  - Drive letter casing variations: `daemonRoot.toUpperCase()`
- **POSIX-Specific Tests (`process.platform !== 'win32'`)**:
  - Symlink canonicalization: `fs.symlinkSync(daemonRoot, symlinkPath)` verified to match `daemonRoot`
  - Foreign Windows path rejection: `C:\Projects\agent-studios` rejected with `Foreign-platform Windows path`

---

## 5. Security Boundary Verification

The path normalization changes preserve all security boundaries:
1. **Path Traversal Rejection**: Paths containing `..` sequences are rejected before normalization (`/(?:^|[/\\])\.\.(?:[/\\]|$)/`).
2. **Null Byte Rejection**: Paths containing `\0` are rejected immediately.
3. **UNC Path Rejection**: Windows UNC paths (`\\server\share`) are rejected.
4. **Cross-Repository Isolation**: Submissions for foreign workspaces (`foreignWorkspace !== daemonRoot`) fail closed with `Workspace mismatch`.
