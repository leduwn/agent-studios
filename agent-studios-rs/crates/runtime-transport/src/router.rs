use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

use agent_studios_provider::capabilities::CapabilitySupport;
use agent_studios_provider::id::ProviderInstanceId;
use agent_studios_provider::instance::ProviderInstance;
use agent_studios_provider::model::ModelDescriptor;
use agent_studios_provider::protocol::ProtocolFamily;
use codex_api::{ApiError, ResponseStream, ResponsesApiRequest};
use codex_model_provider::{ModelInferenceBackend, ModelInferenceContext, ModelProviderFuture};

use crate::auth::ResolvedAuth;
use crate::diagnostic::{
    DiagnosticLevel, NoopRuntimeDiagnosticSink, RuntimeDiagnostic, RuntimeDiagnosticSink,
};
use crate::drivers::ProtocolDriver;
use crate::drivers::anthropic::AnthropicDriver;
use crate::drivers::chat_completions::ChatCompletionsDriver;
use crate::drivers::gemini::GeminiDriver;
use crate::error::TransportError;
use crate::options::RuntimeTransportOptions;
use crate::secret::SecretResolver;
use crate::state::{ContinuationKey, ContinuationManager};

/// Route definition binding a model identifier to a target provider instance and its descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeModelRoute {
    pub instance_id: ProviderInstanceId,
    pub descriptor: ModelDescriptor,
}

impl RuntimeModelRoute {
    pub fn new(instance_id: ProviderInstanceId, descriptor: ModelDescriptor) -> Self {
        Self {
            instance_id,
            descriptor,
        }
    }
}

/// Central runtime router dispatching model execution requests to configured provider instances
/// and wire protocol drivers. Implements Codex's `ModelInferenceBackend` trait.
pub struct RuntimeRouter {
    client: reqwest::Client,
    options: RuntimeTransportOptions,
    diagnostic_sink: Arc<dyn RuntimeDiagnosticSink>,
    provider_instances: RwLock<HashMap<ProviderInstanceId, ProviderInstance>>,
    active_instance_id: RwLock<Option<ProviderInstanceId>>,
    model_routes: RwLock<HashMap<String, RuntimeModelRoute>>,
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

pub const AGENT_STUDIOS_USER_AGENT: &str = "AgentStudios/dev";

impl RuntimeRouter {
    pub fn try_new(
        secret_resolver: Arc<dyn SecretResolver>,
        continuation_manager: Arc<ContinuationManager>,
    ) -> Result<Self, TransportError> {
        Self::try_new_with_options(
            secret_resolver,
            continuation_manager,
            RuntimeTransportOptions::default(),
            Arc::new(NoopRuntimeDiagnosticSink),
        )
    }

    pub fn try_new_with_options(
        secret_resolver: Arc<dyn SecretResolver>,
        continuation_manager: Arc<ContinuationManager>,
        options: RuntimeTransportOptions,
        diagnostic_sink: Arc<dyn RuntimeDiagnosticSink>,
    ) -> Result<Self, TransportError> {
        let client = reqwest::Client::builder()
            .connect_timeout(options.connect_timeout)
            .user_agent(AGENT_STUDIOS_USER_AGENT)
            .build()
            .map_err(|e| TransportError::HttpClientBuildError(e.to_string()))?;

        Ok(Self {
            client,
            options,
            diagnostic_sink,
            provider_instances: RwLock::new(HashMap::new()),
            active_instance_id: RwLock::new(None),
            model_routes: RwLock::new(HashMap::new()),
            secret_resolver,
            continuation_manager,
            chat_driver: Arc::new(ChatCompletionsDriver::new()),
            anthropic_driver: Arc::new(AnthropicDriver::new()),
            gemini_driver: Arc::new(GeminiDriver::new()),
        })
    }

    pub fn new(
        secret_resolver: Arc<dyn SecretResolver>,
        continuation_manager: Arc<ContinuationManager>,
    ) -> Result<Self, TransportError> {
        Self::try_new(secret_resolver, continuation_manager)
    }

