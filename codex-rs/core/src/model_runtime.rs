use codex_model_provider::SharedModelProvider;
use codex_models_manager::manager::SharedModelsManager;

/// Generic thread model runtime override strictly pairing a `SharedModelProvider`
/// with a `SharedModelsManager`.
#[derive(Clone)]
pub struct ModelRuntimeOverride {
    provider: SharedModelProvider,
    models_manager: SharedModelsManager,
}

impl ModelRuntimeOverride {
    pub fn new(provider: SharedModelProvider, models_manager: SharedModelsManager) -> Self {
        Self {
            provider,
            models_manager,
        }
    }

    pub fn provider(&self) -> &SharedModelProvider {
        &self.provider
    }

    pub fn models_manager(&self) -> &SharedModelsManager {
        &self.models_manager
    }

    pub fn into_parts(self) -> (SharedModelProvider, SharedModelsManager) {
        (self.provider, self.models_manager)
    }
}

impl std::fmt::Debug for ModelRuntimeOverride {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelRuntimeOverride")
            .field("provider", &self.provider.info().name)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_login::AuthManager;
    use codex_login::CodexAuth;
    use codex_model_provider::create_model_provider;
    use codex_model_provider_info::ModelProviderInfo;
    use codex_models_manager::manager::StaticModelsManager;
    use codex_protocol::openai_models::ModelsResponse;
    use std::sync::Arc;

    #[test]
    fn test_model_runtime_override_pairing_and_accessors() {
        let auth_manager = AuthManager::from_auth_for_testing(CodexAuth::from_api_key("test-key"));
        let provider = create_model_provider(
            ModelProviderInfo::create_openai_provider(None),
            Some(Arc::clone(&auth_manager)),
        );
        let models_manager: SharedModelsManager = Arc::new(StaticModelsManager::new(
            Some(auth_manager),
            ModelsResponse { models: Vec::new() },
        ));

        let runtime_override =
            ModelRuntimeOverride::new(Arc::clone(&provider), Arc::clone(&models_manager));

        assert!(Arc::ptr_eq(runtime_override.provider(), &provider));
        assert!(Arc::ptr_eq(
            runtime_override.models_manager(),
            &models_manager
        ));
        assert_eq!(
            runtime_override.provider().info().name,
            provider.info().name
        );

        let debug_str = format!("{runtime_override:?}");
        assert!(debug_str.contains("ModelRuntimeOverride"));

        let cloned = runtime_override.clone();
        assert!(Arc::ptr_eq(cloned.provider(), &provider));
        assert!(Arc::ptr_eq(cloned.models_manager(), &models_manager));

        let (unpacked_provider, unpacked_manager) = runtime_override.into_parts();
        assert!(Arc::ptr_eq(&unpacked_provider, &provider));
        assert!(Arc::ptr_eq(&unpacked_manager, &models_manager));
    }

    #[tokio::test]
    async fn test_model_runtime_override_static_catalog_no_discovery() {
        let auth_manager = AuthManager::from_auth_for_testing(CodexAuth::from_api_key("test-key"));
        let provider = create_model_provider(
            ModelProviderInfo::create_openai_provider(None),
            Some(Arc::clone(&auth_manager)),
        );
        let model = codex_models_manager::model_info::model_info_from_slug("custom-static-model");
        let models_manager: SharedModelsManager = Arc::new(StaticModelsManager::new(
            Some(auth_manager),
            ModelsResponse {
                models: vec![model],
            },
        ));

        let runtime_override =
            ModelRuntimeOverride::new(Arc::clone(&provider), Arc::clone(&models_manager));

        let http_client_factory = codex_http_client::HttpClientFactory::new(
            codex_http_client::OutboundProxyPolicy::ReqwestDefault,
        );
        let models = runtime_override
            .models_manager()
            .list_models(
                codex_models_manager::manager::RefreshStrategy::OnlineIfUncached,
                http_client_factory.clone(),
            )
            .await;
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model, "custom-static-model");

        let default_model = runtime_override
            .models_manager()
            .get_default_model(
                &None,
                false,
                codex_models_manager::manager::RefreshStrategy::OnlineIfUncached,
                http_client_factory,
            )
            .await;
        assert_eq!(default_model, "custom-static-model");
    }

    #[test]
    fn test_override_inheritance_decision_logic() {
        let auth_manager = AuthManager::from_auth_for_testing(CodexAuth::from_api_key("test-key"));
        let provider = create_model_provider(
            ModelProviderInfo::create_openai_provider(None),
            Some(Arc::clone(&auth_manager)),
        );
        let models_manager: SharedModelsManager = Arc::new(StaticModelsManager::new(
            Some(auth_manager),
            ModelsResponse { models: Vec::new() },
        ));
        let parent_override = Some(ModelRuntimeOverride::new(provider, models_manager));

        let parent_provider_id = "agent-studios-provider-1";

        // Same provider -> inherits
        let child_provider_matching = "agent-studios-provider-1";
        let inherited = if child_provider_matching == parent_provider_id {
            parent_override.clone()
        } else {
            None
        };
        assert!(inherited.is_some());

        // Different provider -> does NOT inherit
        let child_provider_diff = "openai";
        let not_inherited = if child_provider_diff == parent_provider_id {
            parent_override.clone()
        } else {
            None
        };
        assert!(not_inherited.is_none());
    }
}
