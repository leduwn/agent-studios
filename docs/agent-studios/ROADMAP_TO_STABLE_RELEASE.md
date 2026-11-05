# Agent Studios — Master Roadmap: Path to Stable Release (M15 – M19)

> **Status**: Authoritative Master Product & Engineering Roadmap  
> **Milestone Horizon**: M15 Consolidation to M19 Production General Availability (GA)  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Reference**: Replaces and supersedes legacy `ROADMAP.md` planning sections

---

## 1. Master Milestone Timeline & Sequencing

```text
2026-Q4 ─────────────────────────────────────────────────────────────► 2027-Q2
M15 Consolidation Baseline (COMPLETED)
    │
    ▼
Phase P0: Integration PR Merge & Canonical Workspace Migration (IMMEDIATE)
    │
    ▼
Milestone M15.1: Standalone `ags` CLI/TUI & Model Discovery Integration
    │
    ▼
Milestone M16: Full Codex Feature Surface (Skills, MCP, Hooks, AGENTS.md)
    │
    ▼
Milestone M17: Packaging, Desktop Shell Embedding & Windows MSI Installer
    │
    ▼
Milestone M18: Concurrency Stress, Crash Hardening & Security Audit
    │
    ▼
Milestone M19: Production Stable Release (v1.0.0 GA, English + Vietnamese)
```

---

## 2. Milestone Detailed Specifications

### Phase P0: Integration PR & Canonical Workspace Migration (Immediate)
- **Objective**: Establish `D:\Projects\agent-studios` as the single canonical repository root on `main`.
- **Key Deliverables**:
  1. Operator merges Draft PR (`feat(m15): unify backend, runtime and VS Code frontend`) into `main`.
  2. Preserve uncommitted provider discovery files in canonical root onto `feat/provider-discovery-v1.2-wip`.
  3. Checkout and fast-forward canonical root `D:\Projects\agent-studios` to unified HEAD.
  4. Execute operator-approved compiler cache purge (`cargo clean`) recovering ~21.27 GB.
  5. Prune 7 validated `APPROVED_CANDIDATE` worktrees recovering ~2.22 GB.
  6. Decommission temporary staging worktree `D:\Projects\agent-studios-unified-staging`.
- **Quality Gate**: Clean checkout on canonical root; `cargo test --workspace` and `npm test:unit` pass 100%.

---

### Milestone M15.1: Standalone `ags` CLI / TUI & Dynamic Model Discovery
- **Objective**: Deliver the unified `ags` binary combining interactive TUI, dynamic model discovery, and headless scriptable automation.
- **Key Deliverables**:
  1. Consolidate `crates/cli`, `crates/cli-client`, and `crates/tui` into canonical `agent-studios-rs`.
  2. Integrate `ModelDiscoveryClient` (`crates/provider/src/discovery.rs`) supporting dynamic remote model discovery.
  3. Wire dynamic multi-runtime routing:
     - Priority 1: `InternalCodexRuntime` (in-process Rust execution).
     - Priority 2: `ClaudeCodeRuntime` (Anthropic CLI harness).
     - Priority 3: `OpenCodeRuntime` (OpenCode daemon / local models).
  4. Implement `ags run --headless "<goal>"` with newline-delimited JSON (NDJSON) event streaming and exit code contract.
  5. Support piped input execution (`cat prompt.txt | ags run --stdin`).
- **Quality Gate**: CLI unit tests pass; `ags doctor --json` returns valid system health; headless run executes end-to-end task.

---

### Milestone M16: Full Codex Feature Surface Parity
- **Objective**: Achieve complete feature parity with upstream OpenAI Codex and Claude Code extensibility.
- **Key Deliverables**:
  1. **Skills Engine (`SKILL.md`)**:
     - Progressive instruction loading into context windows.
     - Orthogonal scoping: Global (`~/.agent-studios/skills`), Project (`.agent-studios/skills`), and Agent.
     - GitHub repository installation (`ags skill install <url>` or `/skill install`).
  2. **Model Context Protocol (MCP)**:
     - Full MCP client supporting Stdio subprocesses and SSE HTTP transports.
     - Workspace configuration (`.agent-studios/mcp.json`) and Global configuration (`~/.agent-studios/mcp.json`).
     - Interactive approval interception for MCP tool invocations.
  3. **Lifecycle Hooks**:
     - Shell and script hooks for `pre_turn`, `post_task`, and `on_reconciliation`.
  4. **Hierarchical Instructions**:
     - Scoped `AGENTS.md` and `CLAUDE.md` discovery respecting directory traversal boundaries.