    pub fn new_with_options(
        secret_resolver: Arc<dyn SecretResolver>,
        continuation_manager: Arc<ContinuationManager>,
        options: RuntimeTransportOptions,
        diagnostic_sink: Arc<dyn RuntimeDiagnosticSink>,
    ) -> Result<Self, TransportError> {
        Self::try_new_with_options(
            secret_resolver,
            continuation_manager,
            options,
            diagnostic_sink,
        )
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

    /// Registers an explicit route binding a model slug to an instance and its ModelDescriptor.
    pub fn register_route(
        &self,
        model: impl Into<String>,
        route: RuntimeModelRoute,
    ) -> Result<(), TransportError> {
        let model_str = model.into();
        if route.instance_id != route.descriptor.provider_instance_id {
            return Err(TransportError::InvalidModelRoute(format!(
                "Route instance_id '{}' does not match descriptor provider_instance_id '{}'",
                route.instance_id, route.descriptor.provider_instance_id
            )));
        }

        if route.descriptor.id.as_str() != model_str {
            return Err(TransportError::InvalidModelRoute(format!(
                "Model key '{model_str}' does not match descriptor id '{}'",
                route.descriptor.id.as_str()
            )));
        }

        let instances = self.provider_instances.read().unwrap();
        if !instances.contains_key(&route.instance_id) {
            return Err(TransportError::ProviderNotFound(format!(
                "Provider instance '{}' was not found in registry",
                route.instance_id
            )));
        }

        let mut routes = self.model_routes.write().unwrap();
        routes.insert(model_str, route);
        Ok(())
    }

    /// Convenience method to register a model descriptor and bind it to an instance.
    pub fn register_model(
        &self,
        model: impl Into<String>,
        instance_id: ProviderInstanceId,
        descriptor: ModelDescriptor,
    ) -> Result<(), TransportError> {
        self.register_route(model, RuntimeModelRoute::new(instance_id, descriptor))
    }

    /// Resolves the provider instance and model descriptor for an incoming request.
    pub fn resolve_route(
        &self,
        request: &ResponsesApiRequest,
    ) -> Result<(ProviderInstance, ModelDescriptor), TransportError> {
        let routes = self.model_routes.read().unwrap();
        if let Some(route) = routes.get(&request.model) {
            let instances = self.provider_instances.read().unwrap();
            let instance = instances.get(&route.instance_id).cloned().ok_or_else(|| {
                TransportError::ProviderNotFound(format!(
                    "Provider instance '{}' was not found in registry",
                    route.instance_id
                ))
            })?;
            return Ok((instance, route.descriptor.clone()));
        }

        Err(TransportError::ModelRouteNotFound {
            model: request.model.clone(),
        })
    }

    /// Validates requested capabilities against the model's declared capability profile.
    pub fn gate_capabilities(
        &self,
        request: &ResponsesApiRequest,
        descriptor: &ModelDescriptor,
    ) -> Result<(), TransportError> {
        if request.stream {
            match descriptor.capabilities.streaming {
                CapabilitySupport::Unsupported => {
                    return Err(TransportError::UnsupportedCapability {
                        feature: "streaming",
                        model: request.model.clone(),
                    });
                }
                CapabilitySupport::Unknown => {
                    self.diagnostic_sink.emit(RuntimeDiagnostic {
                        level: DiagnosticLevel::Warning,
                        message: format!(
                            "Model '{}' has unknown support for streaming; proceeding with optimism",
                            request.model
                        ),
                        model: Some(request.model.clone()),
                        provider_instance_id: Some(descriptor.provider_instance_id.to_string()),
                    });
                }
                CapabilitySupport::Supported => {}
            }
        }

        let has_tools = match &request.tools {
            Some(t) => {
                let s = serde_json::to_string(t).unwrap_or_default();
                let trimmed = s.trim();
                !trimmed.is_empty() && trimmed != "[]" && trimmed != "null"
            }
            None => false,
        };

        if has_tools {
            match descriptor.capabilities.tool_calling {
                CapabilitySupport::Unsupported => {
                    return Err(TransportError::UnsupportedCapability {
                        feature: "tool_calling",
                        model: request.model.clone(),
                    });
                }
                CapabilitySupport::Unknown => {
                    self.diagnostic_sink.emit(RuntimeDiagnostic {
                        level: DiagnosticLevel::Warning,
                        message: format!(
                            "Model '{}' has unknown support for tool_calling; proceeding with optimism",
                            request.model
                        ),
                        model: Some(request.model.clone()),
                        provider_instance_id: Some(descriptor.provider_instance_id.to_string()),
                    });
                }
                CapabilitySupport::Supported => {}
            }
        }

        if request.parallel_tool_calls {
            match descriptor.capabilities.parallel_tool_calls {
                CapabilitySupport::Unsupported => {
                    return Err(TransportError::UnsupportedCapability {
                        feature: "parallel_tool_calls",
                        model: request.model.clone(),
                    });
                }
                CapabilitySupport::Unknown => {
                    self.diagnostic_sink.emit(RuntimeDiagnostic {
                        level: DiagnosticLevel::Warning,
                        message: format!(
                            "Model '{}' has unknown support for parallel_tool_calls; proceeding with optimism",
                            request.model
                        ),
                        model: Some(request.model.clone()),
                        provider_instance_id: Some(descriptor.provider_instance_id.to_string()),
                    });
                }
                CapabilitySupport::Supported => {}
            }
        }

        Ok(())
    }

    /// Returns a reference to the continuation manager.
    pub fn continuation_manager(&self) -> &Arc<ContinuationManager> {
        &self.continuation_manager
    }

    /// Returns a reference to the secret resolver.
    pub fn secret_resolver(&self) -> &Arc<dyn SecretResolver> {
        &self.secret_resolver
    }

    /// Returns a reference to the transport options.
    pub fn options(&self) -> &RuntimeTransportOptions {
        &self.options
    }
}

impl ModelInferenceBackend for RuntimeRouter {
    fn stream<'a>(
        &'a self,
        request: ResponsesApiRequest,
        context: ModelInferenceContext,
    ) -> ModelProviderFuture<'a, Result<ResponseStream, ApiError>> {
        Box::pin(async move {
            let (instance, descriptor) = self.resolve_route(&request).map_err(ApiError::from)?;
            if !instance.enabled {
                return Err(ApiError::Stream(format!(
                    "Provider instance '{}' is currently disabled",
                    instance.id
                )));
            }

            self.gate_capabilities(&request, &descriptor)
                .map_err(ApiError::from)?;

            let auth = ResolvedAuth::resolve(&instance.authentication, &self.secret_resolver)
                .await
                .map_err(ApiError::from)?;

            let key = ContinuationKey::new(instance.id, &context.thread_id);
            let tx = self
                .continuation_manager
                .begin_transaction(key)
                .map_err(ApiError::from)?;

            match &instance.protocol {
                ProtocolFamily::OpenAiChatCompletions => self
                    .chat_driver
                    .stream(
                        &self.client,
                        &instance,
                        &descriptor,
                        &auth,
                        request,
                        context,
                        tx,
                        &self.options,
                    )
                    .await
                    .map_err(ApiError::from),
                ProtocolFamily::AnthropicMessages => self
                    .anthropic_driver
                    .stream(
                        &self.client,
                        &instance,
                        &descriptor,
                        &auth,
                        request,
                        context,
                        tx,
                        &self.options,
                    )
                    .await
                    .map_err(ApiError::from),
                ProtocolFamily::GeminiGenerateContent => self
                    .gemini_driver
                    .stream(
                        &self.client,
                        &instance,
                        &descriptor,
                        &auth,
                        request,
                        context,
                        tx,
                        &self.options,
                    )
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
