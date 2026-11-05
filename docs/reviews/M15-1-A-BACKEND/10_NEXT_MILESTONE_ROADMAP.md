# Next Milestone Architecture Roadmap (M15.1-B & M15.2)
**Current Milestone Completed:** M15.1-A — Backend and CLI Consolidation  
**Next Milestones:** M15.1-B (Runtime & Orchestration) & M15.2 (Full IDE & UI Integration)  
**Date:** 2026-11-06  
**Auditor:** Claude 1 (Principal Rust Architect & Systems Lead)

---

## 1. Roadmap Overview

With Milestone M15.1-A delivering the unified release gate, reconciled Provider Discovery v1.2, and the official `ags` developer CLI and TUI foundation, the path forward branches into two sequential execution phases:

```text
[M15.1-A: Backend & CLI] ---> [M15.1-B: Runtime & Orchestration] ---> [M15.2: IDE & UI Integration]
  (COMPLETE)                    - Multi-agent DAG execution            - VS Code extension IPC binding
  - Provider Discovery v1.2     - Claude Code & OpenCode drivers      - Mission Control cockpit
  - ags CLI & TUI ported        - Budget & cancellation engine         - Live diff & reconciliation UI
  - Release gate verified       - SQLite event backfill                - End-to-end user workflows
```

---

## 2. Milestone M15.1-B — Runtime & Orchestration Refinement

**Target Timeline:** Next Sprint  
**Lead:** Backend Implementation Team (Claude 1)

### Core Workstreams:
1. **Dynamic Task DAG Execution Engine (`crates/orchestration`)**:
   - Linearizable dependency resolution for complex task graphs.
   - Dynamic spawn of child agents (Coordinator -> Specialist -> Subagent) with isolated worktree assignments.
   - Failure cascade and retry policies with exponential backoff.
2. **External Runtime Driver Hardening (`crates/claude-code-runtime`, `crates/opencode-runtime`)**:
   - Robust PTY / headless session control for external CLI tools.
   - Structured JSON stream parsers for Claude Code tool invocation logs.
   - Fail-closed execution policies enforcing sandboxed file access.
3. **Budget & Resource Governance**:
   - Per-agent budget limits: turn count, tool calls, wall-clock duration, child agent depth.
   - Automatic emission of `BudgetUsageUpdated` and `BudgetExceeded` events.
   - Graceful task checkpointing when limits are approached.
4. **Daemon Persistence & Event Backfill (`crates/ipc-transport`, `crates/control-plane`)**:
   - Resilient IPC reconnection with sequence-based catchup (`since_sequence`).
   - High-throughput broadcast ring buffers preventing slow clients from blocking the core engine.

---

## 3. Milestone M15.2 — Full IDE & UI Integration

**Target Timeline:** Following M15.1-B  
**Lead:** Frontend & VS Code Lead (Claude 2) supported by Claude 1

### Core Workstreams:
1. **VS Code Native Extension (`apps/agent-studios-vscode`)**:
   - Activation lifecycle managing background `agsd` daemon process.
   - Real-time agent status tree view and task timeline panel.
   - In-editor diff review using VS Code's native `vscode.diff` provider connected to `WorktreeChangeCaptured` artifacts.
2. **Dual-Key Approval UX**:
   - In-editor notifications and modals for tool executions and branch reconciliations.
   - One-click approval, rejection, or feedback submission routed through IPC `ResolveApproval`.
3. **Mission Control Web Cockpit**:
   - High-density observability dashboard for multi-studio monitoring.
   - Live token consumption and cost estimation telemetry.
   - Visual task DAG interactive inspector.

---

## 4. Risk Matrix & Technical Debt Ledger

| Risk / Debt Item | Severity | Likelihood | Impact | Mitigation Strategy |
| :--- | :--- | :--- | :--- | :--- |
| **Windows Named Pipe Permissions** | Medium | Low | IPC connect failure in non-elevated shells | Validate Security Descriptor in `windows/named_pipe.rs`; fallback to loopback TCP if IPC pipe blocked. |
| **External CLI Breaking Output Changes** | High | Medium | Runtime driver parsing errors on tool stdout | Implement version handshake and strict fallback to raw stream capture. |
| **Worktree Disk Saturation** | Medium | Medium | Disk space exhaustion during deep agent fan-out | Enforce active worktree quotas (max 5 active per studio); automatic pruning on task completion. |
| **Cross-Platform PTY Discrepancies** | Medium | High | Output formatting discrepancies on Windows conpty vs Linux pty | Use `portable-pty` abstraction with standardized VT/ANSI stripping in non-interactive mode. |
| **Event Stream Broadcast Lag** | Low | Low | Slow frontend clients dropping events | Sequence-numbered watermark recovery buffer in `IpcClient` event stream. |
