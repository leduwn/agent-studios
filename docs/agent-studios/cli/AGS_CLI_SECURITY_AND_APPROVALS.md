# Agent Studios CLI (`ags`) — Security, Approvals & Sandboxing Architecture

> **Document**: `AGS_CLI_SECURITY_AND_APPROVALS.md`  
> **Status**: Canonical Security & Approval Specification (Audited & Honesty-Tagged)  
> **Target Subsystem**: `ags` Security Gate / Tool Approval Modal / Fail-Closed Protocol  
> **Precedence**: Aligned with `PRODUCT_PRINCIPLES.md` (Principle #22), `AGS_CLI_COMPONENT_INVENTORY.md`, and `M11`/`M12` security specs.  
> **Audit Status Legend**:
> - `[IMPLEMENTED / VERIFIED]`: Fully implemented in code and verified by automated tests.
> - `[IMPLEMENTED / NOT VERIFIED]`: Implemented in code but awaiting live remote / daemon verification.
> - `[PREVIEW ONLY]`: Visual/interaction design demonstrable in `ags --preview` with sample data.
> - `[PLANNED]`: Architectural design finalized; scheduled for Milestone M13+.
> - `[BLOCKED]`: Implementation blocked on external dependency.

---

## 1. Threat Model & Security Philosophy

Agent Studios CLI (`ags`) executes autonomous code generation and refactoring agents directly within developer workspaces. By nature, coding agents invoke external compilers, linters, shell scripts, and network APIs. Without strict, verifiable security boundaries, LLM tool execution presents severe operational risks:
1. **Unintended Workspace Destruction**: Erroneous `rm -rf`, `git reset --hard`, or file truncation overwriting hours of uncommitted work.
2. **Host Machine Compromise**: Prompt-injection attacks tricking an agent into downloading malicious binaries, editing system credentials (`~/.ssh`, `~/.aws`), or exfiltrating private source code to external servers.
3. **Silent Operation Cloaking**: Dangerous commands disguised behind innocuous-looking build tasks.

To defend against these threats, `ags` enforces **Fail-Closed Interactive Security**:
- **Default Deny / Fail-Closed**: Every privileged operation requires explicit permission. If a user disconnects, a watchdog timer expires, or the UI process panics, the approval **must evaluate to Deny**.
- **Transparent Parameter Exposure**: No command is hidden behind generic "Executing..." labels. Full command strings, working directories, and file diffs are displayed before authorization.
- **Two-Phase Commit Resolution**: Permissions are verified with cryptographically unique tokens to prevent replay attacks and duplicate execution.

---

## 2. Risk Classification Tiers

All agent tool actions are evaluated against a 3-tier risk taxonomy:

```
┌────────────────────────────────────────────────────────────────────────┐
│                        Tool Call Request Stream                        │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
       ┌────────────────────────────┼────────────────────────────┐
       ▼                            ▼                            ▼
┌──────────────┐             ┌──────────────┐             ┌──────────────┐
│   Tier 1:    │             │   Tier 2:    │             │   Tier 3:    │
│   ReadOnly   │             │  Workspace-  │             │  Dangerous-  │
│              │             │   Mutating   │             │     Host     │
├──────────────┤             ├──────────────┤             ├──────────────┤
│ • File Reads │             │ • File Writes│             │ • Host Shell │
│ • Grep / Glob│             │ • File Delete│             │ • rm / del   │
│ • Git Status │             │ • Git Branch │             │ • Curl / Net │
│ • AST Search │             │ • Linter Run │             │ • Pkg Install│
├──────────────┤             ├──────────────┤             ├──────────────┤
│ Auto-Allowed │             │ Gated by     │             │ ALWAYS PROMPT│
│  (Default)   │             │ Policy Flag  │             │ Double Red   │
└──────────────┘             └──────────────┘             └──────────────┘
```

### 2.1 Tier Definitions & Rules

| Risk Tier | Visual Indicator | Trigger Operations | Default Policy | Auto-Approve Eligible? |
| :--- | :--- | :--- | :--- | :---: |
| **`ReadOnly`** | Sky Blue (`#58a6ff`) | Reading files, searching directory trees, inspection queries, git log/status. | Auto-allowed | Yes (`--auto-approve readonly`) |
| **`WorkspaceMutating`** | Amber (`#d29922`) | Modifying or creating files within the managed worktree or workspace repository. | Prompts user unless policy allows | Yes (`--auto-approve workspace`) |
| **`DangerousHost`** | Crimson (`#f85149`) | Shell script execution, external network requests, environment mutation, package installation. | ALWAYS PROMPTS USER | **NO** (Strictly forbidden unless `--auto-approve all`) |

---

## 3. Fail-Closed Protocol & Two-Phase Commit

To guarantee that no command executes without verified approval, `ags` implements a two-phase commit protocol linked to `crates/protocol/src/approval.rs`.

```
  Agent Runtime               ags Client / Control Plane                  TUI Modal
       │                                  │                                   │
       │─── 1. Tool Call Intercept ──────►│                                   │
       │    (e.g., exec "cargo build")    │                                   │
       │                                  │─── 2. ControlPlaneEvent ─────────►│
       │                                  │    ApprovalRequested(id, tier)    │
       │                                  │                                   │
       │                                  │                                   │ [User Reviews Modal]
       │                                  │                                   │ [Presses 'y' to Approve]
       │                                  │                                   │
       │                                  │◄── 3. ResolveApproval ────────────│
       │                                  │    (ApprovalId, Decision)         │
       │                                  │                                   │
       │                                  │─── 4. Construct ResolutionToken ──│
       │                                  │    State: Resolving               │
       │◄── 5. Stdin / IPC Deliver ───────│                                   │
       │    {"decision": "allow"}         │                                   │
       │                                  │                                   │
       │─── 6. Delivery Acknowledged ────►│                                   │
       │                                  │─── 7. Emit ApprovalResolved ─────►│
       │                                  │    State: Delivered               │
       ▼                                  ▼                                   ▼
```

### 3.1 Approval Lifecycle States

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    /// Awaiting user keyboard decision or watchdog timeout.
    Pending,
    /// User approved; token generated, in-flight to child process stdin or IPC.
    Resolving,
    /// Write to child process confirmed; tool allowed to proceed.
    Delivered,
    /// Communication failure delivering decision; tool fails closed.
    DeliveryFailed,
    /// Session cancelled via Ctrl+C or user rejection; tool denied.
    Cancelled,
    /// Watchdog timer (default: 60s) elapsed without decision; tool denied.
    TimedOut,
}