- **Quality Gate**: E2E tests validating skill execution, MCP server communication, and hook invocations.

---

### Milestone M17: Packaging, Desktop Shell & Windows Installer
- **Objective**: Package a standalone, out-of-the-box native desktop application and installer.
- **Key Deliverables**:
  1. **Code-OSS Desktop Shell Integration**:
     - Package standalone Code-OSS desktop binary bundling Monaco, LSP, DAP, and the native `agsd` background daemon.
     - Deliver conversation-first Agent Mode startup view with instant zero-loss toggling to IDE Mode.
  2. **Windows 11 x64 Native Installer**:
     - Native MSI / NSIS installer bundling all prerequisites (Node runtime, Git shims, `ags`, `agsd`).
     - Configuration paths:
       - Config: `%APPDATA%\AgentStudios\config.toml`
       - State DB: `%LOCALAPPDATA%\AgentStudios\state.db`
       - Blobs / Artifacts: `%LOCALAPPDATA%\AgentStudios\artifacts\`
  3. **Auto-Updater Pipeline**:
     - Cryptographic signature verification and background delta updates.
  4. **CLI Distribution**:
     - Standalone `ags` CLI package via Winget, Cargo, and NPM.
- **Quality Gate**: Clean Windows 11 VM installation; application launches, connects to daemon, and completes initial goal without manual configuration.

---

### Milestone M18: Concurrency Stress, Security Audit & Reliability
- **Objective**: Harden Agent Studios under intense concurrent multi-agent workloads and edge-case failures.
- **Key Deliverables**:
  1. **Multi-Agent Concurrency Stress**:
     - Benchmark 10+ parallel mutating workers executing simultaneous tasks in isolated worktrees.
     - Validate zero file corruption and deterministic single-writer lease arbitration.
  2. **Long-Running Session Profiling**:
     - Validate 1000+ turns and 100,000+ event envelopes without memory leaks or UI frame drops.
     - Validate SSE backpressure handling and SQLite WAL vacuuming.
  3. **Abnormal Crash & Power Loss Recovery**:
     - Simulate hard daemon kills (`SIGKILL`) during active tool execution.
     - Verify automatic lock reclamation, interrupted task reconciliation to `Failed`, and clean sequence replay upon reboot.
  4. **Independent Security Audit**:
     - Third-party penetration testing of IPC Named Pipes, HMAC handshakes, and secret zeroization.
- **Quality Gate**: Zero data loss, zero memory leaks, and 100% crash recovery across 24-hour continuous stress suites.

---

### Milestone M19: Production Stable Release (v1.0.0 GA)
- **Objective**: Deliver the General Availability (GA) release of Agent Studios.
- **Key Deliverables**:
  1. Complete bilingual localization parity: English and Vietnamese UI strings, documentation, and error messages.
  2. Comprehensive public documentation, API reference, and video tutorials.
  3. Public GitHub Release, release notes, and community contributor guidelines.
  4. Official Tier-1 Platform Support: Windows 11 x64 (Primary), Ubuntu 22.04+ (Secondary), macOS ARM64 (Secondary).
- **Quality Gate**: GA sign-off checklist verified; zero open P0/P1 defects.

---

## 3. Future Proposals Backlog (Post-v1.0)

```text
┌──────┬──────────────────────────────────────────┬──────────┬────────────┐
│ ID   │ Proposal Title                           │ Priority │ Complexity │
├──────┼──────────────────────────────────────────┼──────────┼────────────┤
│ FP01 │ Remote Daemon Hosting (SSH / TLS)        │ P2       │ High       │
│ FP02 │ Ephemeral Cloud MicroVM Sandboxes        │ P2       │ Very High  │
│ FP03 │ Real-time Collaborative Multi-Dev Studio │ P3       │ High       │
│ FP04 │ Native Local Model Acceleration (vLLM)   │ P3       │ Medium     │
└──────┴──────────────────────────────────────────┴──────────┴────────────┘
```

1. **Remote Daemon Hosting (FP01)**: Run `agsd` on a headless Linux server or devcontainer while connecting local Code-OSS over TLS/SSH.
2. **Ephemeral Cloud MicroVM Sandboxes (FP02)**: Run mutating workers in isolated Firecracker microVMs for untrusted repository inspection.
3. **Collaborative Multi-Dev Sessions (FP03)**: Enable team developers to co-pilot, review, and approve agent actions on a shared project session.
4. **Native Local Model Acceleration (FP04)**: Auto-detect local GPUs (NVIDIA/AMD) with direct llama.cpp/vLLM backend routing for zero-cost offline coding.
