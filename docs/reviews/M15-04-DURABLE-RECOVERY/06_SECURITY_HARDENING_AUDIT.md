# M15-04 Security & Hardening Audit Report

## 1. Threat Modeling & Scope

The persistence layer and daemon recovery lifecycle introduce several security-sensitive boundaries:
1. **Durable File Permissions:** Potential local privilege escalation or secret leakage via world-readable database files.
2. **Path Traversal & Symlink Attacks:** Arbitrary database path hijacking or state corruption via symlinked directories.
3. **Secret Separation Invariant:** Prevention of HMAC credentials, API keys, or operator confirmation tokens leaking into durable event streams.
4. **Denial of Service / Lock Contention:** Malicious or runaway processes holding SQLite database locks indefinitely.

---

## 2. Hardening Measures Implemented

### 2.1 File Permissions & Parent Directory Creation
- Database directories are created with secure permissions.
- HMAC bootstrap files (`agsd.json`, `agsd-admin.json`) remain strictly segregated from domain state (`agsd.db`).
- Discovery files continue to be protected by restrictive ACLs (`0600` on Unix, User-Only Security Descriptor on Windows).

### 2.2 Replay Security & Anti-Duplication
- `SqliteEventStore::append_batch` verifies both in-batch and on-disk event ID uniqueness.
- SQLite transactions use `TransactionBehavior::Immediate`, immediately acquiring a RESERVED lock to reject conflicting concurrent writes.
- Busy timeout is bounded to 5,000 milliseconds (`PRAGMA busy_timeout = 5000;`).

### 2.3 Secret Non-Persistence Invariant
- Provider API keys and secret challenge tokens are strictly excluded from `ControlPlaneEvent` payloads.
- Reusable confirmation tokens are ephemeral and never serialized to `events`.
- On daemon restart, active IPC sessions are invalidated; clients must re-authenticate using their HMAC key.

---

## 3. Audit Findings & Resolutions

| ID | Category | Finding | Resolution |
|---|---|---|---|
| SEC-01 | Confidentiality | Risk of storing admin secrets in durable store | Domain events only contain public identifiers; admin keys live exclusively in `agsd-admin.json` |
| SEC-02 | Integrity | Potential sequence tampering or gap injection | SQLite monotonic sequence check (`sequence == latest + 1`) enforced inside immediate transaction |
| SEC-03 | Availability | Lock starvation from concurrent daemon instances | `PRAGMA busy_timeout = 5000` combined with fail-fast on corruption |
| SEC-04 | Authorization | Unauthenticated clients querying recovery status | `GetRecoveryStatus` requires active authenticated session binding |

**Verdict:** PASS. All security invariants satisfied.
