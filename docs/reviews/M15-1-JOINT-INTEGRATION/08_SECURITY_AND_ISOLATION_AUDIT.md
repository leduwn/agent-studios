# Agent Studios — Milestone M15.1 Joint Integration Preflight
## Document 08: Security, Process Containment & Isolation Audit

**Milestone:** M15.1 (Joint Integration Preflight & Cross-Platform Release Gate)  
**Date:** 2026-11-06  
**Role:** Principal Integration Engineer & Security Architect  
**Scope:** Backend Runtimes, IPC Daemons, CLI, and VS Code Extension  
**Status:** ZERO HIGH/CRITICAL VULNERABILITIES — SECURITY ACCEPTED  

---

### 1. Threat Model & Security Posture

Agent Studios operates as an autonomous agent IDE extension, orchestrating background processes, code modifications, and external model interactions. The system security architecture is designed under the principle of **least privilege and fail-closed isolation**:

```
[ VS Code Workbench ]
         │ (Webview CSP + Nonce + Message Validation)
         ▼
[ VS Code Extension Host ]
         │ (Two-Phase HMAC-SHA256 Challenge + Role Separation)
         ▼
[ Native IPC Daemon (agsd) ]
         │ (Job Object Quotas + Loopback 127.0.0.1 + Fail-Closed Platform Gate)
         ▼
[ Sandboxed Child Runtimes (Claude Code / OpenCode / CLI) ]
```

---

### 2. Process Containment & Kernel Sandboxing

#### 2.1 Windows Job Object Containment (`JobObjectGuard`)
- **Quotas Enforced:**
  - Memory limit: 2 GiB per child runtime tree.
  - Process limit: 16 maximum concurrent processes.
  - CPU rate control: Hard-capped execution time.
- **Kill-on-Close Guarantee:**
  - Configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
  - Zero-orphan guarantee: If `agsd` or VS Code crashes, all child compilers, shells, and agent processes are terminated instantly by the Windows kernel.
- **Zero-Window Suspended Spawning:**
  - Processes are created suspended via `CREATE_SUSPENDED`.
  - Added to Job Object before initial thread execution, eliminating race conditions where malicious code could fork before containment is assigned.

#### 2.2 Cross-Platform Fail-Closed Invariant
- On non-Windows platforms (Linux, macOS), if an agent or caller requests `SandboxLevel::Standard` or `SandboxLevel::Strict` without configured OS cgroups/namespaces, the system **fails closed** with `OpenCodeError::UnsupportedPlatform`.
- Under no circumstances does the engine silently downgrade security containment.

---

### 3. Network Isolation & Loopback Binding

- **Loopback Enforcement:**
  - All local HTTP servers (e.g. OpenCode server) and RPC endpoints are strictly bound to `127.0.0.1`.
  - `verify_loopback_url()` validates the hostname before socket binding. Binding to `0.0.0.0` or public interfaces is categorically rejected.
- **Ephemeral Authentication Credentials:**
  - Local HTTP servers generate a 32-byte CSPRNG password on startup.
  - Credentials are transmitted strictly via private process environment variables (`OPENCODE_SERVER_PASSWORD`), never via CLI arguments.

---

### 4. IPC Authentication & Role Separation

- **Two-Phase HMAC-SHA256 Handshake:**
  - Client connects to Named Pipe / Unix Domain Socket.
  - Daemon challenges client with a random nonce.
  - Client signs nonce using shared secret key.
- **Role Separation:**
  - **Operator Role (`role: operator`):** Permitted to inspect state, stream events, and submit tasks. Administrative operations (`shutdown`, binary upgrades) are blocked with `PermissionDenied`.
  - **Admin Role (`role: admin`):** Requires admin credentials; authorized to execute graceful shutdown and maintenance.
- **Frame Validation:**
  - Length-prefixed header bounded at 16 MB. Oversized frames abort connection immediately to prevent memory exhaustion DoS.

---

### 5. Webview & Extension Host Security

- **Strict Content Security Policy (CSP):**
  ```
  default-src 'none';
  img-src vscode-resource: https: data:;
  script-src 'nonce-<random>' 'unsafe-eval';
  style-src vscode-resource: 'unsafe-inline';
  ```
- **Prototype Pollution Defense:**
  - All incoming webview messages are sanitized and validated against explicit message schemas before dispatching.
- **Fail-Closed Command Execution:**
  - If the bridge is disconnected, editor mutations (`applyRefactor`, `generateTests`) fail closed with user warnings rather than operating on stale state.
- **Virtual Document Diff Isolation:**
  - Diffs are rendered via virtual in-memory URI scheme `agent-studios-diff://`. Zero temporary files written to workspace disk.
