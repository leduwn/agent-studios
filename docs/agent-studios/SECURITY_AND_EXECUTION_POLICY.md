# Agent Studios — Security, Execution Policy & Sandboxing Specification

> **Status**: Authoritative Security & Execution Policy  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Reference**: Implements `MASTER_VISION.md` §12, `PRODUCT_PRINCIPLES.md`, `SECURITY_MODEL.md`, M12.3 Security Audit, and M13-IPC-01/02

---

## 1. Executive Summary & Security Constitution

`[EXISTING_CANONICAL_DECISION]`
Agent Studios is designed from the ground up for safe, contained execution of autonomous coding agents. Because autonomous agents write and execute arbitrary code, invoke shell tools, and inspect project repositories, Agent Studios enforces a multi-layered defense-in-depth security model:

```text
┌────────────────────────────────────────────────────────────────────────┐
│                      LAYER 1: IPC WIRE SECURITY                        │
│   HMAC-SHA256 Handshake │ Anti-Replay Sliding Window │ 4 MiB Clamp     │
├────────────────────────────────────────────────────────────────────────┤
│                      LAYER 2: CAPABILITY MATRIX                        │
│   Role-Based Redaction │ Fail-Closed task.submit Gate │ Challenge Auth │
├────────────────────────────────────────────────────────────────────────┤
│                      LAYER 3: CREDENTIAL SECURITY                      │
│   In-Memory SecretString │ Zero Plaintext Storage │ Boundary Scrubbing │
├────────────────────────────────────────────────────────────────────────┤
│                      LAYER 4: OS-LEVEL SANDBOXING                      │
│   Win32 Job Objects (Windows) │ Process Groups & Signals (Linux/macOS) │
├────────────────────────────────────────────────────────────────────────┤
│                      LAYER 5: WORKSPACE CONTAINMENT                    │
│   Dedicated Git Worktrees │ Ephemeral Indices │ Safe Dirty Retention   │
├────────────────────────────────────────────────────────────────────────┤
│                      LAYER 6: TWO-PHASE COMMIT (2PC)                   │
│   Cryptographic Challenge │ Stdin Delivery Verification │ Hard Timeout │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. OS-Level Process Sandboxing & Containment

`[IMPLEMENTED_BEHAVIOR]`
To ensure that autonomous agent subprocesses can never escape, hang indefinitely, or leave orphaned worker processes:

### 2.1 Windows Subprocess Sandboxing (`crates/claude-code-runtime`)
- **Win32 Job Object**: Every spawned agent process tree is assigned to an isolated Win32 Job Object wrapped in `JobObjectGuard`.
- **`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`**: Enforces that if the host daemon (`agsd`) or the runtime wrapper terminates, crashes, or closes the handle, Windows automatically and synchronously terminates the entire subprocess tree.
- **Cross-Platform Conditional Compilation**: Linux/macOS runners do not support Win32 APIs; `JobObjectGuard` is strictly guarded via `#[cfg(windows)]` to prevent build breakage on Ubuntu runners.

### 2.2 POSIX Process Group Sandboxing
- On Linux and macOS, child processes are assigned to a distinct process group via `setpgid`.
- Termination sends `SIGTERM` followed by `SIGKILL` to the negative process group ID (`-pgid`), killing all spawned child processes.

---

## 3. Two-Phase Commit (2PC) Approval Delivery Protocol

`[IMPLEMENTED_BEHAVIOR]`
Integrated in Milestone M12.3 / PR #15 (`3207b7462f`), Agent Studios enforces an atomic Two-Phase Commit (2PC) protocol for interactive tool approvals:

```text
Host Daemon / UI              claude-code-runtime               Child CLI Process
       │                               │                                │
       │◄─── ActionApprovalRequired ───│                                │
       │     (Challenge Token: C_123)  │                                │
       │                               │                                │
[Operator Approves]                    │                                │
       │─── ResolveApproval(C_123) ───►│                                │
       │                               │─── Phase 1: Verify Challenge ──┤
       │                               │    (Match token & unexpired)   │
       │                               │                                │
       │                               │─── Phase 2: Flush to Stdin ───►│
       │                               │    [Write "y\n" to process]    │
       │                               │                                │
       │                               │─── Delivery Success Check ─────┤
       │                               │    [If Write/Flush Fails]:     │
       │                               │    FAIL CLOSED!                │
       │                               │    Terminate Process Tree!     │
```