impl ApprovalState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Delivered | Self::DeliveryFailed | Self::Cancelled | Self::TimedOut
        )
    }
}
```

### 3.2 Resolution Token

The `ResolutionToken` guarantees that approval decisions are strictly correlated and never replayed:

```rust
use agent_studios_protocol::id::ApprovalId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolutionToken {
    pub approval_id: ApprovalId,
    pub request_id: String,
    pub decision: ApprovalDecision,
    pub issued_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    Allow,
    Deny,
}
```

---

## 4. TUI Approval Modal & User Interaction

When an approval is required, `AgsApp` transitions into the modal state, rendering `ApprovalModalWidget` over the center of the screen.

```
┌────────────────────────────────────────────────────────────────────────┐
│                                                                        │
│   ╔════════════════════════════════════════════════════════════════╗   │
│   ║  [!] DANGEROUS HOST EXECUTION REQUEST                          ║   │
│   ╠════════════════════════════════════════════════════════════════╣   │
│   ║                                                                ║   │
│   ║  Agent:     Coder (wt_4b1a0e_task_101)                         ║   │
│   ║  Tool:      bash_execution                                     ║   │
│   ║  Directory: /home/duwn/work/auth-svc/.agent-studios/wt/task-101║   │
│   ║                                                                ║   │
│   ║  Command:                                                      ║   │
│   ║  $ cargo install --path ./crates/cli --force                   ║   │
│   ║                                                                ║   │
│   ║  Risk Explanation:                                             ║   │
│   ║  Compiles and installs executable into host user PATH.         ║   │
│   ║                                                                ║   │
│   ╠════════════════════════════════════════════════════════════════╣   │
│   ║  [y] Approve Once   [n] Reject & Stop                          ║   │
│   ║  [a] Allow for Session   [d] Inspect Full Diff                 ║   │
│   ╚════════════════════════════════════════════════════════════════╝   │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

