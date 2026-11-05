pub mod auth;
pub mod builtin;
pub mod capabilities;
pub mod catalog;
pub mod definition;
pub mod discovery;
pub mod endpoint;
pub mod error;
pub mod id;
pub mod importer;
pub mod instance;
pub mod model;
pub mod protocol;
pub mod registry;
pub mod secret;
pub mod snapshot;
pub mod store;

pub use auth::AuthenticationScheme;
pub use builtin::builtin_definitions;
pub use capabilities::{CapabilitySupport, ModelCapabilities};
pub use catalog::ProviderCatalog;
pub use definition::ProviderDefinition;
pub use discovery::{
    CheckDetail, CheckState, DiscoveryConfig, DiscoveryError, ModelDiscoveryClient,
    ProviderConnectionReport,
};
pub use endpoint::EndpointProfile;
pub use error::ProviderError;
pub use id::{ModelId, ProviderId, ProviderInstanceId};
pub use importer::{
    AgentStudiosSnapshotImporter, CodexConfigImporter, ConflictResolutionStrategy, ImportConflict,
    ImportPreview, ImportResult, OpenCodeConfigImporter, ProviderConfigImporter, SourceFormat,
    apply_import, preview_import,
};
pub use instance::ProviderInstance;
pub use model::{ModelDescriptor, ModelLimits, ModelMetadataSource, ModelRef};
pub use protocol::ProtocolFamily;
pub use registry::{ModelRegistry, ProviderRegistry};
pub use secret::{
    SecretBackend, SecretReference, locator_for_provider_key, secret_ref_for_provider_key,
};
pub use snapshot::{PROVIDER_CATALOG_SCHEMA_VERSION, ProviderCatalogSnapshot};
pub use store::{
    CompositeSecretStore, DEFAULT_KEYRING_SERVICE, DefaultSecretResolver, EnvironmentSecretStore,
    InMemorySecretStore, OsKeyringSecretStore, SecretStore,
};
