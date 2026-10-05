use std::sync::Arc;

use codex_config::LoaderOverrides;
use codex_core::config::{Config, ConfigBuilder};
use codex_core::{
    CodexAppsToolsCache, ThreadManager, init_state_db, passthrough_image_store,
    resolve_installation_id, thread_store_from_config,
};
use codex_exec_server::EnvironmentManager;
use codex_extension_api::ExtensionRegistry;
use codex_home::CodexHomeUserInstructionsProvider;
use codex_login::AuthManager;
use codex_protocol::protocol::SessionSource;
use codex_utils_absolute_path::AbsolutePathBuf;

use crate::contributor::build_agent_studios_extension_registry;
use crate::error::InternalAgentError;

/// Factory for constructing production ThreadManager instances configured with
/// Agent Studios extension registry and lifecycle contributors.
pub struct AgentStudiosCodexRuntimeFactory;

impl AgentStudiosCodexRuntimeFactory {
    /// Builds a test Config configured for an isolated codex_home directory.
    pub async fn create_test_config(
        codex_home: &std::path::Path,
    ) -> Result<Config, InternalAgentError> {
        let mut config = ConfigBuilder::default()
            .loader_overrides(LoaderOverrides::without_managed_config_for_tests())
            .codex_home(codex_home.to_path_buf())
            .build()
            .await
            .map_err(|e| InternalAgentError::Other(e.to_string()))?;

        config.cwd = AbsolutePathBuf::from_absolute_path(codex_home)
            .map_err(|e| InternalAgentError::Other(e.to_string()))?;

        Ok(config)
    }

    /// Builds a ThreadManager configured with Agent Studios extensions and default Exec session source.
    pub async fn build_thread_manager(
        config: &Config,
        auth_manager: Arc<AuthManager>,
        session_source: Option<SessionSource>,
    ) -> Result<ThreadManager, InternalAgentError> {
        let extensions = Arc::new(build_agent_studios_extension_registry());
        Self::build_thread_manager_with_extensions(config, auth_manager, session_source, extensions)
            .await
    }

    /// Builds a ThreadManager with an explicit extension registry (which must have
    /// AgentStudiosToolLifecycleContributor registered).
    pub async fn build_thread_manager_with_extensions(
        config: &Config,
        auth_manager: Arc<AuthManager>,
        session_source: Option<SessionSource>,
        extensions: Arc<ExtensionRegistry<Config>>,
    ) -> Result<ThreadManager, InternalAgentError> {
        let state_db = init_state_db(config).await;
        let thread_store = thread_store_from_config(config, state_db);
        let installation_id = resolve_installation_id(&config.codex_home)
            .await
            .unwrap_or_else(|_| "agent-studios-installation-id".to_string());
        let user_instructions_provider = Arc::new(CodexHomeUserInstructionsProvider::new(
            config.codex_home.clone(),
        ));
        let models_manager = codex_core::build_models_manager(config, auth_manager.clone());
        let environment_manager = Arc::new(EnvironmentManager::default_for_tests());
        let image_store = passthrough_image_store();

        let manager = ThreadManager::new(
            config,
            auth_manager,
            models_manager,
            CodexAppsToolsCache::default(),
            session_source.unwrap_or(SessionSource::Exec),
            environment_manager,
            extensions,
            user_instructions_provider,
            None,
            image_store,
            thread_store,
            None,
            installation_id,
            None,
            None,
        );

        Ok(manager)
    }
}

/// Convenience function constructing ThreadManager with Agent Studios extension registry.
pub async fn build_agent_studios_thread_manager(
    config: &Config,
    auth_manager: Arc<AuthManager>,
) -> Result<ThreadManager, InternalAgentError> {
    AgentStudiosCodexRuntimeFactory::build_thread_manager(config, auth_manager, None).await
}