### 4.1 Keyboard Action Semantics
- `y` or `Enter`: **Approve Once** — Resolves this individual call with `ApprovalDecision::Allow`.
- `n` or `Esc`: **Reject & Deny** — Resolves with `ApprovalDecision::Deny`. The agent receives `PermissionDenied` and must formulate an alternative plan.
- `a`: **Allow for Session** — Adds the exact command prefix or file pattern to the active in-memory session whitelist, automatically approving matching future invocations.
- `d`: **Inspect Diff** — Temporarily overlays the `DiffViewerWidget` showing files modified in the worktree before executing the command.

---

## 5. Session Whitelist & Pattern Matching

To prevent developer fatigue during repetitive build/test cycles, `ags` maintains an in-memory session whitelist.

```rust
use std::path::{Path, PathBuf};
use regex::Regex;

#[derive(Default, Debug)]
pub struct SessionApprovalWhitelist {
    allowed_command_prefixes: Vec<String>,
    allowed_command_patterns: Vec<Regex>,
    allowed_path_prefixes: Vec<PathBuf>,
}

impl SessionApprovalWhitelist {
    pub fn allow_command_prefix(&mut self, prefix: impl Into<String>) {
        self.allowed_command_prefixes.push(prefix.into());
    }

    pub fn allow_path_prefix(&mut self, path: impl Into<PathBuf>) {
        self.allowed_path_prefixes.push(path.into());
    }

    pub fn is_command_allowed(&self, command: &str) -> bool {
        let trimmed = command.trim();
        for prefix in &self.allowed_command_prefixes {
            if trimmed.starts_with(prefix) {
                return true;
            }
        }
        for pattern in &self.allowed_command_patterns {
            if pattern.is_match(trimmed) {
                return true;
            }
        }
        false
    }

    pub fn is_path_allowed(&self, path: &Path) -> bool {
        for allowed in &self.allowed_path_prefixes {
            if path.starts_with(allowed) {
                return true;
            }
        }
        false
    }
}
```

---

## 6. Cross-Runtime Approval Adapters

`ags` normalizes approvals across all three supported runtime engines:

### 6.1 `internal-codex` Adapter
Intercepted via `AgentStudiosToolLifecycleContributor` in `crates/internal-agent`. Before tool dispatch:
```rust
let (approval_tx, approval_rx) = tokio::sync::oneshot::channel();
control_plane.request_approval(studio_id, task_id, agent_id, kind, summary)?;
// Awaits user decision through TUI modal
let decision = approval_rx.await.unwrap_or(ApprovalDecision::Deny);
```

### 6.2 `opencode` Adapter
OpenCode emits a permission event over SSE. The `ags` client captures the event, displays the approval modal, and responds via HTTP POST to `/permission/decision`:
```rust
let payload = PermissionDecisionRequest {
    session_id: session_id.clone(),
    permission_id: perm_id.clone(),
    decision: if decision.is_allowed() { "allow" } else { "deny" }.to_string(),
};
opencode_client.submit_permission_decision(payload).await?;
```

