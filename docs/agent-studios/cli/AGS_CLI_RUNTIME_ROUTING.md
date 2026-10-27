# Agent Studios CLI (`ags`) — Provider & Runtime Routing Architecture

> **Document**: `AGS_CLI_RUNTIME_ROUTING.md`  
> **Status**: Canonical Provider & Runtime Routing Specification  
> **Target Subsystem**: `ags` CLI Router / `runtime-session` / `provider` / External Runtime Adapters  
> **Precedence**: Aligned with `AGS_CLI_COMMAND_SURFACE.md`, `PRODUCT_PRINCIPLES.md`, and `M11`/`M12` architecture specs.

---

## 1. Executive Summary & Routing Philosophy

The Agent Studios CLI (`ags`) provides a unified, vendor-neutral terminal development environment capable of orchestrating multi-agent workflows across diverse AI models and agent runtimes.

A common failure mode in LLM tooling is tightly coupling model provider logic (e.g., Anthropic, OpenAI, Gemini, Ollama) directly to the execution runtime (e.g., Codex thread manager, OpenCode subprocess, or Claude Code CLI). `ags` strictly separates these concerns:
- **Provider Layer (`crates/provider`)**: Manages credentials, endpoint URLs, rate limits, protocol families (`Anthropic`, `OpenAi`, `Gemini`, `Ollama`), and authoritative model references (`ProviderInstanceId` + `ModelId`).
- **Runtime Layer (`crates/runtime-session`, `crates/external-runtime`)**: Manages the agent execution lifecycle, process sandboxing, tool execution, budget tracking, and event emission.

```
┌────────────────────────────────────────────────────────────────────────┐
│                      ags CLI Configuration Input                       │
│        (--provider anthropic --model claude-3-7-sonnet --runtime internal-codex)        │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                     Configuration Resolution Engine                    │
│   CLI Flags  ──►  Session Overrides  ──►  Repo Config  ──►  Catalog    │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Resolved: ProviderInstanceId + ModelId
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                   ProviderCatalog & SecretResolver                     │
│    Retrieves ModelDescriptor, EndpointProfile, and API Key / OAuth     │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
         ┌──────────────────────────┴──────────────────────────┐
         ▼                                                     ▼
┌────────────────────────────────┐            ┌────────────────────────────────┐
│   AgentStudiosRuntimeSession   │            │   External Runtime Registry    │
│   Factory (Internal Engine)    │            │     (Subprocess Adapters)      │
├────────────────────────────────┤            ├────────────────────────────────┤
│ • ModelRuntimeOverride         │            │ • OpenCodeRuntime (M11)        │
│ • SharedModelProvider          │            │ • ClaudeCodeRuntime (M12)      │
│ • StaticModelsManager          │            │ • Windows Job Object Sandbox   │
│ • Codex Thread Session         │            │ • SSE / NDJSON Event Bridge    │
└────────────────────────────────┘            └────────────────────────────────┘
```

---

## 2. Configuration Resolution Hierarchy

When `ags` launches or processes a prompt, it resolves runtime and model parameters using a strict 5-tier precedence hierarchy.

```text
[Highest Priority]
  Tier 1: Explicit CLI command-line arguments (--provider, --model, --runtime)
  Tier 2: Interactive Slash Command Overrides in TUI (/provider, /model, /runtime)
  Tier 3: Session Sticky State (.agent-studios/sessions/<session_id>.json)
  Tier 4: Workspace Repository Configuration (.agent-studios/config.toml)
  Tier 5: Global User Configuration (~/.config/agent-studios/config.toml)
  Tier 6: Built-in Catalog Default Instances (anthropic / claude-3-7-sonnet)
[Lowest Priority]
```

### 2.1 Workspace Configuration Schema (`.agent-studios/config.toml`)

```toml
version = 1

[workspace]
default_runtime = "internal-codex"
default_provider = "anthropic"
default_model = "claude-3-7-sonnet"

[runtimes.internal-codex]
temperature = 0.2
max_tokens = 8192
thinking_budget = 4096

[runtimes.opencode]
server_port = 4096
sandbox_level = "restricted" # "strict" | "restricted" | "permissive"
job_object_memory_limit_mb = 2048

[runtimes.claude-code]
subagent_depth_limit = 2
cli_path = "claude"

[providers.anthropic]
api_base = "https://api.anthropic.com"
auth_type = "api_key"
secret_ref = "env:ANTHROPIC_API_KEY"

[providers.ollama_local]
api_base = "http://127.0.0.1:11434"
auth_type = "none"
protocol = "ollama"
```

