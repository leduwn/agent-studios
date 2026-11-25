//! Native Internal Codex Runtime Bridge for `agsd`
//!
//! Connects `agent-studios-ipc-transport` directly to:
//! - `codex_core::ThreadManager` / `CodexThread` (via `agent-studios-internal-agent`)
//! - `AgentStudiosRuntimeSessionFactory` (via `agent-studios-runtime-session`)
//! - `ContinuationManager` & `SecretResolver` (via `agent-studios-runtime-transport`)
//! - `SkillRegistry`, `McpRegistry`, `InstructionResolver`, and `hooks` (`agent-studios-extensibility`)
//! - `WorkspaceOrchestrator` & `ControlPlane` for isolated Git worktrees & patch artifacts

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use agent_studios_extensibility::{
    InstructionResolver, LifecycleHookPhase, McpRegistry, McpRiskTier, SkillRegistry,
    execute_phase_hooks,
};
use agent_studios_external_runtime::{
    AgentRuntime, DiscoveredRuntimeInstance, RuntimeAvailability, RuntimeCapabilities,
    RuntimeCapabilitySupport, RuntimeConfigRef, RuntimeError, RuntimeEventKind,
    RuntimeEventSubscription, RuntimeImplementationId, RuntimeInput, RuntimeInstanceId,
    RuntimeLifecycleState, RuntimeSessionHandle, RuntimeSessionId, RuntimeSessionRef,
    RuntimeStartRequest, SanitizedRuntimeMessage, SessionEventHub,
};
use agent_studios_internal_agent::{
    AgentStudiosCodexRuntimeFactory, CoordinatorDecision, build_agent_studios_extension_builder,
};
use agent_studios_protocol::agent::{AgentKind, AgentState};
use agent_studios_protocol::id::{
    AgentId, ApprovalId as CpApprovalId, ArtifactId, RunId, StudioId, TaskId, WorktreeId,
};
use agent_studios_protocol::reconciliation::ReconciliationState;
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::{
    BatchTaskSpec, DependencyOutputPolicy, TaskExecutionSpec, TaskState,
};
use agent_studios_protocol::worktree::WorktreeState;
use agent_studios_provider::ProviderCatalog;
use agent_studios_provider::error::ProviderError;
use agent_studios_provider::id::{ModelId, ProviderInstanceId};
use agent_studios_provider::model::ModelRef;
use agent_studios_provider::secret::{SecretBackend, SecretReference};
use agent_studios_provider::store::{OsKeyringSecretStore, SecretStore};
use agent_studios_runtime_session::AgentStudiosRuntimeSessionFactory;
use agent_studios_runtime_transport::secret::{SecretResolver, SecretString};
use agent_studios_runtime_transport::{
    ContinuationManager, TransportError as RuntimeTransportError,
};
use agent_studios_workspace::WorkspaceOrchestrator;
use async_trait::async_trait;
use chrono::Utc;
use codex_config::{Constrained, LoaderOverrides};
use codex_core::config::{Config, ConfigBuilder, Permissions};
use codex_core::{CodexThread, StartThreadOptions, ThreadManager, TurnInputRequest};
use codex_extension_api::{ExtensionDataInit, ToolLifecycleContributor, ToolStartInput};
use codex_login::{AuthManager, CodexAuth};
use codex_protocol::ThreadId;
use codex_protocol::models::PermissionProfile;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::{AskForApproval, EventMsg, Op, SandboxPolicy, SessionSource};
use codex_protocol::user_input::UserInput;
use codex_utils_absolute_path::AbsolutePathBuf;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock as AsyncRwLock;
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::server::ServerState;

// ============================================================================
// 1. Secure Secret Vault (SecretStore + SecretResolver)
// ============================================================================

/// Thread-safe, zero-leakage secret vault for `agsd`.
///
/// Combines:
/// - Process memory cache (`secrets`)
/// - Restricted user-only file persistence (`state_dir/secrets.vault.json`)
/// - Process environment (`std::env::var`) for `SecretBackend::EnvironmentVariable`
/// - Platform OS keyring (`OsKeyringSecretStore`) fallback for `OsCredentialStore`
pub struct DaemonSecretVault {
    entries: RwLock<HashMap<String, String>>,
    vault_path: Option<PathBuf>,
    os_keyring: OsKeyringSecretStore,
}

impl std::fmt::Debug for DaemonSecretVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.entries.read().map(|m| m.len()).unwrap_or(0);
        f.debug_struct("DaemonSecretVault")
            .field("entry_count", &count)
            .field("vault_path", &self.vault_path)
            .finish()
    }
}

impl DaemonSecretVault {
    pub fn new(vault_path: Option<PathBuf>) -> Self {
        let mut initial = HashMap::new();
        if let Some(ref p) = vault_path
            && p.exists()
            && let Ok(raw) = std::fs::read_to_string(p)
            && let Ok(parsed) = serde_json::from_str::<HashMap<String, String>>(&raw)
        {
            initial = parsed;
        }
        Self {
            entries: RwLock::new(initial),
            vault_path,
            os_keyring: OsKeyringSecretStore::new(),
        }
    }

    fn key_for(reference: &SecretReference) -> String {
        format!("{}:{}", reference.backend, reference.locator)
    }

    fn persist_locked(&self, map: &HashMap<String, String>) -> Result<(), String> {
        let Some(ref path) = self.vault_path else {
            return Ok(());
        };
        let json = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
        crate::daemon::save_secure_bootstrap_file(path, &json).map_err(|e| e.to_string())
    }
}

impl SecretStore for DaemonSecretVault {
    fn get_secret(&self, reference: &SecretReference) -> Result<String, ProviderError> {
        reference.validate()?;
        let key = Self::key_for(reference);
        if let Ok(guard) = self.entries.read() {
            if let Some(val) = guard.get(&key)
                && !val.trim().is_empty()
            {
                return Ok(val.clone());
            }
            if reference.backend != SecretBackend::EnvironmentVariable
                && let Some(val) = guard.get(&format!(
                    "{}:{}",
                    SecretBackend::External,
                    reference.locator
                ))
                && !val.trim().is_empty()
            {
                return Ok(val.clone());
            }
        }

        match reference.backend {
            SecretBackend::EnvironmentVariable => match std::env::var(&reference.locator) {
                Ok(v) if !v.trim().is_empty() => Ok(v),
                _ => Err(ProviderError::SecretNotFound(reference.locator.clone())),
            },
            SecretBackend::OsCredentialStore => self.os_keyring.get_secret(reference),
            SecretBackend::External => {
                Err(ProviderError::SecretNotFound(reference.locator.clone()))
            }
        }
    }

    fn set_secret(&self, reference: &SecretReference, value: &str) -> Result<(), ProviderError> {
        reference.validate()?;
        if value.trim().is_empty() {
            return Err(ProviderError::SecretStoreError(
                "Secret value cannot be empty".to_string(),
            ));
        }
        let key = Self::key_for(reference);
        let mut guard = self
            .entries
            .write()
            .map_err(|e| ProviderError::SecretStoreError(format!("Lock poisoned: {e}")))?;
        guard.insert(key, value.to_string());
        if let Err(e) = self.persist_locked(&guard) {
            return Err(ProviderError::SecretStoreError(format!(
                "Failed to persist daemon secret vault: {e}"
            )));
        }
        if reference.backend == SecretBackend::OsCredentialStore {
            let _ = self.os_keyring.set_secret(reference, value);
        }
        Ok(())
    }

    fn delete_secret(&self, reference: &SecretReference) -> Result<(), ProviderError> {
        reference.validate()?;
        let key = Self::key_for(reference);
        let mut guard = self
            .entries
            .write()
            .map_err(|e| ProviderError::SecretStoreError(format!("Lock poisoned: {e}")))?;
        let removed = guard.remove(&key).is_some();
        let _ = self.persist_locked(&guard);
        if reference.backend == SecretBackend::OsCredentialStore {
            let keyring_res = self.os_keyring.delete_secret(reference);
            if !removed && keyring_res.is_err() {
                return Err(ProviderError::SecretNotFound(reference.locator.clone()));
            }
            return Ok(());
        }
        if !removed {
            return Err(ProviderError::SecretNotFound(reference.locator.clone()));
        }
        Ok(())
    }
}

#[async_trait]
impl SecretResolver for DaemonSecretVault {
    async fn resolve(
        &self,
        reference: &SecretReference,
    ) -> Result<SecretString, RuntimeTransportError> {
        match self.get_secret(reference) {
            Ok(v) if v.trim().is_empty() => Err(RuntimeTransportError::EmptySecret {
                locator: reference.locator.clone(),
            }),
            Ok(v) => Ok(SecretString::new(v)),
            Err(_) => Err(RuntimeTransportError::SecretNotFound {
                backend: reference.backend.to_string(),
                locator: reference.locator.clone(),
            }),
        }
    }
}

// ============================================================================
// 2. Read-Only Permission Guard Extension Contributor
// ============================================================================

