# Milestone M12 — Final Handoff & Acceptance Report

- **Milestone**: `M12 — Claude Code CLI Runtime Adapter & Subagent Event Correlation`
- **Execution Run**: `RUN-20261027-M12-claude-code`
- **Date**: `2026-10-27T23:45:00+07:00`
- **Role**: Principal Technical Lead & Integration Owner
- **Target Repository**: `https://github.com/leduwn/agent-studios`
- **Branch**: `feat/m12-claude-code-runtime`
- **Base Commit**: `dc363d1e89fe8751c001f9829bd2cd0b9f7d37f9` (PR #14 merged)
- **Status**: **COMPLETE & ACCEPTED**

---

## 1. Acceptance Checklist & Verification Status

| Milestone Requirement | Verification Method | Status |
|---|---|:---:|
| **Vendor-Neutral Runtime Adapter** | Implemented `AgentRuntime` trait in `agent-studios-claude-code-runtime` | **PASSED** |
| **Bidirectional NDJSON Streaming** | Parsed stdout lines via `NdjsonStreamReader` (16 MiB line limit) | **PASSED** |
| **Native Subagent Event Correlation** | `SubagentCorrelationTracker` tracks DAG with `parent_tool_use_id` | **PASSED** |
| **Host Delegated Tool Approvals** | Formatted `ControlRequest`/`ControlResponse`, fail-closed timeouts | **PASSED** |
| **Budget Enforcement & Redaction** | `--max-budget-usd` validation, canary secret scrubbing regexes | **PASSED** |
| **Windows OS Process Containment** | Win32 Job Objects with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` | **PASSED** |
| **Truthful Security Boundaries** | `SandboxLevel::Strict` fails closed on Windows MSVC | **PASSED** |
| **Comprehensive Test Suite** | 29 passing tests across 4 test suites, 0 failures | **PASSED** |
| **Clippy & Compiler Strictness** | 0 errors, 0 warnings under `-D warnings` | **PASSED** |
| **Upstream IP & License Quarantine** | Zero code copied from `777genius/agent-teams-ai` (AGPL-3.0) | **PASSED** |
| **`codex-rs/` Immutability** | Zero lines modified in `codex-rs/` (verified via git diff) | **PASSED** |
| **Upstream Intelligence Automation** | GitHub Actions workflow `.github/workflows/upstream-intelligence.yml` | **PASSED** |
| **Milestone Documentation** | All 6 audit documents complete under `docs/reviews/M12/RUN-20261027-M12-claude-code/` | **PASSED** |

---

## 2. Operation & Configuration Guide

### 2.1 Discovery & Binary Configuration
The Claude Code CLI runtime adapter discovers the local `claude` executable automatically or through configuration:
```rust
use agent_studios_claude_code_runtime::discovery::ClaudeCodeDiscoveryConfig;
use agent_studios_claude_code_runtime::runtime::ClaudeCodeRuntime;
use std::path::PathBuf;

// Automatic discovery
let runtime = ClaudeCodeRuntime::new()?;

// Or explicit custom binary path:
let config = ClaudeCodeDiscoveryConfig::new()
    .with_custom_binary_path(PathBuf::from("C:\\Users\\User\\.local\\bin\\claude.exe"));
let runtime = ClaudeCodeRuntime::new()?.with_discovery_config(config);
```

### 2.2 Starting a Session with Sandboxing and Budget
```rust
use agent_studios_external_runtime::{
    AgentRuntime, RuntimeInstanceRef, RuntimeImplementationId, RuntimeInstanceId,
    RuntimeStartRequest, WorkspaceAccessMode,
};
use agent_studios_protocol::worktree::ExecutionWorkspace;

let instance_ref = RuntimeInstanceRef::new(
    RuntimeImplementationId::new("claude-code")?,
    RuntimeInstanceId::new(),
);

// Workspaces MUST be managed (SharedSource is rejected for security)
let request = RuntimeStartRequest::new(instance_ref, managed_workspace)
    .with_workspace_access_mode(WorkspaceAccessMode::Mutating)
    .with_runtime_config(agent_studios_external_runtime::RuntimeConfigRef::new("sandbox-standard")?);

let session = runtime.start(request).await?;
```

### 2.3 Handling Pre-Execution Tool Approvals
```rust
use agent_studios_protocol::id::ApprovalId;

// Host receives RuntimeEventKind::ApprovalRequested from subscription
// Host resolves approval by ApprovalId:
runtime.resolve_approval(session.session_id, approval_id, true, None).await?;
```

---

## 3. Commit Ledger on `feat/m12-claude-code-runtime`

- **Commit**: `feat(runtime): implement Claude Code CLI runtime adapter and subagent correlation (M12)`
- **Base Commit**: `dc363d1e89fe8751c001f9829bd2cd0b9f7d37f9` (`origin/main`)
- **Key Files Introduced**:
  - `agent-studios-rs/crates/claude-code-runtime/` (11 modules + 4 test suites, 29 tests)
  - `.github/workflows/upstream-intelligence.yml` (upstream release intelligence)
  - `docs/reviews/M12/RUN-20261027-M12-claude-code/` (6 review reports)
  - `docs/agent-studios/EXTERNAL_RUNTIMES.md` (runtime status updated)
  - `docs/agent-studios/ROADMAP.md` (roadmap status updated)

---

## 4. Review Dossier Index

All review artifacts for this milestone are located in:
`docs/reviews/M12/RUN-20261027-M12-claude-code/`

- `01_SCOPE_AND_BASELINE.md`: Scope, system baselines, environment constraints.
- `02_ARCHITECTURE_AND_PROTOCOL.md`: Wire protocol analysis, CLI flags, sequence diagrams.
- `03_IMPLEMENTATION_REPORT.md`: Comprehensive crate architecture, modules, and test results.
- `04_UPSTREAM_COMPATIBILITY.md`: Analysis of upstream releases (Codex, OpenCode, Claude Code, VSCode) and mitigation strategies.
- `05_SECURITY_AND_QA.md`: Independent QA and security audit, Win32 Job Object test results, canary redaction, and `codex-rs/` immutability verification.
- `06_FINAL_HANDOFF.md`: Acceptance checklist, operational guide, and roadmap recommendations.

---

## 5. Recommendations for Subsequent Milestones (M13+)

1. **Milestone M13 — Advanced Multi-Agent Orchestration & Hyper-V Workspace Isolation**:
   - Leverage `SubagentCorrelationTracker` to provide a real-time DAG visualizer in the Agent Studios desktop interface.
   - Investigate Hyper-V Container isolation on Windows to safely enable `SandboxLevel::Strict` without failing closed.
2. **Upstream Release Triage**:
   - Review automated issues generated by `.github/workflows/upstream-intelligence.yml`.
   - Ensure patch forward-porting for `codex-rs/` is scheduled when OpenAI releases Codex CLI 0.11+.