---

## 3. Provider Catalog & Session Factory Pipeline

For internal agent execution, `ags` constructs an `AgentStudiosRuntimeSessionFactory` from `crates/runtime-session`.

### 3.1 Data Flow & Types

```rust
use std::sync::Arc;
use agent_studios_provider::{
    ModelId, ModelRef, ProviderCatalog, ProviderInstanceId, ProtocolFamily,
};
use agent_studios_runtime_session::{
    AgentStudiosRuntimeSessionFactory, PreparedRuntimeSession, StaticModelsManager,
};
use agent_studios_runtime_transport::options::RuntimeTransportOptions;
use agent_studios_runtime_transport::secret::SecretResolver;
use agent_studios_runtime_transport::state::ContinuationManager;

pub struct RuntimeBootstrapContext {
    pub catalog: Arc<ProviderCatalog>,
    pub secret_resolver: Arc<dyn SecretResolver>,
    pub continuation_manager: Arc<ContinuationManager>,
}

impl RuntimeBootstrapContext {
    /// Prepares an authoritative runtime session for a specific provider and model.
    pub async fn prepare_session(
        &self,
        provider_instance_id: &ProviderInstanceId,
        model_id: &ModelId,
    ) -> Result<PreparedRuntimeSession, RuntimeRoutingError> {
        let factory = AgentStudiosRuntimeSessionFactory::new(
            Arc::clone(&self.catalog),
            Arc::clone(&self.secret_resolver),
            Arc::clone(&self.continuation_manager),
        );

        let model_ref = ModelRef::new(provider_instance_id.clone(), model_id.clone());

        let prepared_session = factory
            .create_session(&model_ref)
            .map_err(|e| RuntimeRoutingError::SessionPreparationFailed {
                provider: provider_instance_id.to_string(),
                model: model_id.to_string(),
                source: e,
            })?;

        tracing::info!(
            provider = %provider_instance_id,
            model = %model_id,
            protocol = ?prepared_session.protocol(),
            "Prepared unified ModelRuntimeOverride for session"
        );

        Ok(prepared_session)
    }
}
```

### 3.2 Injecting Overrides into Codex Thread Sessions

The resulting `PreparedRuntimeSession` contains the strictly paired `ModelRuntimeOverride` handle:
- **`SharedModelProvider`**: Translates Codex conversation requests into the target wire protocol (Anthropic Messages API, OpenAI Chat Completions, Gemini Content, or Ollama generate).
- **`StaticModelsManager`**: Provides instantaneous, zero-discovery model metadata (context windows, tool call support, max tokens) without making external network calls during initialization.

```rust
use codex_core::{ModelRuntimeOverride, StartThreadOptions};

pub fn build_codex_start_options(
    prepared: &PreparedRuntimeSession,
    working_dir: std::path::PathBuf,
) -> StartThreadOptions {
    StartThreadOptions {
        model_runtime_override: Some(prepared.runtime_override().clone()),
        model_provider_id: Some(prepared.model_provider_id().to_string()),
        model_id: Some(prepared.selected_model().to_string()),
        working_directory: working_dir,
        ..Default::default()
    }
}
```

---

## 4. Heterogeneous Runtime Adapter Engines

`ags` supports three distinct runtime engines selected via `--runtime <RUNTIME_ID>` or `/runtime <RUNTIME_ID>`.

| Runtime ID | Execution Paradigm | Sandboxing Level | Best Suited For |
| :--- | :--- | :--- | :--- |
| `internal-codex` | In-process native Rust actor & thread manager | In-process workspace leases & worktrees | Autonomous multi-agent teams, high velocity, minimum overhead |
| `opencode` | Subprocess server / HTTP client | Windows Job Objects, Linux cgroups, seccomp | Sandboxed external execution, untrusted repositories |
| `claude-code` | Subprocess CLI child process (stdio) | Subprocess isolation, interactive approvals | Deep reasoning, complex refactors, native Claude Code workflows |