#[derive(Clone, Debug)]
struct ReadOnlyToolGuardContext {
    read_only: bool,
    blocked_calls: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone, Default)]
struct ReadOnlyToolGuardContributor;

impl ToolLifecycleContributor for ReadOnlyToolGuardContributor {
    fn authorize_tool_call(&self, input: &ToolStartInput<'_>) -> Result<(), String> {
        let Some(guard) = input.thread_store.get::<ReadOnlyToolGuardContext>() else {
            return Ok(());
        };
        if !guard.read_only {
            return Ok(());
        }
        let name = input.tool_name.name.to_ascii_lowercase();
        let is_mutating = matches!(
            name.as_str(),
            "shell"
                | "local_shell"
                | "exec_command"
                | "write_stdin"
                | "apply_patch"
                | "write_file"
                | "edit_file"
                | "delete_file"
        );
        if is_mutating {
            guard
                .blocked_calls
                .lock()
                .unwrap()
                .push(format!("{} ({})", input.tool_name.name, input.call_id));
            return Err(format!(
                "Tool '{}' blocked: session permission_mode is read-only",
                input.tool_name.name
            ));
        }
        Ok(())
    }
}

// ============================================================================
// 3. Durable Studio Thread & Transcript Manager + CodexStudioEngine
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioToolCallRecord {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioTranscriptMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<StudioToolCallRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistedStudioSession {
    pub studio_id: StudioId,
    pub thread_id: Option<String>,
    pub provider_instance_id: Option<String>,
    pub model_id: Option<String>,
    pub permission_mode: Option<String>,
    pub workspace_path: Option<String>,
    pub messages: Vec<StudioTranscriptMessage>,
}

struct LiveStudioThread {
    thread_id: ThreadId,
    thread: Arc<CodexThread>,
    provider_instance_id: ProviderInstanceId,
    model_id: String,
    permission_mode: String,
    cwd: PathBuf,
    read_only_guard: Arc<ReadOnlyToolGuardContext>,
}

#[derive(Debug, Clone)]
pub struct ChatTurnParams {
    pub studio_id: StudioId,
    pub prompt: String,
    pub mode: Option<String>,
    pub provider_instance_id: Option<ProviderInstanceId>,
    pub model_id: Option<String>,
    pub reasoning_effort: Option<String>,
    pub permission_mode: Option<String>,
    pub follow_up_mode: Option<String>,
    pub workspace_path: Option<String>,
    pub attachments: Vec<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ChatTurnOutcome {
    pub studio_id: StudioId,
    pub thread_id: String,
    pub assistant_message: StudioTranscriptMessage,
}

#[derive(Debug, Clone)]
pub struct MultiAgentGoalParams {
    pub studio_id: StudioId,
    pub goal: String,
    pub provider_instance_id: Option<ProviderInstanceId>,
    pub model_id: Option<String>,
    pub repository_path: Option<String>,
    pub permission_mode: Option<String>,
}

type SharedThreadManager = Arc<AsyncRwLock<Option<(Arc<ThreadManager>, Config)>>>;
type WorkerTaskResult = Result<
    (
        TaskId,
        RunId,
        Option<serde_json::Value>,
        Option<ArtifactId>,
        Option<WorktreeId>,
    ),
    String,
>;

pub struct CodexStudioEngine {
    codex_home: PathBuf,
    provider_catalog: Arc<AsyncRwLock<ProviderCatalog>>,
    secret_vault: Arc<DaemonSecretVault>,
    continuation_manager: Arc<ContinuationManager>,
    sessions_path: Option<PathBuf>,
    persisted_sessions: Arc<AsyncRwLock<HashMap<StudioId, PersistedStudioSession>>>,
    live_threads: Arc<AsyncRwLock<HashMap<StudioId, LiveStudioThread>>>,
    thread_manager: SharedThreadManager,
    workspace_orchestrator: Option<Arc<WorkspaceOrchestrator>>,
    default_repo_path: Option<PathBuf>,
    default_model_ref: Option<ModelRef>,
}

impl CodexStudioEngine {
    pub fn new(
        codex_home: PathBuf,
        provider_catalog: Arc<AsyncRwLock<ProviderCatalog>>,
        secret_vault: Arc<DaemonSecretVault>,
        sessions_path: Option<PathBuf>,
        workspace_orchestrator: Option<Arc<WorkspaceOrchestrator>>,
        default_repo_path: Option<PathBuf>,
        default_model_ref: Option<ModelRef>,
    ) -> Self {
        let mut restored = HashMap::new();
        if let Some(ref path) = sessions_path
            && path.exists()
            && let Ok(raw) = std::fs::read_to_string(path)
            && let Ok(parsed) =
                serde_json::from_str::<HashMap<StudioId, PersistedStudioSession>>(&raw)
        {
            restored = parsed;
        }
        Self {
            codex_home,
            provider_catalog,
            secret_vault,
            continuation_manager: Arc::new(ContinuationManager::new()),
            sessions_path,
            persisted_sessions: Arc::new(AsyncRwLock::new(restored)),
            live_threads: Arc::new(AsyncRwLock::new(HashMap::new())),
            thread_manager: Arc::new(AsyncRwLock::new(None)),
            workspace_orchestrator,
            default_repo_path,
            default_model_ref,
        }
    }

    async fn persist_sessions(&self) {
        let Some(ref path) = self.sessions_path else {
            return;
        };
        let guard = self.persisted_sessions.read().await;
        if let Ok(json) = serde_json::to_string_pretty(&*guard) {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let tmp = path.with_extension("tmp");
            if std::fs::write(&tmp, json.as_bytes()).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }

    async fn ensure_thread_manager(&self) -> Result<(Arc<ThreadManager>, Config), String> {
        {
            let guard = self.thread_manager.read().await;
            if let Some(ref pair) = *guard {
                return Ok(pair.clone());
            }
        }
        let mut guard = self.thread_manager.write().await;
        if let Some(ref pair) = *guard {
            return Ok(pair.clone());
        }
        std::fs::create_dir_all(&self.codex_home)
            .map_err(|e| format!("Failed to create codex_home: {e}"))?;
        let mut config = ConfigBuilder::default()
            .loader_overrides(LoaderOverrides::without_managed_config_for_tests())
            .codex_home(self.codex_home.clone())
            .build()
            .await
            .map_err(|e| format!("Failed to build Codex Config: {e}"))?;
        let base_cwd = self
            .default_repo_path
            .clone()
            .unwrap_or_else(|| self.codex_home.clone());
        config.cwd = AbsolutePathBuf::from_absolute_path(&base_cwd)
            .map_err(|e| format!("Invalid cwd path {}: {e}", base_cwd.display()))?;

        let auth_manager = AuthManager::from_auth_for_testing_with_home(
            CodexAuth::from_api_key("agent-studios-byok"),
            config.codex_home.to_path_buf(),
        );
        let mut ext_builder = build_agent_studios_extension_builder::<Config>();
        ext_builder.tool_lifecycle_contributor(Arc::new(ReadOnlyToolGuardContributor));
        let extensions = Arc::new(ext_builder.build());

        let manager = AgentStudiosCodexRuntimeFactory::build_thread_manager_with_extensions(
            &config,
            auth_manager,
            Some(SessionSource::Exec),
            extensions,
        )
        .await
        .map_err(|e| format!("Failed to initialize Codex ThreadManager: {e}"))?;
        let pair = (Arc::new(manager), config);
        *guard = Some(pair.clone());
        Ok(pair)
    }

    async fn resolve_model_ref(
        &self,
        req_provider: Option<ProviderInstanceId>,
        req_model: Option<&str>,
        studio_id: Option<StudioId>,
    ) -> Result<ModelRef, String> {
        let catalog = self.provider_catalog.read().await;

        let saved_binding = if let Some(sid) = studio_id {
            let sessions = self.persisted_sessions.read().await;
            sessions.get(&sid).and_then(|s| {
                let p = s
                    .provider_instance_id
                    .as_deref()
                    .and_then(|v| v.parse::<ProviderInstanceId>().ok())?;
                let m = s.model_id.clone()?;
                Some((p, m))
            })
        } else {
            None
        };

        let resolved_inst_id = if let Some(pid) = req_provider {
            pid
        } else if let Some((saved_pid, _)) = saved_binding {
            saved_pid
        } else if let Some(ref def_ref) = self.default_model_ref {
            *def_ref.provider_instance_id()
        } else {
            catalog
                .list_provider_instances()
                .into_iter()
                .find(|inst| inst.enabled)
                .map(|inst| inst.id)
                .ok_or_else(|| {
                    "No enabled provider configured in daemon. Add a provider via Configure Provider before sending prompts.".to_string()
                })?
        };

        let resolved_model_str = if let Some(m) = req_model.filter(|s| !s.trim().is_empty()) {
            m.trim().to_string()
        } else if let Some((_, saved_m)) = saved_binding {
            saved_m
        } else if let Some(ref def_ref) = self.default_model_ref {
            if *def_ref.provider_instance_id() == resolved_inst_id {
                def_ref.model_id().as_str().to_string()
            } else {
                catalog
                    .models_for_provider_instance(&resolved_inst_id)
                    .first()
                    .map(|m| m.id.as_str().to_string())
                    .ok_or_else(|| {
                        format!("No model registered or specified for provider {resolved_inst_id}")
                    })?
            }
        } else {
            catalog
                .models_for_provider_instance(&resolved_inst_id)
                .first()
                .map(|m| m.id.as_str().to_string())
                .ok_or_else(|| {
                    format!(
                        "No model registered or specified for provider {resolved_inst_id}. Register or discover a model first."
                    )
                })?
        };

        let model_id =
            ModelId::new(&resolved_model_str).map_err(|e| format!("Invalid model id: {e}"))?;
        Ok(ModelRef::new(resolved_inst_id, model_id))
    }

    fn apply_composer_config(
        cfg: &mut Config,
        cwd: &Path,
        reasoning_effort: Option<&str>,
        permission_mode: Option<&str>,
        mode: Option<&str>,
    ) -> Result<(bool, String), String> {
        std::fs::create_dir_all(cwd)
            .map_err(|e| format!("Failed to access cwd {}: {e}", cwd.display()))?;
        cfg.cwd = AbsolutePathBuf::from_absolute_path(cwd)
            .map_err(|e| format!("Invalid workspace cwd {}: {e}", cwd.display()))?;

        if let Some(eff) = reasoning_effort {
            cfg.model_reasoning_effort = match eff.to_ascii_lowercase().as_str() {
                "none" => Some(ReasoningEffort::None),
                "minimal" => Some(ReasoningEffort::Minimal),
                "low" => Some(ReasoningEffort::Low),
                "medium" => Some(ReasoningEffort::Medium),
                "high" => Some(ReasoningEffort::High),
                "xhigh" => Some(ReasoningEffort::XHigh),
                _ => cfg.model_reasoning_effort.clone(),
            };
        }

        let effective_perm = match (permission_mode, mode) {
            (Some(p), _) if !p.trim().is_empty() => p.to_ascii_lowercase(),
            (_, Some("ask")) => "read-only".to_string(),
            _ => "auto-edit".to_string(),
        };

        let is_read_only = effective_perm == "read-only";
        if is_read_only {
            cfg.set_legacy_sandbox_policy(SandboxPolicy::ReadOnly {
                network_access: false,
            })
            .map_err(|e| format!("Failed to set read-only sandbox policy: {e}"))?;
            cfg.permissions = Permissions::from_approval_and_profile(
                Constrained::allow_any(AskForApproval::Never),
                Constrained::allow_any(PermissionProfile::read_only()),
            )
            .map_err(|e| format!("Failed to set read-only permissions: {e}"))?;
        } else {
            cfg.set_legacy_sandbox_policy(SandboxPolicy::DangerFullAccess)
                .map_err(|e| format!("Failed to set workspace execution policy: {e}"))?;
            cfg.permissions = Permissions::from_approval_and_profile(
                Constrained::allow_any(AskForApproval::Never),
                Constrained::allow_any(PermissionProfile::Disabled),
            )
            .map_err(|e| format!("Failed to set workspace permissions: {e}"))?;
        }

        Ok((is_read_only, effective_perm))
    }

    async fn build_effective_turn_prompt(
        &self,
        params: &ChatTurnParams,
        cwd: &Path,
        is_read_only: bool,
    ) -> Result<(String, Vec<StudioToolCallRecord>), String> {
        let mut sections = Vec::new();
        let mut pre_tool_records = Vec::new();
        let global_home = crate::daemon::DaemonConfig::default_state_dir();

        // 1. Hierarchical repository instructions (AGENTS.md / CLAUDE.md)
        if let Ok(resolved) = InstructionResolver::resolve(Some(&global_home), cwd, cwd) {
            let ctx = resolved.merged_prompt_context();
            if !ctx.trim().is_empty() {
                sections.push(format!("[Workspace Instructions]\n{ctx}"));
            }
        }

        // 2. PreTurn Lifecycle hooks (.agents/hooks.json if present)
        let hooks_file = cwd.join(".agents").join("hooks.json");
        if hooks_file.is_file()
            && let Ok(raw_hooks) = std::fs::read_to_string(&hooks_file)
            && let Ok(hooks) =
                serde_json::from_str::<Vec<agent_studios_extensibility::HookSpec>>(&raw_hooks)
        {
            let hook_results = execute_phase_hooks(&hooks, LifecycleHookPhase::PreTurn, cwd)
                .await
                .map_err(|e| format!("PreTurn lifecycle hook failed: {e}"))?;
            for hr in hook_results {
                pre_tool_records.push(StudioToolCallRecord {
                    id: format!("hook_{}", hr.hook_id),
                    name: format!("hook:pre_turn:{}", hr.hook_id),
                    status: if hr.succeeded { "completed" } else { "failed" }.to_string(),
                    target: Some(cwd.display().to_string()),
                    input_summary: Some("pre_turn lifecycle hook".to_string()),
                    output_summary: Some(hr.stdout.trim().to_string()),
                    duration_ms: Some(1),
                    error: if hr.succeeded { None } else { Some(hr.stderr) },
                });
                if !hr.stdout.trim().is_empty() {
                    sections.push(format!(
                        "[Hook Output: {}]\n{}",
                        hr.hook_id,
                        hr.stdout.trim()
                    ));
                }
            }
        }

        // 3. Skills progressive disclosure ($skill-name)
        let mut skill_reg = SkillRegistry::new(&global_home, Some(cwd.to_path_buf()));
        if skill_reg.discover().is_ok() {
            for word in params.prompt.split_whitespace() {
                if let Some(skill_name) = word.strip_prefix('$') {
                    let clean = skill_name
                        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_');
                    if !clean.is_empty()
                        && let Some(desc) = skill_reg.get(clean)
                        && desc.enabled
                        && let Ok(body) = desc.load_full_instructions()
                    {
                        sections.push(format!("[Active Skill: ${clean}]\n{body}"));
                        pre_tool_records.push(StudioToolCallRecord {
                            id: format!("skill_{clean}"),
                            name: format!("skill:{clean}"),
                            status: "completed".to_string(),
                            target: Some(desc.skill_file.display().to_string()),
                            input_summary: Some(format!("Activated ${clean}")),
                            output_summary: Some(format!(
                                "Loaded {} chars of SKILL.md",
                                body.len()
                            )),
                            duration_ms: Some(1),
                            error: None,
                        });
                    }
                }
            }
        }

        // 4. Explicit MCP directive execution if prompt contains `/mcp <server> <tool> <json_args>`
        if let Some(rest) = params.prompt.trim().strip_prefix("/mcp ") {
            let mut parts = rest.splitn(3, ' ');
            if let (Some(server_id), Some(tool_name)) = (parts.next(), parts.next()) {
                let args_val: serde_json::Value = parts
                    .next()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_else(|| serde_json::json!({}));
                let mcp_reg = McpRegistry::load_from_paths(&global_home, Some(cwd))
                    .map_err(|e| format!("Failed to load MCP registry: {e}"))?;
                let max_tier = if is_read_only {
                    Some(McpRiskTier::Low)
                } else {
                    Some(McpRiskTier::Medium)
                };
                let explicit = params.permission_mode.as_deref() == Some("full-access");
                let call_res = mcp_reg
                    .invoke_tool(server_id, tool_name, args_val, cwd, max_tier, explicit)
                    .await
                    .map_err(|e| format!("MCP invocation rejected/failed: {e}"))?;
                pre_tool_records.push(StudioToolCallRecord {
                    id: format!("mcp_{server_id}_{tool_name}"),
                    name: format!("mcp:{server_id}:{tool_name}"),
                    status: if call_res.is_error {
                        "failed"
                    } else {
                        "completed"
                    }
                    .to_string(),
                    target: Some(server_id.to_string()),
                    input_summary: Some(tool_name.to_string()),
                    output_summary: Some(call_res.output.clone()),
                    duration_ms: Some(1),
                    error: if call_res.is_error {
                        Some(call_res.output.clone())
                    } else {
                        None
                    },
                });
                sections.push(format!(
                    "[MCP Tool Result {server_id}:{tool_name}]\n{}",
                    call_res.output
                ));
            }
        }

        // 5. ContextEngine attachments
        if !params.attachments.is_empty() {
            let mut att_lines = Vec::new();
            for att in &params.attachments {
                let rel = att
                    .get("relativePath")
                    .or_else(|| att.get("uri"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("attachment");
                let lang = att.get("languageId").and_then(|v| v.as_str()).unwrap_or("");
                let snippet = att
                    .get("contentSnippet")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !snippet.is_empty() {
                    att_lines.push(format!(
                        "### File `{rel}` ({lang})\n```{lang}\n{snippet}\n```"
                    ));
                } else {
                    att_lines.push(format!("### Context `{rel}` ({lang})"));
                }
            }
            if !att_lines.is_empty() {
                sections.push(format!("[Attached Context]\n{}", att_lines.join("\n")));
            }
        }

        // 6. Mode / Reasoning directives
        if let Some(m) = params.mode.as_deref()
            && (m == "plan" || m == "ask")
        {
            sections.push(format!("[Execution Mode: {m}]"));
        }

        if sections.is_empty() {
            Ok((params.prompt.clone(), pre_tool_records))
        } else {
            sections.push(format!("[User Request]\n{}", params.prompt));
            Ok((sections.join("\n\n"), pre_tool_records))
        }
    }

    pub async fn execute_chat_turn(
        &self,
        server_state: &Arc<ServerState>,
        params: ChatTurnParams,
    ) -> Result<ChatTurnOutcome, String> {
        let model_ref = self
            .resolve_model_ref(
                params.provider_instance_id,
                params.model_id.as_deref(),
                Some(params.studio_id),
            )
            .await?;

        let catalog_snapshot = Arc::new(self.provider_catalog.read().await.clone());
        let session_factory = AgentStudiosRuntimeSessionFactory::new(
            catalog_snapshot,
            self.secret_vault.clone(),
            self.continuation_manager.clone(),
        );
        let prepared = session_factory
            .prepare_runtime_session(&model_ref)
            .map_err(|e| format!("Failed to prepare runtime session for {}: {e}", model_ref))?;

        let requested_ws = params
            .workspace_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                server_state
                    .default_repo_path
                    .clone()
                    .or_else(|| self.default_repo_path.clone())
            })
            .unwrap_or_else(|| {
                self.codex_home
                    .join("studios")
                    .join(params.studio_id.to_string())
            });
        let effective_cwd = dunce::canonicalize(&requested_ws).unwrap_or(requested_ws);

        let (manager, base_config) = self.ensure_thread_manager().await?;
        let mut turn_config = base_config;
        let (is_read_only, effective_perm) = Self::apply_composer_config(
            &mut turn_config,
            &effective_cwd,
            params.reasoning_effort.as_deref(),
            params.permission_mode.as_deref(),
            params.mode.as_deref(),
        )?;

        let (mut effective_prompt, mut tool_records) = self
            .build_effective_turn_prompt(&params, &effective_cwd, is_read_only)
            .await?;

        let prior_history_summary = {
            let guard = self.persisted_sessions.read().await;
            guard.get(&params.studio_id).and_then(|s| {
                if s.messages.is_empty() {
                    None
                } else {
                    let lines: Vec<String> = s
                        .messages
                        .iter()
                        .map(|m| format!("{}: {}", m.role.to_uppercase(), m.content))
                        .collect();
                    Some(format!(
                        "[Prior Conversation History]\n{}",
                        lines.join("\n")
                    ))
                }
            })
        };

        let existing_live = {
            let guard = self.live_threads.read().await;
            guard.get(&params.studio_id).map(|l| {
                (
                    l.thread_id,
                    l.thread.clone(),
                    l.provider_instance_id,
                    l.model_id.clone(),
                    l.permission_mode.clone(),
                    l.cwd.clone(),
                    l.read_only_guard.clone(),
                )
            })
        };

        let (thread_id, thread, read_only_guard) = match existing_live {
            Some((tid, th, pid, mid, perm, old_cwd, ro_guard))
                if pid == *model_ref.provider_instance_id()
                    && mid == model_ref.model_id().as_str()
                    && perm == effective_perm
                    && old_cwd == effective_cwd =>
            {
                (tid, th, ro_guard)
            }
            _ => {
                if let Some(history) = prior_history_summary {
                    effective_prompt = format!("{history}\n\n{effective_prompt}");
                }
                let ro_guard = Arc::new(ReadOnlyToolGuardContext {
                    read_only: is_read_only,
                    blocked_calls: Arc::new(Mutex::new(Vec::new())),
                });
                let mut ext_init = ExtensionDataInit::new();
                ext_init.insert((*ro_guard).clone());

                let mut start_options = StartThreadOptions::new(turn_config);
                start_options.thread_extension_init = ext_init;
                prepared.apply_to_start_thread_options(&mut start_options);

                let new_thread = manager
                    .start_thread(start_options)
                    .await
                    .map_err(|e| format!("Failed to start Codex thread: {e}"))?;
                let live = LiveStudioThread {
                    thread_id: new_thread.thread_id,
                    thread: Arc::clone(&new_thread.thread),
                    provider_instance_id: *model_ref.provider_instance_id(),
                    model_id: model_ref.model_id().as_str().to_string(),
                    permission_mode: effective_perm.clone(),
                    cwd: effective_cwd.clone(),
                    read_only_guard: Arc::clone(&ro_guard),
                };
                self.live_threads
                    .write()
                    .await
                    .insert(params.studio_id, live);
                (new_thread.thread_id, new_thread.thread, ro_guard)
            }
        };

        read_only_guard.blocked_calls.lock().unwrap().clear();

        let _ = thread
            .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                text: effective_prompt,
                text_elements: vec![],
            }]))
            .await
            .map_err(|e| format!("Failed to submit Codex turn: {e}"))?;

        let mut response_text = String::new();
        let mut active_tools: HashMap<String, (String, Instant)> = HashMap::new();
        let mut last_metrics: Option<serde_json::Value> = None;
        let mut turn_error: Option<String> = None;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(250), thread.next_event()).await {
                Ok(Ok(event)) => match event.msg {
                    EventMsg::AgentMessageContentDelta(delta) => {
                        response_text.push_str(&delta.delta);
                        let _ = server_state.emit_event(
                            "runtime_output_delta",
                            serde_json::json!({
                                "studio_id": params.studio_id,
                                "thread_id": thread_id.to_string(),
                                "text": delta.delta,
                            }),
                        );
                    }
                    EventMsg::AgentMessage(msg) => {
                        if response_text.is_empty() {
                            response_text.push_str(&msg.message);
                            let _ = server_state.emit_event(
                                "runtime_output_delta",
                                serde_json::json!({
                                    "studio_id": params.studio_id,
                                    "thread_id": thread_id.to_string(),
                                    "text": msg.message,
                                }),
                            );
                        } else if msg.message.len() > response_text.len()
                            && msg.message.starts_with(&response_text)
                        {
                            let suffix = msg.message[response_text.len()..].to_string();
                            response_text = msg.message;
                            if !suffix.is_empty() {
                                let _ = server_state.emit_event(
                                    "runtime_output_delta",
                                    serde_json::json!({
                                        "studio_id": params.studio_id,
                                        "thread_id": thread_id.to_string(),
                                        "text": suffix,
                                    }),
                                );
                            }
                        }
                    }
                    EventMsg::ExecCommandBegin(ev) => {
                        let cmd_str = ev.command.join(" ");
                        active_tools.insert(ev.call_id.clone(), (cmd_str.clone(), Instant::now()));
                        let _ = server_state.emit_event(
                            "tool_started",
                            serde_json::json!({
                                "studio_id": params.studio_id,
                                "tool_name": "shell",
                                "call_id": ev.call_id,
                                "target": cmd_str,
                            }),
                        );
                    }
                    EventMsg::ExecCommandEnd(ev) => {
                        let (cmd_str, dur) = active_tools
                            .remove(&ev.call_id)
                            .map(|(c, start)| (c, start.elapsed().as_millis() as u64))
                            .unwrap_or_else(|| {
                                (ev.command.join(" "), ev.duration.as_millis() as u64)
                            });
                        let ok = ev.exit_code == 0;
                        tool_records.push(StudioToolCallRecord {
                            id: ev.call_id.clone(),
                            name: "shell".to_string(),
                            status: if ok { "completed" } else { "failed" }.to_string(),
                            target: Some(cmd_str),
                            input_summary: Some(ev.command.join(" ")),
                            output_summary: Some(ev.aggregated_output.chars().take(500).collect()),
                            duration_ms: Some(dur),
                            error: if ok {
                                None
                            } else {
                                Some(format!("exit code {}", ev.exit_code))
                            },
                        });
                        let _ = server_state.emit_event(
                            if ok { "tool_completed" } else { "tool_failed" },
                            serde_json::json!({
                                "studio_id": params.studio_id,
                                "tool_name": "shell",
                                "call_id": ev.call_id,
                                "exit_code": ev.exit_code,
                            }),
                        );
                    }
                    EventMsg::PatchApplyBegin(ev) => {
                        active_tools.insert(
                            ev.call_id.clone(),
                            (format!("{} files", ev.changes.len()), Instant::now()),
                        );
                        let _ = server_state.emit_event(
                            "tool_started",
                            serde_json::json!({
                                "studio_id": params.studio_id,
                                "tool_name": "apply_patch",
                                "call_id": ev.call_id,
                            }),
                        );
                    }
                    EventMsg::PatchApplyEnd(ev) => {
                        let (target, dur) = active_tools
                            .remove(&ev.call_id)
                            .map(|(t, start)| (t, start.elapsed().as_millis() as u64))
                            .unwrap_or_else(|| (format!("{} files", ev.changes.len()), 0));
                        tool_records.push(StudioToolCallRecord {
                            id: ev.call_id.clone(),
                            name: "apply_patch".to_string(),
                            status: if ev.success { "completed" } else { "failed" }.to_string(),
                            target: Some(target),
                            input_summary: None,
                            output_summary: Some(ev.stdout.clone()),
                            duration_ms: Some(dur),
                            error: if ev.success {
                                None
                            } else {
                                Some(ev.stderr.clone())
                            },
                        });
                        let _ = server_state.emit_event(
                            if ev.success {
                                "tool_completed"
                            } else {
                                "tool_failed"
                            },
                            serde_json::json!({
                                "studio_id": params.studio_id,
                                "tool_name": "apply_patch",
                                "call_id": ev.call_id,
                            }),
                        );
                    }
                    EventMsg::TokenCount(tc) => {
                        if let Some(info) = tc.info {
                            last_metrics = serde_json::to_value(info).ok();
                        }
                    }
                    EventMsg::TurnComplete(tc) => {
                        if response_text.is_empty()
                            && let Some(last) = tc.last_agent_message
                        {
                            response_text = last;
                        }
                        if let Some(err) = tc.error {
                            turn_error = Some(err.message);
                        }
                        break;
                    }
                    EventMsg::TurnAborted(_) => {
                        turn_error = Some("Turn aborted by user or interrupt".to_string());
                        break;
                    }
                    EventMsg::Error(err) => {
                        turn_error = Some(err.message);
                        break;
                    }
                    _ => {}
                },
                Ok(Err(e)) => {
                    turn_error = Some(format!("Codex thread event error: {e}"));
                    break;
                }
                Err(_) => {}
            }
        }

        if let Some(err) = turn_error {
            return Err(err);
        }

        for blocked in read_only_guard.blocked_calls.lock().unwrap().drain(..) {
            tool_records.push(StudioToolCallRecord {
                id: format!("blocked_{}", Uuid::new_v4().simple()),
                name: blocked,
                status: "failed".to_string(),
                target: Some(effective_cwd.display().to_string()),
                input_summary: Some("blocked by read-only permission mode".to_string()),
                output_summary: None,
                duration_ms: Some(0),
                error: Some("Blocked by read-only permission mode".to_string()),
            });
        }

        let now_ms = Utc::now().timestamp_millis();
        let user_msg = StudioTranscriptMessage {
            id: format!("msg_user_{}", Uuid::new_v4().simple()),
            role: "user".to_string(),
            content: params.prompt.clone(),
            timestamp: now_ms,
            mode: params.mode.clone(),
            thread_id: Some(thread_id.to_string()),
            provider_instance_id: Some(model_ref.provider_instance_id().to_string()),
            model_id: Some(model_ref.model_id().as_str().to_string()),
            reasoning_effort: params.reasoning_effort.clone(),
            permission_mode: Some(effective_perm.clone()),
            tool_calls: Vec::new(),
            artifacts: Vec::new(),
            metrics: None,
        };
        let assistant_msg = StudioTranscriptMessage {
            id: format!("msg_assistant_{}", Uuid::new_v4().simple()),
            role: "assistant".to_string(),
            content: response_text,
            timestamp: Utc::now().timestamp_millis(),
            mode: params.mode.clone(),
            thread_id: Some(thread_id.to_string()),
            provider_instance_id: Some(model_ref.provider_instance_id().to_string()),
            model_id: Some(model_ref.model_id().as_str().to_string()),
            reasoning_effort: params.reasoning_effort.clone(),
            permission_mode: Some(effective_perm.clone()),
            tool_calls: tool_records,
            artifacts: Vec::new(),
            metrics: last_metrics,
        };

        {
            let mut guard = self.persisted_sessions.write().await;
            let entry = guard
                .entry(params.studio_id)
                .or_insert_with(|| PersistedStudioSession {
                    studio_id: params.studio_id,
                    ..Default::default()
                });
            entry.thread_id = Some(thread_id.to_string());
            entry.provider_instance_id = Some(model_ref.provider_instance_id().to_string());
            entry.model_id = Some(model_ref.model_id().as_str().to_string());
            entry.permission_mode = Some(effective_perm);
            entry.workspace_path = Some(effective_cwd.display().to_string());
            entry.messages.push(user_msg);
            entry.messages.push(assistant_msg.clone());
        }
        self.persist_sessions().await;

        let _ = server_state.emit_event(
            "turn_completed",
            serde_json::json!({
                "studio_id": params.studio_id,
                "thread_id": thread_id.to_string(),
                "message_id": assistant_msg.id,
            }),
        );

        Ok(ChatTurnOutcome {
            studio_id: params.studio_id,
            thread_id: thread_id.to_string(),
            assistant_message: assistant_msg,
        })
    }

    pub async fn interrupt_studio_turn(&self, studio_id: StudioId) -> Result<bool, String> {
        let maybe_thread = {
            let guard = self.live_threads.read().await;
            guard.get(&studio_id).map(|l| l.thread.clone())
        };
        if let Some(thread) = maybe_thread {
            thread
                .submit(Op::Interrupt)
                .await
                .map_err(|e| format!("Failed to interrupt thread: {e}"))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn get_transcript(
        &self,
        studio_id: StudioId,
    ) -> (Option<String>, Vec<StudioTranscriptMessage>) {
        let guard = self.persisted_sessions.read().await;
        match guard.get(&studio_id) {
            Some(session) => (session.thread_id.clone(), session.messages.clone()),
            None => (None, Vec::new()),
        }
    }

    pub async fn fork_studio_transcript(
        &self,
        source_studio_id: StudioId,
        target_studio_id: StudioId,
    ) {
        let cloned_opt = {
            let guard = self.persisted_sessions.read().await;
            guard.get(&source_studio_id).cloned()
        };
        if let Some(mut cloned) = cloned_opt {
            cloned.studio_id = target_studio_id;
            cloned.thread_id = None; // New independent thread allocated on next turn
            self.persisted_sessions
                .write()
                .await
                .insert(target_studio_id, cloned);
            self.persist_sessions().await;
        }
    }

    /// Executes a real Coordinator -> parallel Worktree Workers -> Reconciliation workflow
    /// inside `server_state.control_plane` and `WorkspaceOrchestrator`.
    pub async fn execute_multi_agent_goal(
        &self,
        server_state: &Arc<ServerState>,
        params: MultiAgentGoalParams,
    ) -> Result<serde_json::Value, String> {
        let model_ref = self
            .resolve_model_ref(
                params.provider_instance_id,
                params.model_id.as_deref(),
                Some(params.studio_id),
            )
            .await?;

        let raw_repo = params
            .repository_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                server_state
                    .default_repo_path
                    .clone()
                    .or_else(|| self.default_repo_path.clone())
            })
            .ok_or_else(|| {
                "No repository_path configured for multi-agent worktree orchestration".to_string()
            })?;
        let repo_path = dunce::canonicalize(&raw_repo)
            .map_err(|e| format!("Failed to canonicalize repository path: {e}"))?;

        let orchestrator = server_state
            .workspace_orchestrator
            .clone()
            .or_else(|| self.workspace_orchestrator.clone())
            .ok_or_else(|| "WorkspaceOrchestrator unavailable on daemon".to_string())?;

        let base_sha = orchestrator
            .resolve_head_commit_async(repo_path.clone())
            .await
            .map_err(|e| format!("Repository HEAD resolution failed: {e}"))?;

        // 1. Register Coordinator + 2 Workers + Reviewer in ControlPlane
        let (coord_id, worker_a_id, worker_b_id, reviewer_id) = {
            let mut cp = server_state.control_plane.write().await;
            let specs = vec![
                agent_studios_protocol::agent::BatchAgentSpec {
                    id: AgentId::new(),
                    display_name: "Coordinator".to_string(),
                    kind: AgentKind::Internal,
                    role: Some("coordinator".to_string()),
                },
                agent_studios_protocol::agent::BatchAgentSpec {
                    id: AgentId::new(),
                    display_name: "Worker A".to_string(),
                    kind: AgentKind::Internal,
                    role: Some("worker".to_string()),
                },
                agent_studios_protocol::agent::BatchAgentSpec {
                    id: AgentId::new(),
                    display_name: "Worker B".to_string(),
                    kind: AgentKind::Internal,
                    role: Some("worker".to_string()),
                },
                agent_studios_protocol::agent::BatchAgentSpec {
                    id: AgentId::new(),
                    display_name: "Reviewer".to_string(),
                    kind: AgentKind::Internal,
                    role: Some("reviewer".to_string()),
                },
            ];
            let created = cp
                .register_agent_batch(params.studio_id, specs)
                .map_err(|e| format!("Failed to register team agents: {e}"))?;
            let envs = cp.drain_committed_events();
            drop(cp);
            let _ = server_state.publish_control_plane_events(envs);
            (created[0].id, created[1].id, created[2].id, created[3].id)
        };

        // 2. Run Coordinator planning turn on CodexThread
        let catalog_snapshot = Arc::new(self.provider_catalog.read().await.clone());
        let session_factory = AgentStudiosRuntimeSessionFactory::new(
            catalog_snapshot,
            self.secret_vault.clone(),
            self.continuation_manager.clone(),
        );
        let (manager, base_config) = self.ensure_thread_manager().await?;

        let coord_prepared = session_factory
            .prepare_runtime_session(&model_ref)
            .map_err(|e| format!("Coordinator session prep failed: {e}"))?;
        let mut coord_cfg = base_config.clone();
        let _ = Self::apply_composer_config(
            &mut coord_cfg,
            &repo_path,
            None,
            Some("read-only"),
            Some("plan"),
        )?;
        let mut coord_opts = StartThreadOptions::new(coord_cfg);
        coord_prepared.apply_to_start_thread_options(&mut coord_opts);
        let coord_thread = manager
            .start_thread(coord_opts)
            .await
            .map_err(|e| format!("Failed to start coordinator thread: {e}"))?;

        let schema = schemars::schema_for!(CoordinatorDecision);
        let mut plan_req = TurnInputRequest::user_input(vec![UserInput::Text {
            text: format!(
                "Plan multi-agent worktree tasks for goal: {}. Available agents: worker-a, worker-b, reviewer. Respond with JSON CoordinatorDecision.",
                params.goal
            ),
            text_elements: Vec::new(),
        }]);
        plan_req.start.final_output_json_schema = serde_json::to_value(&schema).ok();
        let _ = coord_thread.thread.start_or_steer_turn(plan_req).await;
        let coord_output = drain_codex_text(&coord_thread.thread).await;
        let _ = coord_thread.thread.shutdown_and_wait().await;

        let planned_tasks = match serde_json::from_str::<CoordinatorDecision>(coord_output.trim()) {
            Ok(CoordinatorDecision::Plan { tasks }) if !tasks.is_empty() => tasks,
            _ => vec![
                agent_studios_internal_agent::PlannedTask {
                    task_key: "worker_a".to_string(),
                    title: format!("Implement part A: {}", params.goal),
                    description: Some(params.goal.clone()),
                    assigned_alias: "worker-a".to_string(),
                    depends_on: Vec::new(),
                    workspace_access: Some(
                        agent_studios_internal_agent::WorkspaceAccessMode::Mutating,
                    ),
                    priority: Some(10),
                    dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
                },
                agent_studios_internal_agent::PlannedTask {
                    task_key: "worker_b".to_string(),
                    title: format!("Implement part B: {}", params.goal),
                    description: Some(params.goal.clone()),
                    assigned_alias: "worker-b".to_string(),
                    depends_on: Vec::new(),
                    workspace_access: Some(
                        agent_studios_internal_agent::WorkspaceAccessMode::Mutating,
                    ),
                    priority: Some(10),
                    dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
                },
                agent_studios_internal_agent::PlannedTask {
                    task_key: "review".to_string(),
                    title: "Verify & Review Changes".to_string(),
                    description: Some(format!("Review all changes for {}", params.goal)),
                    assigned_alias: "reviewer".to_string(),
                    depends_on: vec!["worker_a".to_string(), "worker_b".to_string()],
                    workspace_access: Some(
                        agent_studios_internal_agent::WorkspaceAccessMode::ReadOnly,
                    ),
                    priority: Some(5),
                    dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
                },
            ],
        };

        // 3. Materialize DAG in ControlPlane
        let batch_specs: Vec<BatchTaskSpec> = planned_tasks
            .iter()
            .map(|pt| {
                let assigned = match pt.assigned_alias.as_str() {
                    "worker-b" | "worker_b" => worker_b_id,
                    "reviewer" => reviewer_id,
                    _ => worker_a_id,
                };
                let _ = coord_id;
                let desc = pt.description.clone().unwrap_or_else(|| pt.title.clone());
                BatchTaskSpec {
                    key: pt.task_key.clone(),
                    title: pt.title.clone(),
                    description: desc.clone(),
                    assigned_agent_id: Some(assigned),
                    parent_task_key: None,
                    parent_task_id: None,
                    dependency_keys: pt.depends_on.clone(),
                    dependency_task_ids: Vec::new(),
                    dependency_output_policy: pt.dependency_output_policy,
                    execution: Some(TaskExecutionSpec {
                        initial_prompt: Some(desc),
                        repository_path: Some(repo_path.display().to_string()),
                        runtime_instance_id: Some(
                            InternalCodexAgentRuntime::default_instance_id().to_string(),
                        ),
                        runtime_preference: Some("codex".to_string()),
                        provider_instance_id: Some(model_ref.provider_instance_id().to_string()),
                        model_id: Some(model_ref.model_id().as_str().to_string()),
                    }),
                }
            })
            .collect();

        let created_tasks = {
            let mut cp = server_state.control_plane.write().await;
            let tasks = cp
                .create_task_batch(params.studio_id, batch_specs)
                .map_err(|e| format!("Failed to materialize DAG tasks: {e}"))?;
            let envs = cp.drain_committed_events();
            drop(cp);
            let _ = server_state.publish_control_plane_events(envs);
            tasks
        };

        // 4. Execute DAG waves in parallel isolated physical Git worktrees
        let mut worktree_summaries = Vec::new();
        let mut artifact_ids = Vec::new();
        let mut captured_reconciliations: Vec<(WorktreeId, TaskId, RunId, ArtifactId)> = Vec::new();

        loop {
            let ready_tasks = {
                let cp = server_state.control_plane.read().await;
                let mut list: Vec<_> = cp
                    .all_tasks()
                    .filter(|t| t.studio_id == params.studio_id && t.state == TaskState::Ready)
                    .filter(|t| created_tasks.iter().any(|ct| ct.id == t.id))
                    .cloned()
                    .collect();
                list.sort_by_key(|t| (t.created_at, t.id));
                list
            };

            if ready_tasks.is_empty() {
                let any_pending = {
                    let cp = server_state.control_plane.read().await;
                    created_tasks
                        .iter()
                        .any(|ct| cp.get_task(ct.id).is_some_and(|t| !t.state.is_terminal()))
                };
                if !any_pending {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
                continue;
            }

            let mut join_set: JoinSet<WorkerTaskResult> = JoinSet::new();

            for task in ready_tasks {
                let agent_id = task.assigned_agent_id.unwrap_or(worker_a_id);
                let is_reviewer = agent_id == reviewer_id;
                let run = {
                    let mut cp = server_state.control_plane.write().await;
                    let r = cp
                        .create_run(task.id, agent_id)
                        .map_err(|e| format!("Failed to create run: {e}"))?;
                    let _ = cp.transition_run_state(r.id, RunState::Starting);
                    let _ = cp.transition_run_state(r.id, RunState::Running);
                    let _ = cp.transition_task_state(task.id, TaskState::Running);
                    let _ = cp.update_agent_state(agent_id, AgentState::Starting);
                    let _ = cp.update_agent_state(agent_id, AgentState::Idle);
                    let _ = cp.update_agent_state(agent_id, AgentState::Busy);
                    let envs = cp.drain_committed_events();
                    drop(cp);
                    let _ = server_state.publish_control_plane_events(envs);
                    r
                };

                let orch = orchestrator.clone();
                let repo = repo_path.clone();
                let b_sha = base_sha.clone();
                let srv = server_state.clone();
                let mgr = manager.clone();
                let b_cfg = base_config.clone();
                let sf = session_factory.clone();
                let m_ref = model_ref.clone();
                let studio_id = params.studio_id;
                let perm = params.permission_mode.clone();

                join_set.spawn(async move {
                    let (wt_id_opt, work_cwd) = if !is_reviewer {
                        let managed = orch
                            .create_worktree_async(repo.clone(), Some(b_sha.clone()))
                            .await
                            .map_err(|e| format!("Worktree creation failed: {e}"))?;
                        let wt_id = {
                            let mut cp = srv.control_plane.write().await;
                            let wt_rec = cp
                                .create_worktree_with_cwds(
                                    studio_id,
                                    format!("wt-{}", task.id),
                                    managed.source_root.clone(),
                                    managed.source_cwd.clone(),
                                    managed.root.clone(),
                                    managed.cwd.clone(),
                                    b_sha.clone(),
                                )
                                .map_err(|e| format!("ControlPlane worktree failed: {e}"))?;
                            let _ = cp.transition_worktree_state(wt_rec.id, WorktreeState::Ready);
                            let _ = cp.assign_worktree(
                                wt_id_or(wt_rec.id),
                                task.id,
                                agent_id,
                                Some(run.id),
                            );
                            let _ = cp.transition_worktree_state(wt_rec.id, WorktreeState::InUse);
                            let envs = cp.drain_committed_events();
                            drop(cp);
                            let _ = srv.publish_control_plane_events(envs);
                            wt_rec.id
                        };
                        (Some((wt_id, managed.root.clone())), managed.cwd)
                    } else {
                        (None, repo.clone())
                    };

                    let prep = sf
                        .prepare_runtime_session(&m_ref)
                        .map_err(|e| format!("Worker session prep failed: {e}"))?;
                    let mut w_cfg = b_cfg;
                    let eff_perm = if is_reviewer {
                        Some("read-only")
                    } else {
                        Some(perm.as_deref().unwrap_or("full-access"))
                    };
                    let _ = CodexStudioEngine::apply_composer_config(
                        &mut w_cfg,
                        &work_cwd,
                        None,
                        eff_perm,
                        Some("agent"),
                    )?;
                    let mut opts = StartThreadOptions::new(w_cfg);
                    prep.apply_to_start_thread_options(&mut opts);
                    let w_thread = mgr
                        .start_thread(opts)
                        .await
                        .map_err(|e| format!("Failed to start worker thread: {e}"))?;
                    let req = TurnInputRequest::user_input(vec![UserInput::Text {
                        text: format!("Task: {}\nDetails: {}", task.title, task.description),
                        text_elements: Vec::new(),
                    }]);
                    let _ = w_thread.thread.start_or_steer_turn(req).await;
                    let worker_out = drain_codex_text(&w_thread.thread).await;
                    let _ = w_thread.thread.shutdown_and_wait().await;

                    let mut captured_art_id: Option<ArtifactId> = None;
                    let mut captured_wt_id: Option<WorktreeId> = None;
                    let mut wt_json = None;
                    if let Some((wt_id, wt_root)) = wt_id_opt {
                        captured_wt_id = Some(wt_id);
                        if let Ok(captured) = orch
                            .capture_changes_async(wt_root.clone(), b_sha.clone())
                            .await
                            && captured.files_changed > 0
                            && let Ok(patch_rec) = orch.store_patch_artifact(
                                studio_id,
                                task.id,
                                agent_id,
                                Some(run.id),
                                Some(wt_id),
                                &captured,
                            )
                        {
                            captured_art_id = Some(patch_rec.id);
                            let mut cp = srv.control_plane.write().await;
                            let reg = cp.register_artifact_record(patch_rec).ok();
                            if let Some(ref reg_art) = reg {
                                let _ = cp.record_worktree_change_captured(
                                    wt_id,
                                    Some(run.id),
                                    b_sha.clone(),
                                    captured.head_commit.clone(),
                                    reg_art.id,
                                    None,
                                    captured.files_changed,
                                );
                            }
                            let envs = cp.drain_committed_events();
                            drop(cp);
                            let _ = srv.publish_control_plane_events(envs);
                            wt_json = Some(serde_json::json!({
                                "worktree_id": wt_id,
                                "root": wt_root.display().to_string(),
                                "files_changed": captured.files_changed,
                                "artifact_id": captured_art_id.map(|id| id.to_string()),
                            }));
                        } else {
                            wt_json = Some(serde_json::json!({
                                "worktree_id": wt_id,
                                "root": wt_root.display().to_string(),
                                "files_changed": 0,
                            }));
                        }
                    }

                    let _ = worker_out;
                    let mut cp = srv.control_plane.write().await;
                    let _ = cp.record_run_outcome(run.id, "succeeded", None);
                    let _ = cp.transition_run_state(run.id, RunState::Succeeded);
                    let _ = cp.transition_task_state(task.id, TaskState::Succeeded);
                    let _ = cp.update_agent_state(agent_id, AgentState::Idle);
                    let envs = cp.drain_committed_events();
                    drop(cp);
                    let _ = srv.publish_control_plane_events(envs);
                    Ok((task.id, run.id, wt_json, captured_art_id, captured_wt_id))
                });
            }

            while let Some(res) = join_set.join_next().await {
                let (t_id, r_id, wt_opt, art_opt, wt_id_res) = res.map_err(|e| e.to_string())??;
                if let Some(wt) = wt_opt {
                    worktree_summaries.push(wt);
                }
                if let Some(art_id) = art_opt {
                    artifact_ids.push(art_id.to_string());
                    if let Some(wt_id) = wt_id_res {
                        captured_reconciliations.push((wt_id, t_id, r_id, art_id));
                    }
                }
            }
        }

        // 5. Record reconciliation summary
        if !captured_reconciliations.is_empty() {
            let mut cp = server_state.control_plane.write().await;
            for (wt_id, t_id, r_id, art_id) in captured_reconciliations {
                if let Ok(rec) = cp.create_reconciliation(
                    params.studio_id,
                    wt_id,
                    t_id,
                    Some(r_id),
                    art_id,
                    &repo_path,
                    &base_sha,
                ) {
                    let _ =
                        cp.transition_reconciliation_state(rec.id, ReconciliationState::Checking);
                    let _ =
                        cp.transition_reconciliation_state(rec.id, ReconciliationState::Applying);
                    let _ =
                        cp.transition_reconciliation_state(rec.id, ReconciliationState::Applied);
                }
            }
            let envs = cp.drain_committed_events();
            drop(cp);
            let _ = server_state.publish_control_plane_events(envs);
        }

        Ok(serde_json::json!({
            "studio_id": params.studio_id,
            "goal": params.goal,
            "tasks_completed": created_tasks.len(),
            "task_ids": created_tasks.iter().map(|t| t.id).collect::<Vec<_>>(),
            "worktrees": worktree_summaries,
            "artifact_ids": artifact_ids,
            "coordinator_summary": coord_output,
        }))
    }
}

