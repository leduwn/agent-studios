# M15 UI-04 — Backend Dependency Pinning & Worktree Isolation Strategy

## 1. Pinned Backend Target

- **Authoritative Backend SHA**: `dcbcb39bf61225a4660be5b533cf1ffd5de14b29`
- **Backend Milestone**: M15-04.1 (Durable State Reliability, Crash Recovery & CI Hardening)
- **Backend Branch**: `feat/m15-04-durable-state-recovery` (PR #30)
- **Compiled Daemon Binary**: `D:\Projects\agent-studios-m15-04\agent-studios-rs\target\debug\agsd.exe`
- **Platform Verified**: Windows Server / Windows 11 (`Named Pipes`), Ubuntu 24.04 (`Unix Domain Sockets`)

---

## 2. Worktree Separation Invariant (Zero-Merge Policy)

To preserve code review clarity and maintain clean commit histories:
1. **No Merging**: PR #30 will NOT be merged into PR #27. PR #27 is based on `feat/m14-mission-control-connected-integration`.
2. **Binary Coupling Only**: The frontend consumes the backend as a decoupled external daemon executable over authentic OS IPC.
3. **No Source Edits in Backend**: The UI engineer does not modify any file under `agent-studios-rs/` within the UI worktree. All contract agreements are negotiated via versioned wire framing.

---

## 3. Dynamic Daemon Path Resolution for Test Suites

The test harness in `RealRustDaemonE2E.test.ts` and `RealRustDaemonRecoveryE2E.test.ts` dynamically resolves the pinned daemon using the following hierarchy:

1. `process.env.AGSD_BIN_PATH` (explicit override).
2. Pinned worktree target: `D:\Projects\agent-studios-m15-04\agent-studios-rs\target\debug\agsd(.exe)`.
3. Sibling milestone worktree targets: `agent-studios-m15-03`, `agent-studios-m15-02`.
4. Monorepo local target: `../../agent-studios-rs/target/debug/agsd(.exe)`.

If no binary is found, tests throw an explicit, descriptive setup error rather than failing silently or substituting an in-memory mock.