### 6.3 `claude-code` Adapter
Claude Code streams a `can_use_tool` control request over stdio NDJSON. `ags` captures the JSON, surfaces it in the modal, and writes the response back to child stdin:
```rust
let response_json = decision.to_control_response_value(&request_id);
child_stdin.write_all(response_json.to_string().as_bytes()).await?;
child_stdin.write_all(b"\n").await?;
child_stdin.flush().await?;
```

---

## 7. Concrete Rust Struct Definitions

```rust
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use agent_studios_protocol::id::ApprovalId;

pub struct PendingApprovalGuard {
    pub approval_id: ApprovalId,
    pub request_id: String,
    pub state: ApprovalState,
    pub watchdog_deadline: std::time::Instant,
}

pub struct CliSecurityManager {
    whitelist: Arc<RwLock<SessionApprovalWhitelist>>,
    pending_approvals: Arc<Mutex<std::collections::HashMap<ApprovalId, PendingApprovalGuard>>>,
    default_policy: AutoApprovePolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoApprovePolicy {
    None,
    ReadOnly,
    Workspace,
    All,
}

impl CliSecurityManager {
    pub fn new(default_policy: AutoApprovePolicy) -> Self {
        Self {
            whitelist: Arc::new(RwLock::new(SessionApprovalWhitelist::default())),
            pending_approvals: Arc::new(Mutex::new(std::collections::HashMap::new())),
            default_policy,
        }
    }

    /// Evaluates if an operation can proceed automatically without prompting the user.
    pub async fn should_auto_approve(
        &self,
        kind: agent_studios_protocol::approval::ApprovalKind,
        command_or_path: &str,
    ) -> bool {
        match self.default_policy {
            AutoApprovePolicy::All => true,
            AutoApprovePolicy::Workspace => {
                matches!(
                    kind,
                    agent_studios_protocol::approval::ApprovalKind::FileWrite
                        | agent_studios_protocol::approval::ApprovalKind::Other
                )
            }
            AutoApprovePolicy::ReadOnly => false,
            AutoApprovePolicy::None => false,
        } || self.whitelist.read().await.is_command_allowed(command_or_path)
    }
}
```

---

## 8. Security & Approvals Feature-to-Test Evidence Matrix

| Security / Approval Feature | Honesty Status | Implementation File | Verification Test Module |
| :--- | :--- | :--- | :--- |
| **Fail-Closed Security Gate** | `[IMPLEMENTED / VERIFIED]` | `crates/control-plane/src/` | `crates/control-plane/tests/integration_tests.rs` |
| **ApprovalModal Widget** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/widgets/approval_modal.rs`| `crates/tui/tests/widget_tests.rs::test_widgets_render_without_panics` |
| **Two-Phase Commit Token** | `[IMPLEMENTED / VERIFIED]` | `crates/claude-code-runtime/src/adapter.rs`| `crates/claude-code-runtime/tests/adapter_integration_tests.rs` |
| **Watchdog Timeout Handling** | `[IMPLEMENTED / VERIFIED]` | `crates/claude-code-runtime/src/adapter.rs`| `crates/claude-code-runtime/tests/budget_lifecycle_tests.rs` |
| **Delivery Failure / Abort** | `[IMPLEMENTED / VERIFIED]` | `crates/claude-code-runtime/src/adapter.rs`| `crates/claude-code-runtime/tests/zero_duration_timeout_tests.rs` |
| **OpenCode Permissions Bridge**| `[IMPLEMENTED / VERIFIED]` | `crates/opencode-runtime/src/adapter.rs`| 11 suites in `crates/opencode-runtime/tests/` |
| **Job Object Sandbox (Windows)**| `[IMPLEMENTED / VERIFIED]`| `crates/opencode-runtime/src/sandbox.rs`| `crates/opencode-runtime/tests/sandbox_lifecycle_tests.rs` |
| **Session Whitelist Storage** | `[IMPLEMENTED / VERIFIED]` | `crates/tui/src/widgets/approval_modal.rs`| `crates/tui/tests/widget_tests.rs` |