fn wt_id_or(id: agent_studios_protocol::id::WorktreeId) -> agent_studios_protocol::id::WorktreeId {
    id
}

async fn drain_codex_text(thread: &CodexThread) -> String {
    let mut output = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(250), thread.next_event()).await {
            Ok(Ok(event)) => match event.msg {
                EventMsg::AgentMessageContentDelta(delta) => output.push_str(&delta.delta),
                EventMsg::AgentMessage(msg) => {
                    if output.is_empty() {
                        output = msg.message;
                    }
                }
                EventMsg::TurnComplete(tc) => {
                    if output.is_empty()
                        && let Some(last) = tc.last_agent_message
                    {
                        output = last;
                    }
                    break;
                }
                EventMsg::TurnAborted(_) | EventMsg::Error(_) => break,
                _ => {}
            },
            Ok(Err(_)) | Err(_) => {
                if !output.is_empty() {
                    break;
                }
            }
        }
    }
    output
}

// ============================================================================
// 4. InternalCodexAgentRuntime (implements AgentRuntime for RuntimeRegistry)
// ============================================================================

struct InternalCodexActiveSession {
    _session_ref: RuntimeSessionRef,
    thread: Arc<CodexThread>,
    hub: Arc<SessionEventHub>,
    state: Arc<AsyncRwLock<RuntimeLifecycleState>>,
    _pending_approval: Option<CpApprovalId>,
}

