# Milestone M15 UI-01.2 Review: Workspace Authorization & Containment Audit

**Milestone**: M15 UI-01.2 — Real Rust Execution Integration & Final Safety Gates  
**Area**: Workspace Containment, Path Validation & Fail-Closed Status Classification  
**Verdict**: APPROVED  

---

## 1. Executive Summary

In Milestone M15 UI-01.2, client-local workspace paths are no longer blindly trusted or assigned to `session.workspacePath`. Instead, workspace containment is verified authoritatively against daemon-declared `authorized_workspaces` and `defaults.default_repo_path` retrieved via authenticated `GetExecutionReadiness`.

A dedicated `WorkspaceVerification` module classifies workspace state into four fail-closed categories: `AUTHORIZED`, `MISMATCH`, `UNVERIFIED`, and `UNAVAILABLE`.

---

## 2. Classification Architecture

```typescript
export type WorkspaceStatusClassification =
  | 'AUTHORIZED'
  | 'MISMATCH'
  | 'UNVERIFIED'
  | 'UNAVAILABLE';
```

### 2.1 State Evaluation Rules

1. **`UNAVAILABLE`**:
   - Readiness report is `null` or `undefined` (daemon uninitialized or unreachable).
   - Local workspace folder is missing, empty, or whitespace only.
2. **`UNVERIFIED`**:
   - Daemon readiness declares an empty `authorized_workspaces` array and `default_repo_path` is `null`.
   - Cannot establish an authoritative repository root.
3. **`AUTHORIZED`**:
   - Exact path equality between normalized local workspace path and an authorized root.
   - Hierarchical subpath containment within an authorized root (when `strictRoots` is disabled).
4. **`MISMATCH` (Fail-Closed Rejection)**:
   - Local workspace path is outside all authorized roots.
   - Subdirectory opened when `strictRoots: true`.
   - Security violation detected (null bytes `\0`, UNC network paths `\\` / `//`, path traversal `..`).

### 2.2 Security Hardening & Attack Vector Defense

| Attack Vector | Defense Mechanism | Test Verification |
|---------------|-------------------|-------------------|
| Null Byte Injection (`path\0evil`) | Rejected before FS call; classified as `MISMATCH` | `fails closed with MISMATCH on null byte injection` |
| UNC Network Path (`\\server\share`) | Leading double slashes rejected; classified as `MISMATCH` | `fails closed with MISMATCH on UNC network path injection` |
| Path Traversal (`/repo/../escape`) | Raw segment `..` check before normalization | `fails closed with MISMATCH on path traversal (..)` |
| Gitlink Submodule/Worktree | Detects both standard `.git` directory and `.git` gitlink regular file | `detects .git gitlink file (submodule / worktree) as git metadata` |

---

## 3. Webview UI Integration

In `ChatHtmlBuilder.ts` and `ChatWebviewViewProvider.ts`:
- Dual badge classes (`workspace-badge--authorized workspace-badge--matched` vs `workspace-badge--mismatch workspace-badge--mismatched`) provide backward and forward style compatibility.
- Workspace mismatch disables the **Start Task** execution button and displays an explicit, actionable error banner: `Workspace mismatch (${wsVerification.status}): ${wsVerification.reason}`.
