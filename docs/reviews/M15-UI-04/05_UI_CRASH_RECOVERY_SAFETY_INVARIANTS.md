# M15 UI-04 — UI Crash Recovery Safety Invariants

## 1. Safety Invariants & Negative Tests

1. **Zero Auto-Resumption of Interrupted Tasks**:
   - When the extension reconnects to a restarted daemon, it MUST NOT auto-trigger re-execution or dispatch mutation commands.
   - Any interrupted task remains in `failed` state until an operator explicitly initiates a new task batch.

2. **Accurate Task Status Projection (No Stale RUNNING Display)**:
   - The UI components (`TaskGraphWebview`, `TasksTreeProvider`, `SessionTreeProvider`, `AttentionQueue`) must accurately reflect the reconciled state `failed` with reason `"daemon_interrupted"`.
   - Never project an interrupted task as active or pending completion.

3. **Least-Privilege Path Redaction**:
   - The UI views rendered for standard users must never expose host database filesystem paths (`database_path`).
   - Diagnostic displays for standard clients render "Database: Protected (Active WAL)".

4. **Strictly Read-Only Boundary Preservation**:
   - The UI workbench remains strictly read-only for reviewing tasks, diffs, artifacts, and recovery reports.
   - Mutation capabilities (`task.submit`, patch application, shell execution) remain gated behind operator authorization.