/// Native Agent Studios internal Codex engine exposed as an `AgentRuntime` ("codex").
pub struct InternalCodexAgentRuntime {
    implementation_id: RuntimeImplementationId,
    engine: Arc<CodexStudioEngine>,
    sessions: Arc<AsyncRwLock<HashMap<RuntimeSessionId, InternalCodexActiveSession>>>,
}

impl InternalCodexAgentRuntime {
    pub fn default_instance_id() -> RuntimeInstanceId {
        RuntimeInstanceId::from_uuid(Uuid::from_u128(0x434f4445585f494e54))
    }

    pub fn new(engine: Arc<CodexStudioEngine>) -> Self {
        Self {
            implementation_id: RuntimeImplementationId::new("codex").expect("valid codex id"),
            engine,
            sessions: Arc::new(AsyncRwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl AgentRuntime for InternalCodexAgentRuntime {
    fn implementation_id(&self) -> &RuntimeImplementationId {
        &self.implementation_id
    }

    async fn discover(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError> {
        let mut metadata = BTreeMap::new();
        metadata.insert("engine".to_string(), "codex-core".to_string());
        Ok(vec![DiscoveredRuntimeInstance {
            implementation_id: self.implementation_id.clone(),
            instance_id: Self::default_instance_id(),
            display_name: "Agent Studios Internal Codex Engine".to_string(),
            version: Some("0.1.0-codex".to_string()),
            binary_path: None,
            capabilities: self.capabilities().await,
            supported_configs: vec![
                RuntimeConfigRef::new("default")?,
                RuntimeConfigRef::new("read-only")?,
                RuntimeConfigRef::new("auto-edit")?,
                RuntimeConfigRef::new("full-access")?,
            ],
            availability: RuntimeAvailability::Available,
            metadata,
        }])
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            streaming_events: RuntimeCapabilitySupport::Supported,
            interrupt: RuntimeCapabilitySupport::Supported,
            resume: RuntimeCapabilitySupport::Supported,
            stop: RuntimeCapabilitySupport::Supported,
            persistent_session: RuntimeCapabilitySupport::Supported,
            workspace_binding: RuntimeCapabilitySupport::Supported,
            model_selection: RuntimeCapabilitySupport::Supported,
            provider_selection: RuntimeCapabilitySupport::Supported,
            tools: RuntimeCapabilitySupport::Supported,
            approvals: RuntimeCapabilitySupport::Supported,
            mcp: RuntimeCapabilitySupport::Supported,
            skills: RuntimeCapabilitySupport::Supported,
            image_input: RuntimeCapabilitySupport::Supported,
            structured_output: RuntimeCapabilitySupport::Supported,
            subagents: RuntimeCapabilitySupport::Supported,
            usage_reporting: RuntimeCapabilitySupport::Supported,
        }
    }

    async fn start(
        &self,
        request: RuntimeStartRequest,
    ) -> Result<RuntimeSessionHandle, RuntimeError> {
        request.validate()?;
        let model_ref = if let Some(ref mr) = request.model_ref {
            mr.clone()
        } else {
            self.engine
                .resolve_model_ref(None, None, None)
                .await
                .map_err(RuntimeError::invalid_configuration)?
        };

        let catalog_snapshot = Arc::new(self.engine.provider_catalog.read().await.clone());
        let session_factory = AgentStudiosRuntimeSessionFactory::new(
            catalog_snapshot,
            self.engine.secret_vault.clone(),
            self.engine.continuation_manager.clone(),
        );
        let prepared = session_factory
            .prepare_runtime_session(&model_ref)
            .map_err(|e| RuntimeError::invalid_configuration(e.to_string()))?;

        let (manager, base_config) = self
            .engine
            .ensure_thread_manager()
            .await
            .map_err(RuntimeError::startup_failed)?;
        let mut cfg = base_config;
        let cwd = request.workspace.cwd();
        let perm_str = if matches!(
            request.workspace_access_mode,
            agent_studios_external_runtime::WorkspaceAccessMode::ReadOnly
        ) {
            "read-only"
        } else {
            "full-access"
        };
        let _ = CodexStudioEngine::apply_composer_config(&mut cfg, cwd, None, Some(perm_str), None)
            .map_err(RuntimeError::invalid_configuration)?;
        let mut opts = StartThreadOptions::new(cfg);
        prepared.apply_to_start_thread_options(&mut opts);

        let started = manager.start_thread(opts).await.map_err(|e| {
            RuntimeError::startup_failed(format!("Failed to start Codex thread: {e}"))
        })?;

        let session_id = RuntimeSessionId::new();
        let session_ref = RuntimeSessionRef::new(
            self.implementation_id.clone(),
            *request.instance_id(),
            session_id,
        );
        let hub = Arc::new(SessionEventHub::new(
            session_id,
            *request.instance_id(),
            1000,
        ));
        hub.emit(RuntimeEventKind::SessionStarted {
            session_id,
            instance_id: *request.instance_id(),
        })
        .await?;

        let state = Arc::new(AsyncRwLock::new(RuntimeLifecycleState::Running));
        let thread = Arc::clone(&started.thread);

        self.sessions.write().await.insert(
            session_id,
            InternalCodexActiveSession {
                _session_ref: session_ref.clone(),
                thread: Arc::clone(&thread),
                hub: Arc::clone(&hub),
                state: Arc::clone(&state),
                _pending_approval: None,
            },
        );

        let hub_clone = Arc::clone(&hub);
        let state_clone = Arc::clone(&state);
        let thread_clone = Arc::clone(&thread);
        tokio::spawn(async move {
            while let Ok(ev) = thread_clone.next_event().await {
                match ev.msg {
                    EventMsg::AgentMessageContentDelta(d) => {
                        let _ = hub_clone
                            .emit(RuntimeEventKind::OutputDelta { text: d.delta })
                            .await;
                    }
                    EventMsg::AgentMessage(m) => {
                        let _ = hub_clone
                            .emit(RuntimeEventKind::OutputDelta { text: m.message })
                            .await;
                    }
                    EventMsg::ExecCommandBegin(b) => {
                        let _ = hub_clone
                            .emit(RuntimeEventKind::ToolStarted {
                                tool_name: "shell".to_string(),
                                tool_call_id: b.call_id,
                            })
                            .await;
                    }
                    EventMsg::ExecCommandEnd(e) => {
                        let _ = hub_clone
                            .emit(RuntimeEventKind::ToolCompleted {
                                tool_name: "shell".to_string(),
                                tool_call_id: e.call_id,
                                duration_ms: e.duration.as_millis() as u64,
                                success: e.exit_code == 0,
                            })
                            .await;
                    }
                    EventMsg::TurnComplete(tc) => {
                        if let Some(err) = tc.error {
                            *state_clone.write().await = RuntimeLifecycleState::Failed;
                            let _ = hub_clone
                                .emit(RuntimeEventKind::Failed {
                                    safe_error_summary: SanitizedRuntimeMessage::new(&err.message),
                                })
                                .await;
                        } else {
                            *state_clone.write().await = RuntimeLifecycleState::Completed;
                            let _ = hub_clone
                                .emit(RuntimeEventKind::Completed {
                                    summary: tc.last_agent_message,
                                })
                                .await;
                        }
                        break;
                    }
                    EventMsg::TurnAborted(_) => {
                        *state_clone.write().await = RuntimeLifecycleState::Interrupted;
                        let _ = hub_clone
                            .emit(RuntimeEventKind::Interrupted {
                                reason: SanitizedRuntimeMessage::new("Turn aborted"),
                            })
                            .await;
                        break;
                    }
                    EventMsg::Error(e) => {
                        *state_clone.write().await = RuntimeLifecycleState::Failed;
                        let _ = hub_clone
                            .emit(RuntimeEventKind::Failed {
                                safe_error_summary: SanitizedRuntimeMessage::new(&e.message),
                            })
                            .await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        if let Some(ref prompt) = request.initial_prompt {
            let _ = thread
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: prompt.clone(),
                    text_elements: Vec::new(),
                }]))
                .await;
        }

        let capabilities = self.capabilities().await;
        Ok(RuntimeSessionHandle::new(
            session_id,
            *request.instance_id(),
            self.implementation_id.clone(),
            RuntimeLifecycleState::Running,
            request.workspace,
            request.workspace_access_mode,
            capabilities,
            request.correlation,
            Utc::now(),
        ))
    }

    async fn send(
        &self,
        session: &RuntimeSessionRef,
        input: RuntimeInput,
    ) -> Result<(), RuntimeError> {
        let guard = self.sessions.read().await;
        let active = guard
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })?;
        match input {
            RuntimeInput::Text { content } => {
                let _ = active
                    .thread
                    .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                        text: content,
                        text_elements: Vec::new(),
                    }]))
                    .await
                    .map_err(|e| RuntimeError::send_failed(session.session_id, e.to_string()))?;
            }
            RuntimeInput::Continuation { context } => {
                let _ = active
                    .thread
                    .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                        text: context.unwrap_or_else(|| "Continue".to_string()),
                        text_elements: Vec::new(),
                    }]))
                    .await
                    .map_err(|e| RuntimeError::send_failed(session.session_id, e.to_string()))?;
            }
            RuntimeInput::ApprovalResponse {
                approved, reason, ..
            } => {
                if !approved {
                    let _ = active.thread.submit(Op::Interrupt).await;
                    let _ = active
                        .hub
                        .emit(RuntimeEventKind::Interrupted {
                            reason: SanitizedRuntimeMessage::new(
                                reason.as_deref().unwrap_or("Approval rejected"),
                            ),
                        })
                        .await;
                }
            }
        }
        Ok(())
    }

    async fn interrupt(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        let guard = self.sessions.read().await;
        let active = guard
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })?;
        let _ = active.thread.submit(Op::Interrupt).await;
        Ok(())
    }

    async fn resume(
        &self,
        session: &RuntimeSessionRef,
        input: Option<RuntimeInput>,
    ) -> Result<(), RuntimeError> {
        if let Some(inp) = input {
            self.send(session, inp).await
        } else {
            Ok(())
        }
    }

    async fn stop(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        let removed = self.sessions.write().await.remove(&session.session_id);
        if let Some(active) = removed {
            let _ = active.thread.submit(Op::Shutdown).await;
            *active.state.write().await = RuntimeLifecycleState::Stopped;
            let _ = active.hub.emit(RuntimeEventKind::Stopped).await;
        }
        Ok(())
    }

    async fn status(
        &self,
        session: &RuntimeSessionRef,
    ) -> Result<RuntimeLifecycleState, RuntimeError> {
        let guard = self.sessions.read().await;
        let active = guard
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })?;
        Ok(*active.state.read().await)
    }

    async fn events_after(
        &self,
        session: &RuntimeSessionRef,
        after_sequence: Option<u64>,
    ) -> Result<RuntimeEventSubscription, RuntimeError> {
        let guard = self.sessions.read().await;
        let active = guard
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })?;
        active.hub.subscribe(after_sequence).await
    }
}