### 4.1 Runtime 1: `internal-codex` (In-Process Engine)
- **Architecture**: Runs entirely inside the `ags` process using `crates/internal-agent`.
- **Concurrency**: Managed by `WorkspacePolicyArbitrator` allowing concurrent read-only agents and single-mutator leases.
- **Multi-Agent Teams**: Supports the standard Agent Studios team structure (Coordinator, Coder, Tester, Reviewer) where each agent can possess an independent `ModelRef` and budget profile.

### 4.2 Runtime 2: `opencode` (OpenCode Adapter)
- **Architecture**: Coordinates with an OpenCode HTTP server instance using `crates/opencode-runtime`.
- **Sandboxing**: Automatically places the OpenCode child process inside a Windows Job Object (`JobObjectGuard`) or Unix namespace sandbox:
  ```rust
  use agent_studios_opencode_runtime::sandbox::{JobObjectGuard, SandboxPolicy};

  let policy = SandboxPolicy {
      max_memory_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
      active_process_limit: 32,
      kill_on_job_close: true,
  };
  let job_guard = JobObjectGuard::new("agent-studios-opencode", &policy)?;
  job_guard.assign_process(child_process_handle)?;
  ```
- **Event Streaming**: Reads Server-Sent Events (SSE) from OpenCode `/events` endpoint and translates them into canonical `ControlPlaneEvent` envelopes via `translate_opencode_event_filtered()`.

### 4.3 Runtime 3: `claude-code` (Claude Code Adapter)
- **Architecture**: Launches the Claude Code CLI (`claude`) in headless JSON streaming mode via stdin/stdout NDJSON pipes.
- **Protocol Translation**: Translates Claude Code JSON control requests into Agent Studios `ControlPlaneEvent::ApprovalRequested` and waits for `ResolutionToken` response before writing approval decisions back to child stdin.
- **Subagent Tracking**: Captures subagent creation events, assigning hierarchical `AgentId` tags and correlating tool costs against the parent task budget.

---

## 5. Dynamic Mid-Session Runtime & Model Switching

A developer can switch models or runtimes dynamically without exiting `ags` or losing conversation context.

### 5.1 The `/model` Switch Flow

```text
Developer enters: /model openai/o3-mini
                   │
                   ▼
1. TUI parses command and validates "openai" provider and "o3-mini" in ProviderCatalog.
2. Client emits ControlPlaneCommand::RecordRuntimeBound:
   - agent_id: active_agent_id
   - runtime_kind: "internal-codex"
   - provider_instance_id: "openai"
   - model_id: "o3-mini"
   - protocol: "openai_chat_completions"
3. RuntimeSessionBootstrap builds new PreparedRuntimeSession.
4. Next user turn injects the updated ModelRuntimeOverride.
5. HeaderWidget updates model badge: "openai/o3-mini".
6. Status notice printed to transcript: "Switched active model to openai/o3-mini".
```

### 5.2 The `/runtime` Switch Flow

When switching runtimes (e.g., from `internal-codex` to `claude-code`):
1. Active in-flight tools complete or are cleanly cancelled.
2. Uncommitted worktree changes are captured via `capture_changes()` to ensure no code loss.
3. The new runtime adapter is initialized and attached to the existing `StudioId` and task graph.
4. Subsequent tasks execute through the newly active runtime adapter.

---

## 6. Typed Implementation Structures

```rust
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use agent_studios_provider::id::{ModelId, ProviderInstanceId};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CliRuntimeConfig {
    pub provider_instance: ProviderInstanceId,
    pub model_id: ModelId,
    pub runtime_id: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub auto_approve_policy: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeRoutingError {
    #[error("Unknown provider instance: '{provider}'. Available: {available:?}")]
    UnknownProvider {
        provider: String,
        available: Vec<String>,
    },

    #[error("Model '{model}' not found in provider '{provider}'. Available: {available:?}")]
    UnknownModel {
        provider: String,
        model: String,
        available: Vec<String>,
    },

    #[error("Unsupported runtime adapter: '{runtime}'. Supported: 'internal-codex', 'opencode', 'claude-code'")]
    UnsupportedRuntime { runtime: String },

    #[error("Failed to resolve credentials for provider '{provider}': {reason}")]
    AuthenticationFailed {
        provider: String,
        reason: String,
    },

    #[error("Runtime session preparation failed for {provider}/{model}: {source}")]
    SessionPreparationFailed {
        provider: String,
        model: String,
        #[source]
        source: agent_studios_runtime_session::RuntimeSessionError,
    },
}
```