- **Fail-Closed Stdin Delivery**: If child process stdin cannot accept the approval token (e.g. pipe closed, buffer full, process aborted), the runtime aborts immediately and terminates the process tree rather than assuming approval succeeded.
- **Zero-Duration Timeout Protection**: Prevents watchdog self-abort races by verifying non-zero execution timeout floors.

---

## 4. Native IPC Wire Security & Protocol Foundation

`[IMPLEMENTED_BEHAVIOR]`
Connecting Code-OSS or the `ags` CLI to `agsd` requires passing strict cryptographic validation:

### 4.1 Length-Prefixed Framing
- Every message is framed with a 4-byte big-endian unsigned integer representing payload length.
- Maximum frame size is clamped to **4 MiB (4,194,304 bytes)** to prevent denial-of-service memory exhaustion.

### 4.2 HMAC-SHA256 Authentication
- Client and daemon execute a mutual challenge-response handshake upon connection.
- Handshake envelopes include 32-byte cryptographically secure random nonces.
- Timestamp drift window: Discards packets with timestamps deviating by more than 30 seconds from server clock.
- Sliding anti-replay window: Tracks recently observed nonce hashes, rejecting replay attacks.

### 4.3 Privilege Separation
- **Standard Clients** (`agsd.json`): Read-only access to tasks, agents, and event streams. Mutations rejected.
- **Admin Clients** (`agsd-admin.json`): Authorized to invoke daemon shutdown, state eviction, and policy overrides.

---

## 5. Secret Scrubbing & In-Memory Zeroization

`[EXISTING_CANONICAL_DECISION]` & `[IMPLEMENTED_BEHAVIOR]`
Agent Studios guarantees **Zero Plaintext Secret Leakage**:

1. **In-Memory Zeroization**: API keys, bearer tokens, and private credentials use zeroizing types (`SecretString`). Memory is actively scrubbed with zeros upon `Drop`.
2. **Dynamic Resolution**: Models and tools reference secrets via abstract `SecretReference` identifiers. Secrets are resolved into memory only at the precise millisecond of wire transmission.
3. **Regex-Free Boundary Scrubbing**: Before any text is emitted to event logs, telemetry streams, diagnostic channels, or terminal boxes, token boundary scrubbers strip potential secret formats (sk-, key-, token-, bearer tokens, and query strings).

---

## 6. Workspace Containment & Safe Git Operations

`[EXISTING_CANONICAL_DECISION]` & `[IMPLEMENTED_BEHAVIOR]`
### 6.1 Ephemeral Index Isolation
- Mutating tasks execute in dedicated Git worktrees on temporary branches.
- File changes are captured using an isolated index (`GIT_INDEX_FILE`). The operator's main working tree index is never polluted or modified during execution.

### 6.2 Destructive Cleanup Prohibition
Standard automated lifecycle cleanup is **strictly forbidden** from executing:
1. `git reset --hard`
2. `git clean -fdx`
3. `git worktree remove --force`
Conflicted, failed, or dirty worktrees are safely retained under `.git/agent-studios/worktrees/` for operator recovery.

---

## 7. The Zero Billable Invariant ($0.00 Token Gate)

`[USER_DIRECT_REQUIREMENT]` & `[IMPLEMENTED_BEHAVIOR]`
To protect operators from unexpected commercial API expenditures:
- `task.submit` is disabled by default in `CapabilityMatrix`.
- All automated unit, integration, and cross-process E2E test suites operate against mock adapters or read-only non-billable CLI metadata commands (`claude --version`).
- Total billable token cost across all CI runs and milestone test suites is verified at **$0.00**.
- Enabling live AI cloud execution requires explicit operator challenge validation and budget ceiling authorization.
