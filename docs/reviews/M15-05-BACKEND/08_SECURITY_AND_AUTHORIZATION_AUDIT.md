# 08 — Security & Authorization Audit

## 1. Authentication & Session Security

- **Cryptographic Protocol**: Every client connection is authenticated using HMAC-SHA256 challenge-response framing over the local transport (Unix domain socket or Windows Named Pipe).
- **Key & Secret Isolation**:
  - Auth credentials (`auth_keys`, secrets) are loaded strictly from environment variables or secure daemon bootstrap parameters.
  - Auth secrets are **NEVER** persisted into `agsd.db` or serialized into domain event payloads.
- **Session Freshness**:
  - After a daemon restart or crash, all prior active transport sessions are invalidated.
  - Clients must execute a fresh HMAC handshake; session tokens and authorization states are non-reusable across process lifecycles.
- **Single-Writer Lock Protection**:
  - Daemon startup verifies single-writer mutual exclusion via OS-level advisory locking (`agsd.lock`).
  - Path traversal protections prevent lockfile path escape beyond the configured daemon state directory.

---

## 2. Role-Based Access Control (RBAC) & Redaction

Agent Studios enforces strict role segregation across authenticated sessions:
- **`Role::Standard`**:
  - May create studios, register tasks, submit runs within authorized studios, and subscribe to events.
  - **Redacted Information**:
    - `GetRecoveryStatus`: Host `database_path` is redacted (`None`) to prevent filesystem disclosure and reconnaissance.
    - Diagnostics and internal crash dump traces are stripped.
  - **Restricted Endpoints**:
    - `Shutdown`: Explicitly rejected with `PermissionDenied`.
    - Cross-session cancellation of tasks initiated by other sessions: Rejected with `PermissionDenied`.
    - Approval resolution for unassigned operator approvals: Rejected.
- **`Role::Admin`**:
  - Authorized to receive diagnostic information (`database_path: Some(...)`).
  - Authorized to issue `Shutdown` commands.
  - Authorized to resolve administrative approvals and perform emergency task cancellations.

---

## 3. Worktree & Filesystem Containment Policies

- **Traversal Neutralization**:
  - All worktree paths and artifact paths are sanitized via canonicalization checks.
  - Any path containing `..`, null bytes, or attempting to escape the configured workspace root is rejected with `InvalidPath`.
- **Preservation of Partial Worktrees**:
  - During daemon crash recovery or reconciliation, existing worktree directories are **NEVER** deleted automatically.
  - Partial changes and uncommitted developer files are preserved in place for manual inspection.
- **Foreign Worktree Isolation**:
  - If a worktree directory on disk does not match the owning studio's registered metadata, it is marked as `worktrees_foreign` in the recovery status report and left unmodified.

---

## 4. Production Execution Gate (Tier D Zero-Billable Invariant)

- **AI Token Gating**:
  - In automated CI and default local test suites, Tier D commercial AI calls are completely blocked.
  - Task submission endpoints default to fail-closed (`AutoExecutionFailsClosed`) unless an authorized runtime adapter is explicitly wired by the operator.
  - No real API keys are committed or referenced in test fixtures.
