use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

use agent_studios_provider::id::ProviderInstanceId;
use agent_studios_provider::instance::ProviderInstance;
use agent_studios_provider::protocol::ProtocolFamily;
use codex_api::{ApiError, ResponseStream, ResponsesApiRequest};
use codex_model_provider::{ModelInferenceBackend, ModelInferenceContext, ModelProviderFuture};

use crate::auth::ResolvedAuth;
use crate::drivers::ProtocolDriver;
use crate::drivers::anthropic::AnthropicDriver;
use crate::drivers::chat_completions::ChatCompletionsDriver;
use crate::drivers::gemini::GeminiDriver;
use crate::error::TransportError;
use crate::secret::SecretResolver;
use crate::state::ContinuationManager;

/// Central runtime router dispatching model execution requests to configured provider instances
/// and wire protocol drivers. Implements Codex's `ModelInferenceBackend` trait.
pub struct RuntimeRouter {
    client: reqwest::Client,
    provider_instances: RwLock<HashMap<ProviderInstanceId, ProviderInstance>>,
    active_instance_id: RwLock<Option<ProviderInstanceId>>,
    model_routes: RwLock<HashMap<String, ProviderInstanceId>>,
    secret_resolver: Arc<dyn SecretResolver>,
    continuation_manager: Arc<ContinuationManager>,
    chat_driver: Arc<ChatCompletionsDriver>,
    anthropic_driver: Arc<AnthropicDriver>,
    gemini_driver: Arc<GeminiDriver>,
}

impl fmt::Debug for RuntimeRouter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeRouter")
            .field("active_instance_id", &self.active_instance_id)
            .field("model_routes", &self.model_routes)
            .field("continuation_manager", &self.continuation_manager)
            .finish()
    }
}

impl RuntimeRouter {
    pub fn new(
        secret_resolver: Arc<dyn SecretResolver>,
        continuation_manager: Arc<ContinuationManager>,
    ) -> Self {
        Self {
            client: reqwest::Client::builder().build().unwrap_or_default(),
            provider_instances: RwLock::new(HashMap::new()),
            active_instance_id: RwLock::new(None),
            model_routes: RwLock::new(HashMap::new()),
            secret_resolver,
            continuation_manager,
            chat_driver: Arc::new(ChatCompletionsDriver::new()),
            anthropic_driver: Arc::new(AnthropicDriver::new()),
            gemini_driver: Arc::new(GeminiDriver::new()),
        }
    }

    /// Registers a provider instance.
    pub fn register_instance(&self, instance: ProviderInstance) {
        let mut map = self.provider_instances.write().unwrap();
        map.insert(instance.id, instance);
    }

    /// Sets the default active provider instance.
    pub fn set_active_instance(&self, id: ProviderInstanceId) {
        let mut active = self.active_instance_id.write().unwrap();
        *active = Some(id);
    }

    /// Binds a specific model slug to a provider instance.
    pub fn route_model(&self, model: impl Into<String>, instance_id: ProviderInstanceId) {
        let mut routes = self.model_routes.write().unwrap();
        routes.insert(model.into(), instance_id);
    }

    /// Resolves the provider instance for an incoming request.
    pub fn resolve_instance(
        &self,
        request: &ResponsesApiRequest,
    ) -> Result<ProviderInstance, TransportError> {
        let routes = self.model_routes.read().unwrap();
        let target_id = if let Some(id) = routes.get(&request.model) {
            Some(*id)
        } else {
            let active = self.active_instance_id.read().unwrap();
            *active
        };

        let target_id = target_id.ok_or_else(|| {
            TransportError::ProviderNotFound(format!(
                "No provider instance configured or routed for model '{}'",
                request.model
            ))
        })?;

        let instances = self.provider_instances.read().unwrap();
        instances.get(&target_id).cloned().ok_or_else(|| {
            TransportError::ProviderNotFound(format!(
                "Provider instance '{target_id}' was not found in registry"
            ))
        })
    }

    /// Returns a reference to the continuation manager.
    pub fn continuation_manager(&self) -> &Arc<ContinuationManager> {
        &self.continuation_manager
    }

    /// Returns a reference to the secret resolver.
    pub fn secret_resolver(&self) -> &Arc<dyn SecretResolver> {
        &self.secret_resolver
    }
}

impl ModelInferenceBackend for RuntimeRouter {
    fn stream<'a>(
        &'a self,
        request: ResponsesApiRequest,
        context: ModelInferenceContext,
    ) -> ModelProviderFuture<'a, Result<ResponseStream, ApiError>> {
        Box::pin(async move {
            let instance = self.resolve_instance(&request).map_err(ApiError::from)?;
            if !instance.enabled {
                return Err(ApiError::Stream(format!(
                    "Provider instance '{}' is currently disabled",
                    instance.id
                )));
            }

            let auth = ResolvedAuth::resolve(&instance.authentication, &self.secret_resolver)
                .await
                .map_err(ApiError::from)?;

            let tx = self
                .continuation_manager
                .begin_transaction(&context.thread_id);

            match &instance.protocol {
                ProtocolFamily::OpenAiChatCompletions => self
                    .chat_driver
                    .stream(&self.client, &instance, &auth, request, context, tx)
                    .await
                    .map_err(ApiError::from),
                ProtocolFamily::AnthropicMessages => self
                    .anthropic_driver
                    .stream(&self.client, &instance, &auth, request, context, tx)
                    .await
                    .map_err(ApiError::from),
                ProtocolFamily::GeminiGenerateContent => self
                    .gemini_driver
                    .stream(&self.client, &instance, &auth, request, context, tx)
                    .await
                    .map_err(ApiError::from),
                ProtocolFamily::OpenAiResponses => Err(ApiError::Stream(
                    "OpenAiResponses wire protocol handled directly by Codex native transport"
                        .to_string(),
                )),
                ProtocolFamily::Custom(name) => Err(ApiError::Stream(format!(
                    "Custom protocol family '{name}' is not supported by runtime transport"
                ))),
            }
        })
    }
}
