# M15 UI-01: Workspace Verification & Containment

**Document ID**: `M15-UI-01-DOC-04`  
**Milestone**: M15 UI-01 Interactive Agent Execution Workbench  
**Author**: Principal VS Code Extension Engineer  
**Date**: 2026-10-31  
**Status**: Completed & Verified  

---

## 1. Containment Principles

To prevent directory traversal, cross-repository contamination, and unintentional mutations to external projects, the workbench enforces **strict workspace containment verification**:

1. **Pre-Execution Equality Check**: Before any mutating task execution can be confirmed or submitted, the local VS Code workspace root (`vscode.workspace.workspaceFolders[0].uri.fsPath`) is compared against the daemon session root (`session.workspacePath`).
2. **Fail-Closed Mismatch Enforcement**: If the local workspace root and the daemon session root do not match canonically:
   - The workbench toolbar displays an assertive warning badge (`⚠ Mismatch`).
   - The UI displays the mismatch reason.
   - The "Start Task" button is disabled.
   - Any message payload attempting to bypass the UI and invoke `submitTask` is rejected immediately with an explanatory error.
3. **Cross-Platform Canonical Normalization**: Paths are canonicalized via `normalizeWorkspacePath` to avoid false mismatches caused by slashes, casing differences on Windows, or unresolved symlinks.

---

## 2. Normalization Logic (`normalizeWorkspacePath`)

Located in `apps/agent-studios-vscode/src/bridge/ipc/CapabilityMatrix.ts`:

```typescript
export function normalizeWorkspacePath(p: string): string {
  if (!p || typeof p !== 'string') {
    return '';
  }
  const trimmed = p.trim();
  if (trimmed.length === 0) {
    return '';
  }

  // Reject foreign-platform Windows drive letters or UNC paths on POSIX
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
    // If not accessible on disk, keep path.resolve result
  }

  // Strip trailing slashes
  resolved = resolved.replace(/[/\\]+$/, '');

  // Case-insensitive comparison on Windows
  return process.platform === 'win32' ? resolved.toLowerCase() : resolved;
}
```

---

## 3. UI Status Indication

In `ChatHtmlBuilder.ts`:
- **Verified Match**:
  ```html
  <div id="workspaceBadge" class="workspace-badge workspace-badge--matched" title="Workspace verified against daemon root: /projects/agent-studios">
    <span>✓ Verified</span>
    <span id="workspacePathDisplay">/projects/agent-studios</span>
  </div>
  ```
- **Mismatch**:
  ```html
  <div id="workspaceBadge" class="workspace-badge workspace-badge--mismatched" title="Daemon root is bound to /repo/a but VS Code is in /repo/b">
    <span>⚠ Mismatch</span>
    <span id="workspacePathDisplay">/repo/b</span>
    <span class="workspace-mismatch-detail">(Daemon root is bound to /repo/a but VS Code is in /repo/b)</span>
  </div>
  ```
